use std::{collections::HashSet, sync::RwLock};

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

#[derive(Debug, Default)]
pub struct InMemoryPidCache {
    positives: RwLock<HashSet<String>>,
    negatives: RwLock<HashSet<String>>,
}

impl PidCache for InMemoryPidCache {
    fn might_contain(&self, pid: &PaymentId) -> bool {
        let negatives = self.negatives.read().unwrap();
        !negatives.contains(pid.as_str())
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
        negatives.insert(pid.as_str().to_string());
    }
}

impl InMemoryPidCache {
    pub fn known_present(&self, pid: &PaymentId) -> bool {
        self.positives.read().unwrap().contains(pid.as_str())
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
}
