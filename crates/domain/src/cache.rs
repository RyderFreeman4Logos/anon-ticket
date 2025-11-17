use std::{
    collections::HashMap,
    collections::HashSet,
    sync::RwLock,
    time::{Duration, Instant},
};

use crate::PaymentId;

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
}

#[derive(Debug)]
pub struct InMemoryPidCache {
    positives: RwLock<HashSet<String>>,
    negatives: RwLock<HashMap<String, Instant>>,
    ttl: Duration,
}

impl PidCache for InMemoryPidCache {
    fn might_contain(&self, pid: &PaymentId) -> bool {
        self.prune_expired();
        let negatives = self.negatives.read().unwrap();
        !negatives.contains_key(pid.as_str())
    }

    fn mark_present(&self, pid: &PaymentId) {
        let mut positives = self.positives.write().unwrap();
        positives.insert(pid.as_str().to_string());
        drop(positives);
        let mut negatives = self.negatives.write().unwrap();
        negatives.remove(pid.as_str());
    }

    fn mark_absent(&self, pid: &PaymentId) {
        let mut negatives = self.negatives.write().unwrap();
        negatives.insert(pid.as_str().to_string(), Instant::now());
    }
}

impl InMemoryPidCache {
    const DEFAULT_TTL: Duration = Duration::from_secs(60);

    pub fn new(ttl: Duration) -> Self {
        Self {
            positives: RwLock::new(HashSet::new()),
            negatives: RwLock::new(HashMap::new()),
            ttl,
        }
    }

    fn prune_expired(&self) {
        let mut negatives = self.negatives.write().unwrap();
        let now = Instant::now();
        negatives.retain(|_, inserted| now.duration_since(*inserted) <= self.ttl);
    }

    pub fn known_present(&self, pid: &PaymentId) -> bool {
        self.positives.read().unwrap().contains(pid.as_str())
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
        let pid = PaymentId::new("0123456789abcdef0123456789abcdef");
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
        let pid = PaymentId::new("fedcba9876543210fedcba9876543210");
        cache.mark_absent(&pid);
        assert!(!cache.might_contain(&pid));
        std::thread::sleep(Duration::from_millis(15));
        assert!(cache.might_contain(&pid));
    }
}
