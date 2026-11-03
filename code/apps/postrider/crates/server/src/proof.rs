//! Whether a mail is from who it says: a DKIM signature that holds, by the
//! sender's own organisation. Anyone can write a bank's address in From,
//! and the newsletters' rule (`judging.rs`) mustn't name a sender for mail
//! someone else sent in its name, or the sender's real notices would skip
//! the Inbox. The account's server doesn't say reliably what it checked
//! (Uberspace adds no Authentication-Results to most mail, and mail
//! forwarded from elsewhere carries the other server's), but a DKIM
//! signature survives the way: Postrider checks it itself, looking the key
//! up in DNS.

use std::sync::OnceLock;

use mail_auth::dns::DnsCache;
use mail_auth::{AuthenticatedMessage, DkimResult, MessageAuthenticator, NoCache, Parameters};

/// Whether this source is signed for `address`'s organisation, as DNS says
/// today. No when DNS can't be asked.
pub(crate) async fn proven(raw: &[u8], address: &str) -> bool {
    static CHECKER: OnceLock<Option<MessageAuthenticator>> = OnceLock::new();
    let checker = CHECKER.get_or_init(|| {
        MessageAuthenticator::new_system_conf()
            .inspect_err(|e| tracing::warn!(error = %e, "can't ask DNS for DKIM keys"))
            .ok()
    });
    match checker {
        Some(checker) => signed_for(checker, None::<&NoCache>, raw, address).await,
        None => false,
    }
}

/// Whether a signature in `raw` that holds is by `address`'s organisation,
/// covers its From, and that From is the mail's only one and `address`.
pub(crate) async fn signed_for<C: DnsCache>(
    checker: &MessageAuthenticator,
    cache: Option<&C>,
    raw: &[u8],
    address: &str,
) -> bool {
    let Some(message) = AuthenticatedMessage::parse(raw) else {
        return false;
    };
    // Two From fields: one is signed, the other may be the one shown.
    if message.from_addresses() != [address.to_lowercase()] {
        return false;
    }
    let Some((_, domain)) = address.rsplit_once('@') else {
        return false;
    };
    let params = Parameters {
        input: &message,
        cache,
    };
    checker.verify_dkim(params).await.iter().any(|output| {
        output.result() == &DkimResult::Pass
            && output.signature().is_some_and(|signature| {
                signature.h.iter().any(|h| h.eq_ignore_ascii_case("from"))
                    && same_organisation(&signature.d, domain)
            })
    })
}

/// Whether two domains are one organisation's, by the public suffix list:
/// `news.example.co.uk` and `example.co.uk` are, `example.co.uk` and
/// `other.co.uk` aren't.
fn same_organisation(a: &str, b: &str) -> bool {
    let (a, b) = (a.to_lowercase(), b.to_lowercase());
    match (
        psl::domain_str(a.trim_end_matches('.')),
        psl::domain_str(&b),
    ) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Borrow;
    use std::collections::HashMap;
    use std::hash::Hash;
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    use base64::Engine;
    use base64::engine::general_purpose::STANDARD;
    use mail_auth::TxtRecord;
    use mail_auth::crypto::Ed25519Key;
    use mail_auth::dkim::{DkimSigner, DomainKey};
    use mail_auth::dns::{ResolverCache, TxtRecordParser};
    use mail_auth::headers::HeaderWriter;

    use super::*;

    // RFC 8463's example key.
    const SEED: &str = "nWGxne/9WmC6hEr0kuwsxERJxWl7MmkZcDusAxyuf2A=";
    const PUBLIC: &str = "11qYAYKxCrfVS/7TyWQHOg7hcvPapiMlrwIaaPcHURo=";

    /// DNS that knows one key: `sel._domainkey.<domain>` for each domain.
    struct Keys(Mutex<HashMap<Box<str>, TxtRecord>>);

    impl ResolverCache<Box<str>, TxtRecord> for Keys {
        fn get<Q>(&self, name: &Q) -> Option<TxtRecord>
        where
            Box<str>: Borrow<Q>,
            Q: Hash + Eq + ?Sized,
        {
            self.0.lock().unwrap().get(name).cloned()
        }

        fn remove<Q>(&self, name: &Q) -> Option<TxtRecord>
        where
            Box<str>: Borrow<Q>,
            Q: Hash + Eq + ?Sized,
        {
            self.0.lock().unwrap().remove(name)
        }

        fn insert(&self, key: Box<str>, value: TxtRecord, _: Instant) {
            self.0.lock().unwrap().insert(key, value);
        }
    }

    impl DnsCache for Keys {
        type Txt = Keys;
        type Mx = NoCache;
        type Ipv4 = NoCache;
        type Ipv6 = NoCache;
        type Ptr = NoCache;

        fn txt(&self) -> Option<&Keys> {
            Some(self)
        }
        fn mx(&self) -> Option<&NoCache> {
            None
        }
        fn ipv4(&self) -> Option<&NoCache> {
            None
        }
        fn ipv6(&self) -> Option<&NoCache> {
            None
        }
        fn ptr(&self) -> Option<&NoCache> {
            None
        }
    }

    fn keys(domains: &[&str]) -> Keys {
        let record = format!("v=DKIM1; k=ed25519; p={PUBLIC}");
        let keys = Keys(Mutex::new(HashMap::new()));
        let until = Instant::now() + Duration::from_secs(3600);
        for domain in domains {
            let name = format!("sel._domainkey.{domain}.");
            let key = DomainKey::parse(record.as_bytes()).unwrap();
            keys.insert(name.into_boxed_str(), TxtRecord::from(key), until);
        }
        keys
    }

    fn signed(domain: &str, mail: &str) -> Vec<u8> {
        let seed = STANDARD.decode(SEED).unwrap();
        let public = STANDARD.decode(PUBLIC).unwrap();
        let key = Ed25519Key::from_seed_and_public_key(&seed, &public).unwrap();
        let signature = DkimSigner::from_key(key)
            .domain(domain)
            .selector("sel")
            .headers(["From", "Subject"])
            .sign(mail.as_bytes())
            .unwrap();
        format!("{}{mail}", signature.to_header()).into_bytes()
    }

    async fn proves(raw: &[u8], address: &str) -> bool {
        let checker = MessageAuthenticator::new_system_conf().unwrap();
        let dns = keys(&["bank.example", "mail.bank.example", "forger.example"]);
        signed_for(&checker, Some(&dns), raw, address).await
    }

    const MAIL: &str = "From: Bank <news@bank.example>\r\nSubject: Our news\r\n\r\nHello.\r\n";

    #[tokio::test]
    async fn a_mail_signed_by_its_senders_organisation_is_proven() {
        assert!(proves(&signed("bank.example", MAIL), "news@bank.example").await);
        assert!(proves(&signed("mail.bank.example", MAIL), "news@bank.example").await);
    }

    #[tokio::test]
    async fn a_mail_signed_by_someone_else_is_not() {
        assert!(!proves(&signed("forger.example", MAIL), "news@bank.example").await);
    }

    #[tokio::test]
    async fn a_mail_changed_after_signing_is_not() {
        let raw = signed("bank.example", MAIL);
        let changed = String::from_utf8(raw)
            .unwrap()
            .replace("Hello.", "Pay here.");
        assert!(!proves(changed.as_bytes(), "news@bank.example").await);
        let unsigned = MAIL.as_bytes();
        assert!(!proves(unsigned, "news@bank.example").await);
    }

    #[tokio::test]
    async fn a_second_from_or_another_address_is_not() {
        let raw = signed("bank.example", MAIL);
        let twice = [b"From: news@bank.example\r\n".as_slice(), &raw].concat();
        assert!(!proves(&twice, "news@bank.example").await);
        assert!(!proves(&raw, "alerts@bank.example").await);
    }

    #[test]
    fn organisations_go_by_the_public_suffix_list() {
        assert!(same_organisation("news.example.co.uk", "Example.co.uk"));
        assert!(!same_organisation("example.co.uk", "other.co.uk"));
        assert!(!same_organisation("sendgrid.net", "bank.example"));
    }
}
