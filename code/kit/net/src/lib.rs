//! Requests that only reach the addresses they may.
//!
//! A server that fetches an address someone else gave it (a picture's, a
//! newsletter's way out, a runner's, a health check's) must not be led to
//! this machine, a private network or the cloud's metadata service. Looking
//! at the name in the address isn't enough: a public name can resolve to
//! `127.0.0.1` or `169.254.169.254`, and a public page can redirect there.
//! So the check is made on the addresses a name resolves to, as the
//! connection is made ([`Resolver`]), and again on every redirect
//! ([`redirects`]). [`client`] builds a reqwest client with both.
//!
//! A [`Reach`] says where requests may go: [`Reach::Public`] only to the
//! internet, [`Reach::OwnNetworks`] to this machine and private networks
//! too, for an address an admin gave on purpose, but never to a link-local
//! address, where cloud metadata answers.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use reqwest::redirect::Policy;
use url::{Host, Url};

/// The most redirects [`redirects`] follows.
pub const REDIRECTS: usize = 5;

/// Where requests may go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reach {
    /// Only addresses on the internet: never this machine, a private
    /// network (a tailnet's too), a link, or one kept for documentation,
    /// benchmarks or later.
    Public,
    /// The internet, this machine and private networks, but never a
    /// link-local address or cloud metadata: for an address an admin gives
    /// on purpose, such as an app on this Mac or the company's network.
    OwnNetworks,
}

impl Reach {
    /// Whether a connection may go to `ip`.
    pub fn allows(self, ip: IpAddr) -> bool {
        match self {
            Reach::Public => public_ip(ip),
            Reach::OwnNetworks => !link_or_metadata(ip),
        }
    }

    /// Whether a request may go to `url`: http or https, with a host this
    /// reach allows. An address in the URL is checked here; a name only by
    /// what it says (`localhost`, `metadata.google.internal`), since what
    /// it resolves to is [`Resolver`]'s to check.
    pub fn allows_url(self, url: &Url) -> bool {
        if !matches!(url.scheme(), "http" | "https") {
            return false;
        }
        // The host as the HTTP client reads it: `2852039166`,
        // `0xa9.0xfe.0xa9.0xfe` and `x@169.254.169.254` are addresses.
        match url.host() {
            Some(Host::Ipv4(ip)) => self.allows(ip.into()),
            Some(Host::Ipv6(ip)) => self.allows(ip.into()),
            Some(Host::Domain(name)) => self.allows_name(name),
            None => false,
        }
    }

    /// Whether a name may be looked up at all: not one that only ever
    /// names this machine, a local network, or the metadata service.
    fn allows_name(self, name: &str) -> bool {
        let name = name.trim_end_matches('.').to_ascii_lowercase();
        let is = |suffix: &str| name == suffix || name.ends_with(&format!(".{suffix}"));
        match self {
            Reach::Public => !["localhost", "local", "internal", "home.arpa"]
                .into_iter()
                .any(is),
            Reach::OwnNetworks => !is("metadata.google.internal"),
        }
    }
}

/// Whether `url` is a public http(s) address: [`Reach::Public`]'s check.
pub fn public(url: &Url) -> bool {
    Reach::Public.allows_url(url)
}

/// An address on the internet: not this machine's, a private network's (a
/// tailnet's too), a link's, a multicast group's, or one kept for
/// documentation, benchmarks or later. IPv4 carried in IPv6 is judged as
/// itself.
pub fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => public_v4(ip),
        IpAddr::V6(ip) => match carried_v4(ip) {
            Some(v4) => public_v4(v4),
            None => public_v6(ip),
        },
    }
}

/// A link-local address, where cloud metadata answers (169.254.169.254,
/// `fe80::/10`), or another cloud's metadata address (Alibaba's
/// 100.100.100.200, AWS's `fd00:ec2::254`).
pub fn link_or_metadata(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => link_or_metadata_v4(ip),
        IpAddr::V6(ip) => match carried_v4(ip) {
            Some(v4) => link_or_metadata_v4(v4),
            None => {
                ip.is_unicast_link_local()
                    || ip == Ipv6Addr::new(0xfd00, 0xec2, 0, 0, 0, 0, 0, 0x254)
            }
        },
    }
}

fn link_or_metadata_v4(ip: Ipv4Addr) -> bool {
    ip.is_link_local() || ip == Ipv4Addr::new(100, 100, 100, 200)
}

fn public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    !(a == 0 // "this network", 0.0.0.0 among it
        || ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local() // 169.254/16, cloud metadata
        || (a == 100 && (64..128).contains(&b)) // 100.64/10: carrier NAT, tailnets
        || (a == 192 && b == 0 && c == 0) // 192.0.0/24: protocol assignments
        || ip.is_documentation()
        || (a == 198 && (18..20).contains(&b)) // 198.18/15: benchmarks
        || ip.is_multicast()
        || a >= 240) // reserved, and the broadcast address
}

fn public_v6(ip: Ipv6Addr) -> bool {
    let s = ip.segments();
    !(ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_multicast()
        || ip.is_unique_local() // fc00::/7
        || ip.is_unicast_link_local() // fe80::/10
        || s[0] & 0xffc0 == 0xfec0 // fec0::/10: the old site-local
        || s[..4] == [0x100, 0, 0, 0] // 100::/64: discard
        || s[..3] == [0x64, 0xff9b, 1] // 64:ff9b:1::/48: local NAT64
        || s[..2] == [0x2001, 0xdb8] // documentation
        || (s[0] == 0x3fff && s[1] < 0x1000) // 3fff::/20: documentation
        || s[..3] == [0x2001, 2, 0] // 2001:2::/48: benchmarks
        || (s[0] == 0x2002 && !public_v4(Ipv4Addr::new(
            (s[1] >> 8) as u8,
            s[1] as u8,
            (s[2] >> 8) as u8,
            s[2] as u8,
        )))) // 6to4 of an address that isn't public
}

/// The IPv4 address an IPv6 one carries: mapped (`::ffff:a.b.c.d`),
/// compatible (`::a.b.c.d`, `::1` and `::` among them) or NAT64's
/// (`64:ff9b::a.b.c.d`).
fn carried_v4(ip: Ipv6Addr) -> Option<Ipv4Addr> {
    let s = ip.segments();
    if let Some(v4) = ip.to_ipv4_mapped() {
        Some(v4)
    } else if s[..6] == [0; 6] || s[..6] == [0x64, 0xff9b, 0, 0, 0, 0] {
        Some(Ipv4Addr::from(ip.to_bits() as u32))
    } else {
        None
    }
}

/// Resolves a name only to the addresses its [`Reach`] allows, and the
/// connection goes to those: a name that leads elsewhere finds nothing.
/// An address in the URL is never resolved, so check it with
/// [`Reach::allows_url`]; [`client`] and [`redirects`] do for redirects.
#[derive(Debug, Clone, Copy)]
pub struct Resolver(pub Reach);

impl Resolve for Resolver {
    fn resolve(&self, name: Name) -> Resolving {
        let reach = self.0;
        let host = name.as_str().to_string();
        Box::pin(async move {
            if !reach.allows_name(&host) {
                return Err(format!("{host} isn't an address this request may go to").into());
            }
            // The port is the URL's; the client sets it on what this finds.
            let found = tokio::net::lookup_host((host.as_str(), 0)).await?;
            let allowed: Vec<SocketAddr> = found.filter(|a| reach.allows(a.ip())).collect();
            if allowed.is_empty() {
                return Err(format!("{host} leads nowhere this request may go").into());
            }
            Ok(Box::new(allowed.into_iter()) as Addrs)
        })
    }
}

/// Follows up to [`REDIRECTS`] redirects, each only where `reach` allows;
/// otherwise it stops and the redirect itself is the answer. A redirect to
/// an address in the URL never reaches the [`Resolver`], so this is what
/// checks it.
pub fn redirects(reach: Reach) -> Policy {
    Policy::custom(move |attempt| {
        if attempt.previous().len() > REDIRECTS || !reach.allows_url(attempt.url()) {
            attempt.stop()
        } else {
            attempt.follow()
        }
    })
}

/// A reqwest client builder whose requests only go where `reach` allows:
/// its names resolve through [`Resolver`], its redirects are checked by
/// [`redirects`], and it uses no proxy, since a proxy would resolve the
/// name itself. Add a timeout, a user agent, or `Policy::none()` for no
/// redirects at all. Check the first URL with [`Reach::allows_url`]: an
/// address in it never reaches the resolver.
pub fn client(reach: Reach) -> reqwest::ClientBuilder {
    reqwest::Client::builder()
        .dns_resolver(Resolver(reach))
        .redirect(redirects(reach))
        .no_proxy()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn ip(text: &str) -> IpAddr {
        text.parse().unwrap()
    }

    fn url(text: &str) -> Url {
        Url::parse(text).unwrap()
    }

    #[test]
    fn public_addresses_are_the_internet_only() {
        for public in [
            "1.1.1.1",
            "93.184.215.14",
            "100.63.255.255",
            "100.128.0.1",
            "172.32.0.1",
            "198.20.0.1",
            "2a00:1450:4001::200e",
            "2606:4700::1111",
            "::ffff:1.1.1.1",
            "64:ff9b::101:101",
            "2002:101:101::1",
        ] {
            assert!(public_ip(ip(public)), "{public}");
        }
        for not in [
            "0.0.0.0",
            "0.1.2.3",
            "127.0.0.1",
            "127.255.0.9",
            "10.0.0.1",
            "172.16.0.1",
            "192.168.1.4",
            "169.254.169.254",
            "100.64.0.1",
            "100.100.100.200",
            "100.127.255.255",
            "192.0.0.8",
            "192.0.2.1",
            "198.51.100.1",
            "203.0.113.1",
            "198.18.0.1",
            "198.19.255.255",
            "224.0.0.1",
            "239.255.255.250",
            "240.0.0.1",
            "255.255.255.255",
            "::",
            "::1",
            "::ffff:127.0.0.1",
            "::ffff:169.254.169.254",
            "::127.0.0.1",
            "::10.0.0.1",
            "64:ff9b::7f00:1",
            "64:ff9b::a9fe:a9fe",
            "64:ff9b:1::1",
            "fc00::1",
            "fd12::1",
            "fd00:ec2::254",
            "fe80::1",
            "febf::1",
            "fec0::1",
            "ff02::1",
            "100::1",
            "2001:db8::1",
            "3fff::1",
            "2001:2::1",
            "2002:7f00:1::1",
            "2002:a9fe:a9fe::1",
        ] {
            assert!(!public_ip(ip(not)), "{not}");
        }
    }

    #[test]
    fn own_networks_are_everything_but_links_and_metadata() {
        for ok in [
            "127.0.0.1",
            "::1",
            "10.0.0.1",
            "192.168.1.4",
            "100.101.102.103",
            "fd12::1",
            "1.1.1.1",
        ] {
            assert!(Reach::OwnNetworks.allows(ip(ok)), "{ok}");
        }
        for not in [
            "169.254.169.254",
            "169.254.0.1",
            "100.100.100.200",
            "fe80::1",
            "fd00:ec2::254",
            "::ffff:169.254.169.254",
            "::169.254.169.254",
            "64:ff9b::a9fe:a9fe",
        ] {
            assert!(!Reach::OwnNetworks.allows(ip(not)), "{not}");
        }
    }

    #[test]
    fn a_public_url_is_http_to_a_public_host() {
        for ok in [
            "https://www.zooplus.de/felix.jpg",
            "http://news.example/u",
            "https://[2a00:1450:4001::200e]/x.png",
        ] {
            assert!(public(&url(ok)), "{ok}");
        }
        for not in [
            "ftp://news.example/u",
            "mailto:leave@news.example",
            "https://localhost/x.png",
            "https://localhost./x.png",
            "https://app.localhost/x.png",
            "https://printer.local/x.png",
            "https://metadata.google.internal/",
            "https://router.home.arpa/",
            "https://127.0.0.1:7785/api",
            "https://192.168.1.4/x.png",
            "https://[::1]/x.png",
            "https://[::ffff:127.0.0.1]/x.png",
            "https://[fd12::1]/x.png",
            "https://[fe80::1]/x.png",
            "https://100.100.1.2/x.png",
            // The metadata address, as the URL parser reads these.
            "http://2852039166/",
            "http://0xa9.0xfe.0xa9.0xfe/",
            "http://x@169.254.169.254/",
            "http://169.254.169.254./",
        ] {
            assert!(!public(&url(not)), "{not}");
        }
    }

    #[test]
    fn own_networks_urls_reach_this_mac_but_not_metadata() {
        let ok = |a: &str| Reach::OwnNetworks.allows_url(&url(a));
        assert!(ok("http://127.0.0.1:9876/wake"));
        assert!(ok("http://localhost:7385/api/health"));
        assert!(ok("https://runner.example/wake"));
        assert!(ok("http://10.0.0.5/health"));
        assert!(!ok("http://169.254.169.254/latest"));
        assert!(!ok("http://metadata.google.internal/"));
        assert!(!ok("http://METADATA.google.internal./"));
        assert!(!ok("http://2852039166/"));
        assert!(!ok("http://[fe80::1]/"));
        assert!(!ok("http://[::ffff:169.254.169.254]/"));
        assert!(!ok("ftp://runner.example/"));
    }

    #[tokio::test]
    async fn a_name_resolves_only_to_what_its_reach_allows() {
        let name = || "localhost".parse::<Name>().unwrap();
        assert!(Resolver(Reach::Public).resolve(name()).await.is_err());
        let found: Vec<_> = Resolver(Reach::OwnNetworks)
            .resolve(name())
            .await
            .unwrap()
            .collect();
        assert!(!found.is_empty());
        assert!(found.iter().all(|a| a.ip().is_loopback()));
    }

    /// A server on this Mac answering every request with `answer`, and
    /// its port.
    async fn answering(answer: &'static str) -> u16 {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let mut request = [0; 4096];
                    let _ = stream.read(&mut request).await;
                    let _ = stream.write_all(answer.as_bytes()).await;
                    let _ = stream.shutdown().await;
                });
            }
        });
        port
    }

    #[tokio::test]
    async fn a_name_that_leads_here_doesnt_connect() {
        let port = answering("HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\nok").await;
        let public = client(Reach::Public).build().unwrap();
        assert!(
            public
                .get(format!("http://localhost:{port}/"))
                .send()
                .await
                .is_err()
        );
        let own = client(Reach::OwnNetworks).build().unwrap();
        let answer = own
            .get(format!("http://localhost:{port}/"))
            .send()
            .await
            .unwrap();
        assert_eq!(answer.status(), 200);
    }

    #[tokio::test]
    async fn a_redirect_to_an_address_it_may_not_reach_stops() {
        let port = answering(
            "HTTP/1.1 302 Found\r\nlocation: http://169.254.169.254/latest/meta-data\r\n\
             content-length: 0\r\n\r\n",
        )
        .await;
        let own = client(Reach::OwnNetworks).build().unwrap();
        let answer = own
            .get(format!("http://127.0.0.1:{port}/"))
            .send()
            .await
            .unwrap();
        assert_eq!(answer.status(), 302);
        assert_eq!(answer.url().as_str(), format!("http://127.0.0.1:{port}/"));
    }

    #[tokio::test]
    async fn a_redirect_it_may_follow_is_followed() {
        let to = answering("HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\nok").await;
        let answer: &'static str = format!(
            "HTTP/1.1 302 Found\r\nlocation: http://127.0.0.1:{to}/there\r\n\
             content-length: 0\r\n\r\n"
        )
        .leak();
        let from = answering(answer).await;
        let own = client(Reach::OwnNetworks).build().unwrap();
        let answer = own
            .get(format!("http://127.0.0.1:{from}/"))
            .send()
            .await
            .unwrap();
        assert_eq!(answer.status(), 200);
        assert_eq!(
            answer.url().as_str(),
            format!("http://127.0.0.1:{to}/there")
        );
    }
}
