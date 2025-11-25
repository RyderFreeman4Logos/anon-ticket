use std::time::Duration;

use fastbloom::AtomicBloomFilter;
use moka::sync::Cache;
use thiserror::Error;

use crate::model::PaymentId;

/// The cached knowledge about a PID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PidPresence {
    Present,
    Absent,
}

/// Trait describing a PID cache/bloom filter abstraction.
pub trait PidCache: Send + Sync {
    /// Returns `true` if the PID is known/predicted to exist.
    fn might_contain(&self, pid: &PaymentId) -> bool;

    /// Marks the PID as present (remove any negative entries).
    fn mark_present(&self, pid: &PaymentId);
}

#[derive(Debug)]
pub struct InMemoryPidCache {
    positives: Cache<[u8; 8], ()>,
}

impl PidCache for InMemoryPidCache {
    fn might_contain(&self, pid: &PaymentId) -> bool {
        self.positives.contains_key(pid.as_bytes())
    }

    fn mark_present(&self, pid: &PaymentId) {
        self.positives.insert(*pid.as_bytes(), ());
    }
}

impl InMemoryPidCache {
    pub const DEFAULT_TTL: Duration = Duration::from_secs(60);
    pub const DEFAULT_CAPACITY: u64 = 100_000;

    pub fn new(ttl: Duration) -> Self {
        Self::with_capacity(ttl, Self::DEFAULT_CAPACITY)
    }

    pub fn with_capacity(ttl: Duration, capacity: u64) -> Self {
        let capacity = capacity.max(1);
        Self {
            positives: Cache::builder()
                .time_to_live(ttl)
                .max_capacity(capacity)
                .build(),
        }
    }

    pub fn known_present(&self, pid: &PaymentId) -> bool {
        self.positives.contains_key(pid.as_bytes())
    }
}

impl Default for InMemoryPidCache {
    fn default() -> Self {
        Self::new(Self::DEFAULT_TTL)
    }
}

/// Bloom filter for PID hints. False positives are allowed; false negatives are
/// not expected from the underlying implementation.
#[derive(Debug)]
pub struct PidBloom {
    filter: AtomicBloomFilter,
}

impl PidBloom {
    pub fn new(expected_items: u64, false_positive_rate: f64) -> Result<Self, BloomConfigError> {
        if expected_items == 0 {
            return Err(BloomConfigError::InvalidEntries);
        }
        if !(0.0..1.0).contains(&false_positive_rate) {
            return Err(BloomConfigError::InvalidFalsePositiveRate(
                false_positive_rate,
            ));
        }
        let filter = AtomicBloomFilter::with_false_pos(false_positive_rate)
            .seed(&0_u128)
            .expected_items(expected_items as usize);
        Ok(Self { filter })
    }

    #[inline]
    pub fn insert(&self, pid: &PaymentId) {
        self.filter.insert(pid.as_bytes());
    }

    #[inline]
    pub fn might_contain(&self, pid: &PaymentId) -> bool {
        self.filter.contains(pid.as_bytes())
    }
}

#[derive(Debug, Error, PartialEq)]
pub enum BloomConfigError {
    #[error("expected_items must be greater than zero")]
    InvalidEntries,
    #[error("false positive rate must be in (0,1): {0}")]
    InvalidFalsePositiveRate(f64),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PaymentId;

    #[test]
    fn marks_presence() {
        let cache = InMemoryPidCache::default();
        let pid = PaymentId::new("0123456789abcdef");
        assert!(!cache.might_contain(&pid));
        cache.mark_present(&pid);
        assert!(cache.might_contain(&pid));
        assert!(cache.known_present(&pid));
    }

    #[test]
    fn bloom_inserts_without_false_negative() {
        let pid = PaymentId::new("0123456789abcdef");
        let bloom = PidBloom::new(10_000, 0.01).expect("bloom config ok");
        assert!(!bloom.might_contain(&pid));
        bloom.insert(&pid);
        assert!(bloom.might_contain(&pid));
    }
}
