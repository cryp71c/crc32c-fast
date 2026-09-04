#[cfg(all(target_arch = "x86_64", feature = "bench-internals"))]
pub mod x86_64;

#[cfg(all(target_arch = "x86_64", not(feature = "bench-internals")))]
mod x86_64;

#[cfg(all(target_arch = "aarch64", feature = "bench-internals"))]
pub mod aarch64;

#[cfg(all(target_arch = "aarch64", not(feature = "bench-internals")))]
mod aarch64;

#[cfg(feature = "bench-internals")]
pub mod portable;

#[cfg(not(feature = "bench-internals"))]
mod portable;

/// Computes the CRC32C checksum of `data`.
///
/// The implementation selects a hardware-accelerated backend at runtime
/// when supported by the current CPU and otherwise falls back to the
/// portable implementation.
///
/// # Examples
///
/// ```
/// use crc32c_fast::crc32c;
///
/// assert_eq!(crc32c(b"123456789"), 0xE3069283);
/// ```
pub fn crc32c(data: &[u8]) -> u32 {
    #[cfg(target_arch = "x86_64")]
    {
        if std::arch::is_x86_feature_detected!("sse4.2") {
            unsafe { return x86_64::crc32c_u32(data) }
        }
    }

    #[cfg(target_arch = "aarch64")]
    {
        if std::arch::is_aarch64_feature_detected!("crc") {
            unsafe { return aarch64::crc32c_u32(data) }
        }
    }

    portable::crc32c_u32(data)
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn public_api_known_vector() {
        let result = crc32c(b"123456789");

        assert_eq!(
            result, 0xE3069283,
            "CRC32C public API mismatch for input: {:?}",
            "123456789"
        );
    }
}
