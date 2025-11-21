use std::time::{Duration, Instant};

use moka::sync::Cache;

use crate::model::PaymentId;

/// The cached knowledge about a PID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PidPresence {
    Present,
    Absent,
}

/// Trait describing a PID cache/bloom filter abstraction.
pub trait PidCache: Send + Sync {
    /// Returns `true` if the PID might exist (i.e. not known absent).
    fn might_contain(&self, pid: &PaymentId) -> bool;

    /// Marks the PID as present (remove any negative entries).
    fn mark_present(&self, pid: &PaymentId);

    /// Marks the PID as absent for a period of time.
    fn mark_absent(&self, pid: &PaymentId);

    /// Returns how long the PID has been cached as absent, if the implementation tracks it.
    fn negative_entry_age(&self, _pid: &PaymentId) -> Option<Duration> {
        None
    }
}

#[derive(Debug)]
pub struct InMemoryPidCache {
    positives: Cache<String, ()>,
    negatives: Cache<String, Instant>,
}

impl PidCache for InMemoryPidCache {
    fn might_contain(&self, pid: &PaymentId) -> bool {
        !self.negatives.contains_key(pid.as_str())
    }

    fn mark_present(&self, pid: &PaymentId) {
        self.positives.insert(pid.as_str().to_string(), ());
        self.negatives.invalidate(pid.as_str());
    }

    fn mark_absent(&self, pid: &PaymentId) {
        self.negatives
            .insert(pid.as_str().to_string(), Instant::now());
    }

    fn negative_entry_age(&self, pid: &PaymentId) -> Option<Duration> {
        self.negatives
            .get(pid.as_str())
            .map(|inserted| Instant::now().saturating_duration_since(inserted))
    }
}

impl InMemoryPidCache {
    const DEFAULT_TTL: Duration = Duration::from_secs(60);
    const DEFAULT_CAPACITY: u64 = 100_000;

    pub fn new(ttl: Duration) -> Self {
        Self::with_capacity(ttl, Self::DEFAULT_CAPACITY)
    }

    fn with_capacity(ttl: Duration, capacity: u64) -> Self {
        let capacity = capacity.max(1);
        Self {
            positives: Cache::builder()
                .time_to_live(ttl)
                .max_capacity(capacity)
                .build(),
            negatives: Cache::builder()
                .time_to_live(ttl)
                .max_capacity(capacity)
                .build(),
        }
    }

    pub fn known_present(&self, pid: &PaymentId) -> bool {
        self.positives.contains_key(pid.as_str())
    }
}

impl Default for InMemoryPidCache {
    fn default() -> Self {
        Self::new(Self::DEFAULT_TTL)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PaymentId;

    #[test]
    fn marks_presence_and_absence() {
        let cache = InMemoryPidCache::default();
        let pid =
            PaymentId::new("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef");
        assert!(cache.might_contain(&pid));
        cache.mark_absent(&pid);
        assert!(!cache.might_contain(&pid));
        cache.mark_present(&pid);
        assert!(cache.might_contain(&pid));
        assert!(cache.known_present(&pid));
    }

    #[test]
    fn negatives_expire() {
        let cache = InMemoryPidCache::new(Duration::from_millis(10));
        let pid =
            PaymentId::new("fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210");
        cache.mark_absent(&pid);
        assert!(!cache.might_contain(&pid));
        std::thread::sleep(Duration::from_millis(15));
        assert!(cache.might_contain(&pid));
    }
}
