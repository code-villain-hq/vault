//! Which pages agents may use (BRW-3): this machine's, always, and the sites
//! the user allowed (Settings → Browser, or a request answered, BRW-11).
//!
//! A page's text is written by whoever runs the site, and an agent reads it
//! the way it reads a ticket: as something that may tell it what to do. So
//! agents go only where the user said they may.

/// The host of an `http` or `https` URL, lowercased, without the port or
/// IPv6 brackets. None for any other scheme.
pub fn host_of(url: &str) -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
        return None;
    }
    let authority = rest.split(['/', '?', '#']).next()?;
    // A userinfo part is someone's way of making `http://localhost@evil.test`
    // read as this machine.
    let authority = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let host =
        if let Some(v6) = authority.strip_prefix('[') { v6.split(']').next()? } else { authority.split(':').next()? };
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    (!host.is_empty()).then_some(host)
}

/// This machine, by any of its names.
pub fn is_local(host: &str) -> bool {
    host == "localhost" || host.ends_with(".localhost") || host == "127.0.0.1" || host == "::1"
}

/// A site as the user or an agent wrote it, as it is kept: a bare host,
/// lowercased, from a host or a URL. None for anything that is not one.
/// A leading `*.` is dropped, as a site covers its subdomains anyway; a `*`
/// elsewhere stays, as a wildcard (`pr-*.example.com`, see `host_matches`).
pub fn normalize(site: &str) -> Option<String> {
    let site = site.trim();
    let host = if site.contains("://") { host_of(site)? } else { host_of(&format!("https://{site}"))? };
    let mut host = host.as_str();
    while let Some(rest) = host.strip_prefix("*.") {
        host = rest;
    }
    let valid = host.contains('.') || host == "localhost";
    let clean = host.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':' | '*'));
    (valid && clean && !host.starts_with('.') && !host.starts_with('-') && wildcard_ok(host)).then(|| host.to_string())
}

/// Whether a site's wildcards leave its domain written out: a `*` may stand
/// in the names in front of a domain (`pr-*.example.com`,
/// `*.preview.example.com`), never in the domain itself (`example.*`,
/// `*.com`), which would take in sites nobody meant. Under `.localhost`
/// anything goes: it is all this machine.
pub fn wildcard_ok(host: &str) -> bool {
    let labels: Vec<&str> = host.split('.').collect();
    if labels.last() == Some(&"localhost") {
        return true;
    }
    labels.len() >= 2 && labels[labels.len() - 2..].iter().all(|l| !l.contains('*'))
}

/// Whether `host` is the host `pattern` names: the same number of names
/// between the dots, each the same, where a `*` in the pattern stands for
/// any run of letters, digits and dashes within that one name. So
/// `pr-*.example.com` is `pr-12.example.com`, not `pr-1.x.example.com`.
pub fn host_matches(pattern: &str, host: &str) -> bool {
    if !pattern.contains('*') {
        return pattern == host;
    }
    let (want, have): (Vec<&str>, Vec<&str>) = (pattern.split('.').collect(), host.split('.').collect());
    want.len() == have.len() && want.iter().zip(&have).all(|(w, h)| label_matches(w, h))
}

/// One name of a host against one of a pattern, `*` standing for any run.
fn label_matches(pattern: &str, label: &str) -> bool {
    let mut parts = pattern.split('*');
    let first = parts.next().unwrap_or_default();
    let Some(mut rest) = label.strip_prefix(first) else { return false };
    let mut parts: Vec<&str> = parts.collect();
    let Some(last) = parts.pop() else { return rest.is_empty() };
    for part in parts {
        match rest.find(part) {
            Some(at) => rest = &rest[at + part.len()..],
            None => return false,
        }
    }
    rest.ends_with(last)
}

/// Whether agents may use the page at `url`. A site covers its subdomains:
/// allowing `example.com` allows `docs.example.com`, and allowing
/// `pr-*.example.com` allows `pr-12.example.com` and `api.pr-12.example.com`.
pub fn allowed(url: &str, sites: &[String]) -> bool {
    if url == "about:blank" {
        return true;
    }
    let Some(host) = host_of(url) else {
        return false;
    };
    // The host, then each domain it is a subdomain of.
    let mut suffixes = std::iter::successors(Some(host.as_str()), |h| h.split_once('.').map(|(_, rest)| rest));
    is_local(&host) || suffixes.any(|h| sites.iter().any(|s| host_matches(s, h)))
}

/// A URL from an agent, with `http://` or `https://` put in front when it
/// has no scheme: this machine's pages are mostly served without TLS.
pub fn complete(url: &str) -> String {
    let url = url.trim();
    if url.contains("://") || url.starts_with("about:") {
        return url.to_string();
    }
    let host = url.split(['/', '?', '#']).next().unwrap_or_default();
    let host = host.rsplit_once(':').map_or(host, |(h, _)| h);
    if is_local(&host.to_ascii_lowercase()) {
        format!("http://{url}")
    } else {
        format!("https://{url}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sites(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn this_machine_is_always_allowed_and_nothing_else_is_by_default() {
        for url in [
            "http://localhost:3000/",
            "http://app.localhost/x",
            "http://127.0.0.1:8080",
            "http://[::1]:5173/",
            "about:blank",
        ] {
            assert!(allowed(url, &[]), "{url}");
        }
        for url in [
            "https://example.com",
            "file:///etc/passwd",
            "chrome://settings",
            "javascript:alert(1)",
            "data:text/html,hi",
        ] {
            assert!(!allowed(url, &[]), "{url}");
        }
    }

    #[test]
    fn a_site_covers_its_subdomains_and_not_lookalikes() {
        let s = sites(&["example.com"]);
        assert!(allowed("https://example.com/a", &s));
        assert!(allowed("https://docs.example.com/a", &s));
        assert!(!allowed("https://badexample.com/", &s));
        assert!(!allowed("https://example.com.evil.test/", &s));
    }

    #[test]
    fn a_wildcard_stands_for_any_run_within_one_name() {
        let s: Vec<String> = ["pr-*.example.com", "*.preview.acme.test"].iter().filter_map(|x| normalize(x)).collect();
        assert!(allowed("https://pr-12.example.com/", &s));
        assert!(allowed("https://api.pr-12.example.com/", &s), "and its subdomains");
        assert!(!allowed("https://example.com/", &s));
        assert!(!allowed("https://pr12.example.com/", &s));
        assert!(!allowed("https://pr-1.example.com.evil.test/", &s));
        assert!(!allowed("https://evil-pr-1.example.com/", &s));
        assert!(allowed("https://preview.acme.test/", &s), "a leading *. is the domain and its subdomains");
        assert!(allowed("https://a.b.preview.acme.test/", &s));
        assert!(host_matches("a*b*c.example.com", "aXXbYYc.example.com"));
        assert!(!host_matches("a*b*c.example.com", "aXXcYYb.example.com"));
        assert!(host_matches("*-api.example.com", "pr-3-api.example.com"));
        assert!(!host_matches("*-api.example.com", "pr-3-api-x.example.com"));
    }

    #[test]
    fn a_wildcard_never_stands_in_the_domain_itself() {
        assert_eq!(normalize("pr-*.example.com").as_deref(), Some("pr-*.example.com"));
        assert_eq!(normalize("*.*.example.com").as_deref(), Some("example.com"));
        assert_eq!(normalize("https://PR-*.Example.com/x").as_deref(), Some("pr-*.example.com"));
        assert_eq!(normalize("*.app.localhost").as_deref(), Some("app.localhost"));
        assert_eq!(normalize("pr-*.localhost").as_deref(), Some("pr-*.localhost"));
        for broad in ["*", "*.com", "example.*", "*.*", "ex*.com", "a.*.com"] {
            assert_eq!(normalize(broad), None, "{broad}");
        }
    }

    #[test]
    fn a_userinfo_part_does_not_pass_for_the_host() {
        assert!(!allowed("http://localhost@evil.test/", &[]));
        assert!(!allowed("http://localhost:80@evil.test/", &[]));
        assert_eq!(host_of("http://user:pw@LOCALHOST:3000/x").as_deref(), Some("localhost"));
    }

    #[test]
    fn a_site_is_kept_as_its_host_from_whatever_it_was_written_as() {
        assert_eq!(normalize("https://GitHub.com/org/repo").as_deref(), Some("github.com"));
        assert_eq!(normalize("*.example.com").as_deref(), Some("example.com"));
        assert_eq!(normalize("docs.example.com:8443").as_deref(), Some("docs.example.com"));
        assert_eq!(normalize("not a site"), None);
        assert_eq!(normalize("com"), None);
        assert_eq!(normalize("file:///etc"), None);
    }

    #[test]
    fn a_url_without_a_scheme_gets_http_for_this_machine_and_https_elsewhere() {
        assert_eq!(complete("localhost:3000/login"), "http://localhost:3000/login");
        assert_eq!(complete("example.com"), "https://example.com");
        assert_eq!(complete("https://example.com"), "https://example.com");
    }
}
