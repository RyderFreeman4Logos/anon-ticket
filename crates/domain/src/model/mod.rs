//! Data structures and helpers shared across the API and monitor binaries.

use chrono::{DateTime, Utc};
use hex::encode as hex_encode;
use sha3::{Digest, Sha3_256};
use thiserror::Error;

/// Returns a static readiness message shared by sibling crates.
pub fn workspace_ready_message() -> &'static str {
    "anon-ticket workspace scaffolding ready"
}

/// Deterministically derives a SHA3-256 fingerprint for a PID or token seed.
/// This keeps hashing consistent across binaries until the full token module
/// lands.
pub fn derive_pid_fingerprint(pid: &str) -> String {
    let mut hasher = Sha3_256::new();
    hasher.update(pid.as_bytes());
    let digest = hasher.finalize();
    hex_encode(digest)
}

/// Generates a deterministic SHA3-256 service token from the PID + TXID pair.
pub fn derive_service_token(pid: &PaymentId, txid: &str) -> ServiceToken {
    let mut hasher = Sha3_256::new();
    hasher.update(pid.as_str().as_bytes());
    hasher.update(txid.as_bytes());
    let digest = hasher.finalize();
    ServiceToken::new(hex_encode(digest))
}

/// Required length (in hex characters) for externally supplied payment IDs.
pub const PID_LENGTH: usize = 64;

/// Errors emitted when user-supplied payment IDs fail validation.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PidFormatError {
    #[error("payment id must be exactly {PID_LENGTH} hex characters")]
    WrongLength,
    #[error("payment id contains non-hex characters")]
    NonHex,
}

/// Validates that the supplied PID matches the 64 hex-character contract.
pub fn validate_pid(pid: &str) -> Result<(), PidFormatError> {
    if pid.len() != PID_LENGTH {
        return Err(PidFormatError::WrongLength);
    }

    if !pid.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(PidFormatError::NonHex);
    }

    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PaymentId(String);

impl PaymentId {
    pub fn new(value: impl Into<String>) -> Self {
        let mut owned = value.into();
        owned.make_ascii_lowercase();
        Self(owned)
    }

    pub fn parse(pid: &str) -> Result<Self, PidFormatError> {
        validate_pid(pid)?;
        Ok(Self::new(pid))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_inner(self) -> String {
        self.0
    }
}

impl From<&str> for PaymentId {
    fn from(value: &str) -> Self {
        Self::new(value.to_owned())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ServiceToken(String);

impl ServiceToken {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_inner(self) -> String {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaymentStatus {
    Unclaimed,
    Claimed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentRecord {
    pub pid: PaymentId,
    pub txid: String,
    pub amount: i64,
    pub block_height: i64,
    pub status: PaymentStatus,
    pub created_at: DateTime<Utc>,
    pub claimed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewPayment {
    pub pid: PaymentId,
    pub txid: String,
    pub amount: i64,
    pub block_height: i64,
    pub detected_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimOutcome {
    pub pid: PaymentId,
    pub txid: String,
    pub amount: i64,
    pub block_height: i64,
    pub claimed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewServiceToken {
    pub token: ServiceToken,
    pub pid: PaymentId,
    pub amount: i64,
    pub issued_at: DateTime<Utc>,
    pub abuse_score: i16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceTokenRecord {
    pub token: ServiceToken,
    pub pid: PaymentId,
    pub amount: i64,
    pub issued_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub revoke_reason: Option<String>,
    pub abuse_score: i16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevokeTokenRequest {
    pub token: ServiceToken,
    pub reason: Option<String>,
    pub abuse_score: Option<i16>,
}

#[cfg(test)]
mod tests {
    use super::*;
    const VALID_PID: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    #[test]
    fn readiness_message_is_stable() {
        assert_eq!(
            workspace_ready_message(),
            "anon-ticket workspace scaffolding ready"
        );
    }

    #[test]
    fn pid_fingerprint_is_deterministic() {
        let left = derive_pid_fingerprint("abcd");
        let right = derive_pid_fingerprint("abcd");
        assert_eq!(left, right);
        assert_eq!(left.len(), 64);
    }

    #[test]
    fn pid_validation_rejects_invalid_inputs() {
        assert_eq!(validate_pid("deadbeef"), Err(PidFormatError::WrongLength));
        assert_eq!(
            validate_pid(&"z".repeat(PID_LENGTH)),
            Err(PidFormatError::NonHex)
        );
        assert!(validate_pid(VALID_PID).is_ok());
    }

    #[test]
    fn payment_id_parse_checks_format() {
        assert!(PaymentId::parse(VALID_PID).is_ok());
        assert!(PaymentId::parse("not-valid").is_err());
    }

    #[test]
    fn payment_id_canonicalizes_case() {
        let uppercase = "ABCDEFAB".repeat(8);
        let pid = PaymentId::parse(&uppercase).unwrap();
        assert_eq!(pid.as_str(), "abcdefab".repeat(8));

        let raw = PaymentId::new("FEDCBA9876543210".repeat(4));
        assert_eq!(raw.as_str(), "fedcba9876543210".repeat(4));
    }

    #[test]
    fn service_token_derivation_is_deterministic() {
        let pid = PaymentId::parse(VALID_PID).unwrap();
        let a = derive_service_token(&pid, "tx1");
        let b = derive_service_token(&pid, "tx1");
        assert_eq!(a.as_str(), b.as_str());
    }
}
