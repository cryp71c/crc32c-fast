# crc32c-fast

**Fast CRC32C (Castagnoli) for Rust, with runtime hardware acceleration and a portable fallback.**

[![CI](https://github.com/cryp71c/crc32c-fast/actions/workflows/ci.yml/badge.svg)](https://github.com/cryp71c/crc32c-fast/actions/workflows/ci.yml)

`crc32c-fast` provides a simple, safe API for computing CRC32C checksums. It uses CPU instructions when available and falls back to a portable, slice-by-8 software implementation elsewhere.

- **Hardware-accelerated on x86_64:** SSE4.2 CRC32 instructions, plus a three-way PCLMULQDQ combining path for larger inputs.
- **Hardware-accelerated on little-endian AArch64:** the ARM CRC extension.
- **Portable software fallback:** no CPU-specific instruction requirements; explicit little-endian byte handling.
- **Zero runtime dependencies.**
- **Single-shot API:** `crc32c(&[u8]) -> u32`.

> **CRC32C is not CRC32/IEEE.** This crate implements the Castagnoli polynomial, **not** the CRC32 variant used for Ethernet Frame Check Sequences (FCS). These checksums are not interchangeable.

## Quick start

```rust
use crc32c_fast::crc32c;

fn main() {
    let checksum = crc32c(b"123456789");
    assert_eq!(checksum, 0xE306_9283);
}
```

The API accepts a byte slice and returns its CRC32C checksum as a `u32`. Backend selection is automatic; callers do not need to select CPU features or invoke unsafe code.

**Requirements:** Rust **1.88 or newer** (edition 2024). The library currently uses the Rust standard library; it is **not** a `no_std` crate.

## Architecture support

| Target | Backend selected | Verification status |
| --- | --- | --- |
| **x86_64** | SSE4.2; optionally PCLMULQDQ for inputs ≥ 768 bytes | Covered by existing native CI |
| **AArch64, little-endian** | ARM CRC extension, when detected; otherwise portable | Covered by existing ARM64 native CI |
| **i686 (32-bit x86)** | Portable slice-by-8 | **Six tests passed locally** on Rust 1.89; new i686 CI job configured (first run pending) |
| **PowerPC64, big-endian** | Portable slice-by-8 | Compile-checked in CI; runtime not verified |
| **Other targets** (including ARMv7, MIPS, and RV32) | Portable fallback where the Rust target supports building the crate | Not individually validated; no hardware-specific backend |

**What “portable” means:** targets without an enabled hardware backend compile the same byte-order-independent software implementation. It does **not** mean every Rust target has been built, run, or benchmarked. In particular, ARMv7, MIPS, and RISC-V support should be considered unverified until tested.

The x86_64 hardware backend is **not** currently available on 32-bit x86: i686 uses the software fallback even when its CPU has SSE4.2.

## Backend selection

`crc32c()` checks relevant CPU features at runtime on supported architectures:

| Condition | Implementation |
| --- | --- |
| x86_64 + SSE4.2 + PCLMULQDQ + input length ≥ 768 bytes | Three-way CRC32C with carry-less combining |
| x86_64 + SSE4.2 | Serial hardware CRC32C |
| Little-endian AArch64 + CRC extension | ARM hardware CRC32C |
| Otherwise | Portable slice-by-8 |

The **768-byte crossover** was selected using direct benchmarks against the serial SSE4.2 implementation. It is an implementation choice, not a universal performance guarantee; results depend on CPU and workload.

### Portable implementation

The fallback uses **eight 256-entry lookup tables (8 KiB of read-only table data)** to process eight input bytes at a time. It uses `u32` arithmetic and explicit little-endian byte conversions, so checksum results do not depend on the host's byte order.

This is an optimized software backend, not a bit-by-bit reference loop. Its memory footprint may matter on cache-constrained embedded hardware, which is one reason future embedded-specific benchmarking would be useful.

## Testing

Run the standard tests:

```bash
cargo test --locked
```

Run all feature-gated tests:

```bash
cargo test --all-features --locked
```

Run formatting and lint checks:

```bash
cargo fmt -- --check
cargo clippy --all-targets --all-features --locked -- -D warnings
```

The test suite includes known CRC32C vectors, an independent bitwise oracle for the portable implementation, and comparisons against the `crc32c` reference crate across varied input lengths, alignments, and large buffers.

### 32-bit x86 testing

On a Linux x86_64 host with 32-bit system development libraries installed:

```bash
rustup target add i686-unknown-linux-gnu --toolchain 1.89.0
cargo +1.89.0 test --target i686-unknown-linux-gnu --lib --locked
cargo +1.89.0 test --target i686-unknown-linux-gnu --test reference --locked
```

These commands **passed locally (3 library tests + 3 reference tests)**. The new GitHub Actions i686 job is configured to run both test suites; its first remote run has not yet been verified.

## Benchmarking

Benchmark the public API against the reference crate:

```bash
cargo bench --bench api
```

Compare internal implementations directly:

```bash
cargo bench --bench backend_comparison --features bench-internals
```

The `backend_comparison` benchmark compares the portable backend, hardware backends when available, and the reference crate. On x86_64 it also explores the PCLMULQDQ crossover and streaming workloads. The `bench-internals` feature exposes **internal unsafe backend functions** for these comparisons and for `tests/backend_validation.rs`, which checks the PCLMULQDQ backend against an independent reference. These internals are **not** a supported public API and may change without notice. Benchmark results depend on CPU features, input size, cache behavior, and compiler settings; test on the hardware you care about before drawing performance conclusions.

## Correctness and security

CRC32C detects **accidental data corruption**. It is useful for integrity checks where the expected checksum is trusted and the threat is random errors.

**CRC32C is not a cryptographic hash.** It is linear and forgeable; an attacker who can modify data can also compute a matching CRC32C. Do **not** use it for authentication, adversarial tamper detection, or deduplication of untrusted data. Use a cryptographic hash (for example, SHA-2 or BLAKE3) or an HMAC when authentication is required.

This crate computes **CRC32C (Castagnoli)** using the reflected polynomial `0x82F6_3B78`, **not CRC32/IEEE** (`0xEDB8_8320`). Confirm your protocol's exact checksum variant before using it.

## Scope and limitations

- The public API currently computes a checksum over one complete byte slice; there is no streaming/incremental API.
- i686 is functionally verified through the portable path, but does not have a dedicated hardware-accelerated backend.
- ARMv7, MIPS, and RV32 have not been individually built and runtime-tested for this project.
- The crate currently requires `std`; `no_std` support is not claimed.

## License

Dual-licensed under the [MIT license](LICENSE-MIT) or [Apache License 2.0](LICENSE-APACHE), at your option.

## AI-assisted development

AI tools were used during portions of this project's development and review. See [AI_USAGE.md](AI_USAGE.md) for the project's disclosure.
