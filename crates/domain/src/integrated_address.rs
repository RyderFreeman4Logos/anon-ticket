use std::str::FromStr;

use monero::{
    util::address::{AddressType, PaymentId as MoneroPaymentId},
    Address,
};
use thiserror::Error;

use crate::model::{PaymentId, PidFormatError};

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum IntegratedAddressError {
    #[error("invalid primary address: {0}")]
    InvalidPrimary(String),
    #[error("primary address must be a standard address (not integrated/subaddress)")]
    NonStandardPrimary,
    #[error("invalid integrated address: {0}")]
    InvalidIntegrated(String),
    #[error("integrated address missing embedded payment id")]
    MissingPaymentId,
    #[error("embedded payment id is invalid: {0}")]
    InvalidPaymentId(String),
}

/// Build an integrated address from a standard primary address and a validated payment id.
///
/// This is suitable for FFI/wasm exports: inputs/outputs are plain strings, and any parse failure
/// returns a descriptive error instead of panicking.
pub fn build_integrated_address(
    primary_address: &str,
    payment_id: &PaymentId,
) -> Result<String, IntegratedAddressError> {
    let base = Address::from_str(primary_address)
        .map_err(|err| IntegratedAddressError::InvalidPrimary(err.to_string()))?;

    if !matches!(base.addr_type, AddressType::Standard) {
        return Err(IntegratedAddressError::NonStandardPrimary);
    }

    let pid = MoneroPaymentId::from_slice(payment_id.as_bytes());
    let integrated = Address::integrated(base.network, base.public_spend, base.public_view, pid);

    Ok(integrated.to_string())
}

/// Parse an integrated address, extracting both the embedded payment id and the underlying
/// standard address.
pub fn decode_integrated_address(
    integrated_address: &str,
) -> Result<(String, PaymentId), IntegratedAddressError> {
    let address = Address::from_str(integrated_address)
        .map_err(|err| IntegratedAddressError::InvalidIntegrated(err.to_string()))?;

    let AddressType::Integrated(pid) = address.addr_type else {
        return Err(IntegratedAddressError::MissingPaymentId);
    };

    let payment_id = PaymentId::try_from(pid.as_bytes().to_vec())
        .map_err(|err: PidFormatError| IntegratedAddressError::InvalidPaymentId(err.to_string()))?;

    let primary = Address {
        addr_type: AddressType::Standard,
        ..address
    };

    Ok((primary.to_string(), payment_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PRIMARY_MAINNET: &str =
        "4ADT1BtbxqEWeMKp9GgPr2NeyJXXtNxvoDawpyA4WpzFcGcoHUvXeijE66DNfohE9r1bQYaBiQjEtKE7CtkTdLwiDznFzra";
    const SAMPLE_PID: &str = "0123456789abcdef";

    #[test]
    fn builds_and_decodes_integrated_address() {
        let pid = PaymentId::parse(SAMPLE_PID).expect("valid pid");
        let integrated =
            build_integrated_address(PRIMARY_MAINNET, &pid).expect("build integrated address");

        let (standard, recovered_pid) =
            decode_integrated_address(&integrated).expect("decode succeeds");

        assert_eq!(standard, PRIMARY_MAINNET);
        assert_eq!(recovered_pid, pid);
    }

    #[test]
    fn rejects_non_standard_primary() {
        let pid = PaymentId::parse(SAMPLE_PID).expect("valid pid");
        let integrated =
            build_integrated_address(PRIMARY_MAINNET, &pid).expect("build integrated address");

        let err = build_integrated_address(&integrated, &pid).unwrap_err();
        assert_eq!(err, IntegratedAddressError::NonStandardPrimary);
    }
}
