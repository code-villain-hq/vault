//! Bounded per-app cache. Concurrent readers share one refresh; errors aren't cached.
use std::{
    collections::HashMap,
    future::Future,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;
use uuid::Uuid;

type Entry<T> = Arc<Mutex<Option<(Instant, T)>>>;
pub struct Cache<T> {
    entries: Mutex<HashMap<Uuid, Entry<T>>>,
    ttl: Duration,
}
impl<T: Clone> Cache<T> {
    pub fn new(ttl: Duration) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            ttl,
        }
    }
    pub async fn get<E, F: Future<Output = Result<T, E>>>(
        &self,
        key: Uuid,
        load: impl FnOnce() -> F,
    ) -> Result<T, E> {
        let entry = {
            let mut entries = self.entries.lock().await;
            if entries.len() >= 1024 {
                entries.retain(|_, entry| {
                    Arc::strong_count(entry) > 1
                        || entry.try_lock().map_or(true, |v| {
                            v.as_ref().is_some_and(|(at, _)| at.elapsed() < self.ttl)
                        })
                });
            }
            if !entries.contains_key(&key) && entries.len() >= 1024 {
                drop(entries);
                return load().await;
            }
            entries
                .entry(key)
                .or_insert_with(|| Arc::new(Mutex::new(None)))
                .clone()
        };
        let mut value = entry.lock().await;
        if let Some((at, data)) = &*value
            && at.elapsed() < self.ttl
        {
            return Ok(data.clone());
        }
        let data = load().await?;
        *value = Some((Instant::now(), data.clone()));
        Ok(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    #[tokio::test]
    async fn readers_share_refresh_and_failures_are_retried() {
        let cache = Cache::new(Duration::from_secs(60));
        let key = Uuid::now_v7();
        let calls = AtomicUsize::new(0);
        let load = || async {
            calls.fetch_add(1, Ordering::SeqCst);
            tokio::task::yield_now().await;
            Ok::<_, ()>(42)
        };
        let (a, b) = tokio::join!(cache.get(key, load), cache.get(key, load));
        assert_eq!((a, b), (Ok(42), Ok(42)));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let other = Uuid::now_v7();
        assert!(
            cache
                .get(other, || async { Err::<i32, _>(()) })
                .await
                .is_err()
        );
        assert_eq!(cache.get(other, load).await, Ok(42));
        let expired = Cache::new(Duration::ZERO);
        expired.get(key, load).await.unwrap();
        expired.get(key, load).await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 4);
    }
}
