//! Data structures and helpers shared across the API and monitor binaries.

use cfg_if::cfg_if;
use chrono::{DateTime, Utc};
use getrandom::fill;
use hex::{decode as hex_decode, encode as hex_encode, FromHexError};
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
/// A separator is inserted between components to avoid accidental collisions if
/// their lengths diverge in future formats.
pub fn derive_service_token(pid: &PaymentId, txid: &str) -> ServiceToken {
    let mut hasher = Sha3_256::new();
    hasher.update(pid.to_hex().as_bytes());
    hasher.update(b"|");
    hasher.update(txid.as_bytes());
    let digest = hasher.finalize();
    ServiceToken::from_bytes(digest.into())
}

/// Required length (in hex characters) for externally supplied payment IDs.
pub const PID_LENGTH: usize = 16;

/// Errors emitted when user-supplied payment IDs fail validation.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PidFormatError {
    #[error("payment id must be exactly {PID_LENGTH} hex characters")]
    WrongLength,
    #[error("payment id contains non-hex characters")]
    NonHex,
}

/// Validates that the supplied PID matches the 16 hex-character contract.
pub fn validate_pid(pid: &str) -> Result<(), PidFormatError> {
    if pid.len() != PID_LENGTH {
        return Err(PidFormatError::WrongLength);
    }

    if !pid.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(PidFormatError::NonHex);
    }

    Ok(())
}

fn decode_pid_hex(pid: &str) -> Result<[u8; 8], PidFormatError> {
    let bytes = hex_decode(pid).map_err(map_hex_error_to_pid)?;
    if bytes.len() != 8 {
        return Err(PidFormatError::WrongLength);
    }
    let mut array = [0u8; 8];
    array.copy_from_slice(&bytes);
    Ok(array)
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PaymentId([u8; 8]);

cfg_if! {
    if #[cfg(target_arch = "wasm32")] {
        fn fill_pid_bytes(bytes: &mut [u8; 8]) -> Result<(), getrandom::Error> {
            fill(bytes)
        }
    } else {
        fn fill_pid_bytes(bytes: &mut [u8; 8]) -> Result<(), getrandom::Error> {
            fill(bytes)
        }
    }
}

impl PaymentId {
    pub(crate) fn new(hex: impl AsRef<str>) -> Self {
        let bytes = decode_pid_hex(hex.as_ref()).expect("caller validated pid hex");
        Self(bytes)
    }

    pub fn parse(pid: &str) -> Result<Self, PidFormatError> {
        validate_pid(pid)?;
        Ok(Self::new(pid))
    }

    pub fn generate() -> Result<Self, getrandom::Error> {
        let mut bytes = [0u8; 8];
        fill_pid_bytes(&mut bytes)?;
        Ok(Self(bytes))
    }

    pub fn as_bytes(&self) -> &[u8; 8] {
        &self.0
    }

    pub fn to_hex(&self) -> String {
        hex_encode(self.0)
    }

    pub fn into_inner(self) -> String {
        self.to_hex()
    }

    pub fn into_bytes(self) -> [u8; 8] {
        self.0
    }
}

impl TryFrom<String> for PaymentId {
    type Error = PidFormatError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl TryFrom<Vec<u8>> for PaymentId {
    type Error = PidFormatError;

    fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
        if value.len() != 8 {
            return Err(PidFormatError::WrongLength);
        }
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&value);
        Ok(Self(bytes))
    }
}

impl std::fmt::Display for PaymentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_hex())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Error)]
pub enum TokenFormatError {
    #[error("service token must be exactly 64 hex characters")]
    WrongLength,
    #[error("service token contains non-hex characters")]
    NonHex,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ServiceToken([u8; 32]);

impl ServiceToken {
    pub fn parse(hex: &str) -> Result<Self, TokenFormatError> {
        validate_hex_64(hex)?;
        let bytes = decode_token_hex(hex)?;
        Ok(Self(bytes))
    }

    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn to_hex(&self) -> String {
        hex_encode(self.0)
    }

    pub fn into_inner(self) -> String {
        self.to_hex()
    }

    pub fn into_bytes(self) -> [u8; 32] {
        self.0
    }
}

impl std::fmt::Display for ServiceToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl TryFrom<Vec<u8>> for ServiceToken {
    type Error = TokenFormatError;

    fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
        if value.len() != 32 {
            return Err(TokenFormatError::WrongLength);
        }
        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(&value);
        Ok(Self(bytes))
    }
}

fn validate_hex_64(input: &str) -> Result<(), TokenFormatError> {
    if input.len() != 64 {
        return Err(TokenFormatError::WrongLength);
    }
    if !input.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(TokenFormatError::NonHex);
    }
    Ok(())
}

fn decode_token_hex(token: &str) -> Result<[u8; 32], TokenFormatError> {
    let bytes = hex_decode(token).map_err(map_hex_error_to_token)?;
    if bytes.len() != 32 {
        return Err(TokenFormatError::WrongLength);
    }
    let mut array = [0u8; 32];
    array.copy_from_slice(&bytes);
    Ok(array)
}

fn map_hex_error_to_pid(err: FromHexError) -> PidFormatError {
    match err {
        FromHexError::InvalidHexCharacter { .. } => PidFormatError::NonHex,
        FromHexError::InvalidStringLength => PidFormatError::WrongLength,
        _ => PidFormatError::NonHex,
    }
}

fn map_hex_error_to_token(err: FromHexError) -> TokenFormatError {
    match err {
        FromHexError::InvalidHexCharacter { .. } => TokenFormatError::NonHex,
        FromHexError::InvalidStringLength => TokenFormatError::WrongLength,
        _ => TokenFormatError::NonHex,
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
    const VALID_PID: &str = "0123456789abcdef";

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
        let uppercase = "ABCDEFAB12345678";
        let pid = PaymentId::parse(&uppercase).unwrap();
        assert_eq!(pid.to_hex(), "abcdefab12345678");

        let raw = PaymentId::new("FEDCBA9876543210");
        assert_eq!(raw.to_hex(), "fedcba9876543210");
    }

    #[test]
    fn service_token_derivation_is_deterministic() {
        let pid = PaymentId::parse(VALID_PID).unwrap();
        let a = derive_service_token(&pid, "tx1");
        let b = derive_service_token(&pid, "tx1");
        assert_eq!(a.to_hex(), b.to_hex());
    }

    #[test]
    fn service_token_uses_separator_and_sha3() {
        let pid = PaymentId::parse(VALID_PID).unwrap();
        let token = derive_service_token(&pid, "tx1");
        assert_eq!(
            token.to_hex(),
            "369e0f7c09124783e45fa6a6b7588733e362e2917f36fb7036f49284c1952fa9"
        );
    }

    #[test]
    fn generate_produces_valid_pid() {
        let pid = PaymentId::generate().expect("entropy available");
        let hex = pid.to_hex();
        assert_eq!(hex.len(), PID_LENGTH);
        assert!(validate_pid(&hex).is_ok());
    }
}
