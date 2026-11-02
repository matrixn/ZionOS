use std::{
    future::Future,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

/// One collector at a time. Cancellation releases the gate; values keep their original timestamp.
pub struct Cache<T> {
    value: Mutex<Option<(Instant, T)>>,
}
impl<T: Clone> Cache<T> {
    pub const fn new() -> Self {
        Self {
            value: Mutex::const_new(None),
        }
    }
    pub async fn get<F, Fut>(&self, ttl: Duration, collect: F) -> T
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = T>,
    {
        let mut value = self.value.lock().await;
        if let Some((when, data)) = &*value {
            if when.elapsed() < ttl {
                return data.clone();
            }
        }
        let data = collect().await;
        *value = Some((Instant::now(), data.clone()));
        data
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    #[tokio::test]
    async fn cancelled_collector_releases_the_singleflight_gate() {
        let cache = std::sync::Arc::new(Cache::new());
        let worker = cache.clone();
        let (started, ready) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            worker
                .get(Duration::from_secs(5), || async move {
                    let _ = started.send(());
                    std::future::pending::<usize>().await
                })
                .await
        });
        ready.await.unwrap();
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        assert_eq!(cache.get(Duration::from_secs(5), || async { 7 }).await, 7);
    }
    #[tokio::test]
    async fn concurrent_readers_share_one_collector_and_expiry_refreshes() {
        let cache = Cache::new();
        let count = AtomicUsize::new(0);
        let collect = || async {
            tokio::task::yield_now().await;
            count.fetch_add(1, Ordering::SeqCst) + 1
        };
        let (a, b, c) = tokio::join!(
            cache.get(Duration::from_secs(5), collect),
            cache.get(Duration::from_secs(5), collect),
            cache.get(Duration::from_secs(5), collect)
        );
        assert_eq!((a, b, c), (1, 1, 1));
        assert_eq!(cache.get(Duration::ZERO, collect).await, 2);
    }
}
