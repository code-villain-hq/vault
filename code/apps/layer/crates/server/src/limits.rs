//! Limits on guessing passwords: after too many wrong ones for an account
//! in a while, more tries are refused until the oldest ones are old enough.
//! Tries in progress count too, so many at once can't slip past the limit.
//! Signing up is limited the same way, per client address, counting every
//! try. Kept in memory, which is enough for one server process.

use std::collections::{HashMap, VecDeque};
use std::convert::Infallible;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::HeaderMap;
use axum::http::request::Parts;

use crate::state::AppState;

#[derive(Default)]
struct Record {
    failures: VecDeque<Instant>,
    in_flight: usize,
}

pub struct Attempts {
    max: usize,
    window: Duration,
    records: Arc<Mutex<HashMap<String, Record>>>,
}

impl Default for Attempts {
    /// Ten wrong passwords in fifteen minutes.
    fn default() -> Self {
        Attempts::new(10, Duration::from_secs(15 * 60))
    }
}

/// One password check in progress. Say how it went with [`Attempt::failed`]
/// or [`Attempt::succeeded`]; dropped without either, it simply ends.
#[must_use]
pub struct Attempt {
    key: String,
    records: Arc<Mutex<HashMap<String, Record>>>,
}

impl Attempts {
    pub fn new(max: usize, window: Duration) -> Self {
        Attempts {
            max,
            window,
            records: Arc::default(),
        }
    }

    /// Five sign-ups an hour from one client.
    pub fn sign_ups() -> Self {
        Attempts::new(5, Duration::from_secs(60 * 60))
    }

    /// Counts a try for `key` whatever comes of it, or says how long to wait
    /// if there have been too many lately. For limits on doing something at
    /// all, such as signing up, rather than on getting it wrong.
    pub fn count(&self, key: &str) -> Result<(), Duration> {
        self.begin(key).map(Attempt::failed)
    }

    /// Starts a password check for `key`, or says how long to wait if it
    /// has failed (or is being tried) too often recently.
    pub fn begin(&self, key: &str) -> Result<Attempt, Duration> {
        let now = Instant::now();
        let mut records = self.records.lock().expect("not poisoned");
        // Forget old failures now and then, so the map doesn't grow forever.
        if records.len() > 10_000 {
            records.retain(|_, r| {
                r.in_flight > 0 || r.failures.back().is_some_and(|t| now - *t < self.window)
            });
        }
        let record = records.entry(key.to_string()).or_default();
        while record
            .failures
            .front()
            .is_some_and(|t| now - *t >= self.window)
        {
            record.failures.pop_front();
        }
        if record.failures.len() + record.in_flight >= self.max {
            let wait = record
                .failures
                .front()
                .map_or(Duration::from_secs(60), |first| {
                    self.window - (now - *first)
                });
            return Err(wait);
        }
        record.in_flight += 1;
        Ok(Attempt {
            key: key.to_string(),
            records: self.records.clone(),
        })
    }
}

impl Attempt {
    pub fn failed(self) {
        self.finish(|record| record.failures.push_back(Instant::now()));
    }

    pub fn succeeded(self) {
        self.finish(|record| record.failures.clear());
    }

    fn finish(&self, change: impl FnOnce(&mut Record)) {
        let mut records = self.records.lock().expect("not poisoned");
        if let Some(record) = records.get_mut(&self.key) {
            change(record);
        }
    }
}

impl Drop for Attempt {
    fn drop(&mut self) {
        let mut records = self.records.lock().expect("not poisoned");
        if let Some(record) = records.get_mut(&self.key) {
            record.in_flight = record.in_flight.saturating_sub(1);
            if record.in_flight == 0 && record.failures.is_empty() {
                records.remove(&self.key);
            }
        }
    }
}

/// Where a request comes from, for limits per client: the address of the
/// connection or, when Layer is told it's behind a proxy
/// (`LAYER_TRUST_FORWARDED_FOR=1`), the last address in `X-Forwarded-For`,
/// the one that proxy added. IPv6 clients count per /64, since anyone with
/// one address usually has the whole block. `None` when neither is known.
pub struct ClientAddress(pub Option<IpAddr>);

impl ClientAddress {
    /// The key to count the client's tries under.
    pub fn key(&self) -> String {
        match self.0.map(canonical) {
            Some(IpAddr::V6(ip)) => {
                let s = ip.segments();
                format!("{:x}:{:x}:{:x}:{:x}::/64", s[0], s[1], s[2], s[3])
            }
            Some(ip) => ip.to_string(),
            None => "unknown".into(),
        }
    }
}

/// IPv4 addresses mapped into IPv6 (`::ffff:1.2.3.4`) as plain IPv4.
fn canonical(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(ip, IpAddr::V4),
        v4 => v4,
    }
}

/// The last address in the last `X-Forwarded-For` header.
fn forwarded_for(headers: &HeaderMap) -> Option<IpAddr> {
    headers
        .get_all("x-forwarded-for")
        .iter()
        .next_back()?
        .to_str()
        .ok()?
        .rsplit(',')
        .next()?
        .trim()
        .parse()
        .ok()
}

impl FromRequestParts<AppState> for ClientAddress {
    type Rejection = Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let forwarded = state
            .trust_forwarded_for
            .then(|| forwarded_for(&parts.headers))
            .flatten();
        let peer = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|info| info.0.ip());
        Ok(ClientAddress(forwarded.or(peer)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counting_counts_every_try() {
        let attempts = Attempts::new(2, Duration::from_secs(60));
        attempts.count("1.2.3.4").unwrap();
        attempts.count("1.2.3.4").unwrap();
        assert!(attempts.count("1.2.3.4").is_err());
        assert!(attempts.count("5.6.7.8").is_ok(), "others aren't affected");
    }

    #[test]
    fn clients_are_counted_by_address() {
        let key = |ip: &str| ClientAddress(Some(ip.parse().unwrap())).key();
        assert_eq!(key("203.0.113.7"), "203.0.113.7");
        assert_eq!(key("::ffff:203.0.113.7"), "203.0.113.7");
        assert_eq!(key("2001:db8:1:2:3:4:5:6"), "2001:db8:1:2::/64");
        assert_eq!(key("2001:db8:1:2:ffff::1"), "2001:db8:1:2::/64");
        assert_eq!(ClientAddress(None).key(), "unknown");

        let mut headers = HeaderMap::new();
        headers.append("x-forwarded-for", "10.0.0.1, 198.51.100.2".parse().unwrap());
        assert_eq!(forwarded_for(&headers), "198.51.100.2".parse().ok());
        headers.append("x-forwarded-for", "198.51.100.3".parse().unwrap());
        assert_eq!(forwarded_for(&headers), "198.51.100.3".parse().ok());
    }

    #[test]
    fn too_many_failures_lock_a_key_until_they_age() {
        let attempts = Attempts::new(2, Duration::from_millis(50));
        attempts.begin("ada").unwrap().failed();
        attempts.begin("ada").unwrap().failed();
        assert!(attempts.begin("ada").is_err());
        assert!(attempts.begin("bob").is_ok(), "others aren't affected");
        std::thread::sleep(Duration::from_millis(60));
        assert!(attempts.begin("ada").is_ok());
    }

    #[test]
    fn tries_in_progress_count() {
        let attempts = Attempts::new(2, Duration::from_secs(60));
        let first = attempts.begin("ada").unwrap();
        let second = attempts.begin("ada").unwrap();
        assert!(attempts.begin("ada").is_err(), "two already running");
        drop(first);
        second.failed();
        let third = attempts.begin("ada").unwrap();
        assert!(attempts.begin("ada").is_err(), "one failure, one running");
        third.succeeded();
        assert!(attempts.begin("ada").is_ok(), "success clears failures");
    }
}
