//! A fast CRC32C (Castagnoli) implementation with runtime hardware
//! acceleration and a portable fallback.
//!
//! The crate exposes a single function, [`crc32c`]. It is safe, cannot panic,
//! and has no runtime dependencies.
//!
//! ```
//! use crc32c_fast::crc32c;
//!
//! assert_eq!(crc32c(b"123456789"), 0xE3069283);
//! ```
//!
//! # Backend selection
//!
//! [`crc32c`] picks a backend on every call based on CPU feature detection,
//! so one binary runs correctly on machines with and without the relevant
//! instructions:
//!
//! | Target | Backend | Required feature |
//! | --- | --- | --- |
//! | x86_64 | `crc32` (SSE4.2) | `sse4.2` |
//! | AArch64, little-endian | `crc32c*` (CRC extension) | `crc` |
//! | anything else | portable slice-by-8 | none |
//!
//! The portable path is not merely a stub: it retires eight bytes per
//! iteration through eight independent table lookups, and it is what runs on
//! big-endian AArch64 and on x86_64 CPUs predating SSE4.2.
//!
//! # This is not a cryptographic hash
//!
//! CRC32C detects accidental corruption — bit flips on a wire, a bad disk
//! sector, a truncated transfer. It offers **no security against a deliberate
//! attacker**: the function is linear and trivially forgeable, so anyone who
//! can modify your data can recompute a matching checksum or craft a different
//! message with the same one.
//!
//! Do not use it for authentication, tamper detection, or deduplication of
//! untrusted content. Reach for SHA-2 or BLAKE3 instead, or an HMAC where a
//! shared secret is involved.

#![warn(missing_docs)]

#[cfg(target_arch = "x86_64")]
mod x86_64;

// Little-endian only: the backend's `ldr` loads disagree with what `crc32c*`
// consumes on a big-endian target. `aarch64_be-*` reports `target_arch =
// "aarch64"`, so gating on the architecture alone would select a backend that
// computes the wrong checksum there. Those targets fall through to `portable`,
// which reads bytes with `from_le_bytes` and is endian-independent.
#[cfg(all(target_arch = "aarch64", target_endian = "little"))]
mod aarch64;

mod portable;

/// Internal backends, exposed only so this crate's own benchmarks can compare
/// them directly.
///
/// Gated behind the `bench-internals` feature, hidden from the documentation,
/// and **not part of the public API** — anything reachable through this module
/// may change or disappear in a patch release.
///
/// Cargo features are additive across an entire dependency graph, so another
/// crate enabling `bench-internals` makes these reachable from your code as
/// well. They are `unsafe fn` with real CPU-feature contracts; read each
/// function's `# Safety` section before calling one.
#[cfg(feature = "bench-internals")]
#[doc(hidden)]
pub mod __bench {
    #[cfg(target_arch = "x86_64")]
    pub mod x86_64 {
        pub use crate::x86_64::crc32c_u32;
    }

    #[cfg(all(target_arch = "aarch64", target_endian = "little"))]
    pub mod aarch64 {
        pub use crate::aarch64::crc32c_u32;
    }

    pub mod portable {
        pub use crate::portable::crc32c_u32;
    }
}

/// Computes the CRC32C checksum of `data`.
///
/// The implementation selects a hardware-accelerated backend at runtime
/// when supported by the current CPU and otherwise falls back to the
/// portable implementation.
///
/// CRC32C is a checksum, not a cryptographic hash. It detects accidental
/// corruption, but it is linear and trivially forgeable — see the crate README
/// before using it anywhere an attacker influences the input.
///
/// # Examples
///
/// ```
/// use crc32c_fast::crc32c;
///
/// assert_eq!(crc32c(b"123456789"), 0xE3069283);
/// ```
#[inline]
pub fn crc32c(data: &[u8]) -> u32 {
    #[cfg(target_arch = "x86_64")]
    {
        if std::arch::is_x86_feature_detected!("sse4.2") {
            return unsafe { x86_64::crc32c_u32(data) };
        }
    }

    #[cfg(all(target_arch = "aarch64", target_endian = "little"))]
    {
        if std::arch::is_aarch64_feature_detected!("crc") {
            return unsafe { aarch64::crc32c_u32(data) };
        }
    }

    portable::crc32c_u32(data)
}

#[cfg(test)]
pub(crate) mod test_vectors {
    /// Known-good CRC32C values shared by every backend's unit tests.
    ///
    /// Chosen to exercise each tail path of the hardware backends: an 8-byte
    /// main-loop pass plus every combination of the 4, 2 and 1 byte handlers.
    pub(crate) const KNOWN: [(&[u8], u32); 10] = [
        (b"", 0x00000000),
        (b"1", 0x90F599E3),         // 1
        (b"12", 0x7355C460),        // 2
        (b"123", 0x107B2FB2),       // 2 + 1
        (b"1234", 0xF63AF4EE),      // 4
        (b"12345", 0x18D12335),     // 4 + 1
        (b"123456", 0x41357186),    // 4 + 2
        (b"1234567", 0x124297EA),   // 4 + 2 + 1
        (b"12345678", 0x6087809A),  // 8, main loop only
        (b"123456789", 0xE3069283), // 8 + 1
    ];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_api_known_vectors() {
        for (input, expected) in test_vectors::KNOWN {
            assert_eq!(
                crc32c(input),
                expected,
                "CRC32C public API mismatch for input: {input:?}"
            );
        }
    }
}
