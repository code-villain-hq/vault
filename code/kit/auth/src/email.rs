//! Shared email checks for sign-in linking.

/// Forge agents use this domain; `LinkPolicy::PeopleOnly` refuses it.
pub const AGENT_EMAIL_DOMAIN: &str = "agents.forge.invalid";

/// Trims and lowercases; refuses blanks, spaces, and addresses without a domain.
pub fn normalize_email(email: &str) -> Option<String> {
    let email = email.trim().to_lowercase();
    let valid = email.len() <= 254
        && !email.contains(char::is_whitespace)
        && email.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty() && domain.contains('.') && !domain.starts_with('.')
        });
    valid.then_some(email)
}

/// True when the address is a Forge agent (`@agents.forge.invalid` or a subdomain).
pub fn is_agent_email(email: &str) -> bool {
    let email = email.trim().to_lowercase();
    email.rsplit_once('@').is_some_and(|(_, domain)| {
        let domain = domain.trim_end_matches('.');
        domain == AGENT_EMAIL_DOMAIN
            || domain
                .strip_suffix(AGENT_EMAIL_DOMAIN)
                .is_some_and(|rest| rest.ends_with('.'))
    })
}
