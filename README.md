# crc32c-fast

A fast CRC32C implementation for Rust with runtime hardware acceleration on supported CPUs. The crate provides x86_64 SSE4.2 and PCLMULQDQ acceleration, an AArch64 CRC instruction backend, and a portable software fallback.

## AI-assisted development

AI tools were used during portions of the development and review of this project.
See [AI_USAGE.md](AI_USAGE.md) for the project's AI usage statement.

## Features

- Safe public `crc32c(&[u8]) -> u32` API
- **Zero runtime dependencies** — nothing is pulled into your dependency tree
- Runtime CPU feature detection
- x86_64 hardware acceleration using SSE4.2 CRC32 instructions, with a three-way PCLMULQDQ path for large inputs
- AArch64 hardware acceleration using the CRC instruction extension (little-endian targets)
- Portable slice-by-8 software fallback for unsupported CPUs
- Criterion benchmarks for public API and backend research

Requires Rust 1.88 or later.

## What this is not

CRC32C is a checksum, not a cryptographic hash. It detects accidental
corruption — bit flips on a wire, a bad disk sector, a truncated transfer.

It provides **no security against a deliberate attacker**. CRC32C is linear
and trivially forgeable: anyone who can modify your data can recompute a
matching checksum, or craft a different message with the same one. Do not use
it for authentication, tamper detection, deduplication of untrusted content,
or anywhere an adversary influences the input.

For those cases use a cryptographic hash (SHA-2, BLAKE3) or, where a shared
secret is involved, an HMAC.

## Usage

```rust
use crc32c_fast::crc32c;

fn main() {
    let checksum = crc32c(b"123456789");

    assert_eq!(checksum, 0xE3069283);
}
```

The public `crc32c()` function automatically selects the best available backend at runtime and falls back to the portable implementation when hardware acceleration is unavailable.

## Architecture Support

| Architecture | Backend | Required CPU Feature | Status |
| --- | --- | --- | --- |
| x86_64, 768+ bytes | Three-way hardware CRC32C with carry-less combining | SSE4.2 and PCLMULQDQ | Supported |
| x86_64 fallback | Hardware CRC32C | SSE4.2 | Supported |
| AArch64 (little-endian) | Hardware CRC32C | CRC extension | Supported |
| Other | Portable software implementation | None | Supported |

The x86_64 and AArch64 backends are selected at runtime only when the required CPU features are detected. On x86_64, inputs of at least 768 bytes use the three-way PCLMULQDQ backend when available; smaller inputs and processors without PCLMULQDQ use the SSE4.2 backend.

Big-endian AArch64 targets (`aarch64_be-*`) use the portable backend. The CRC extension instructions consume a register least-significant-byte-first, which disagrees with a big-endian load, so the hardware path is compiled out there rather than left to produce a wrong checksum. The portable implementation reads bytes explicitly little-endian and is correct on any byte order.

## Testing

Run the production test suite:

```powershell
cargo test
```

Run all tests, including the feature-gated backend research tests:

```powershell
cargo test --all-features
```

Check formatting and linting:

```powershell
cargo fmt -- --check
cargo clippy --all-targets --all-features -- -D warnings
```

## Benchmarks

Run the public API benchmark:

```powershell
cargo bench --bench api
```

Run the internal backend research benchmark:

```powershell
cargo bench --bench backend_research --features bench-internals
```

The `backend_research` target is intentionally gated behind the `bench-internals` feature because it exposes internal architecture-specific implementations for direct comparison.

## x86_64 PCLMULQDQ Backend

For inputs of at least 768 bytes, the public dispatcher can process three independent 256-byte SSE4.2 CRC chains and combine them using PCLMULQDQ. Remaining bytes are handled by the serial SSE4.2 tail in the same backend.

Runtime dispatch requires both SSE4.2 and PCLMULQDQ. If either the input or CPU does not meet those conditions, `crc32c()` uses the ordinary SSE4.2 backend or the portable fallback.

The 768-byte crossover was selected from direct Criterion comparisons between the serial SSE4.2 and three-way PCLMULQDQ implementations. Exact multiples of 768 avoid a serial remainder, while arbitrary input lengths remain fully supported.

The feature-gated research benchmark retains direct backend comparisons so the crossover can be reevaluated on other x86_64 microarchitectures.

## License

Licensed under either of:

- Apache License, Version 2.0
- MIT License

at your option.

See [LICENSE-APACHE](LICENSE-APACHE) and [LICENSE-MIT](LICENSE-MIT) for details.
