# crc32c-fast

A fast CRC32C implementation for Rust with runtime hardware acceleration on supported CPUs. The crate currently provides x86_64 SSE4.2 and AArch64 CRC instruction backends with a portable software fallback.

## Features

- Safe public `crc32c(&[u8]) -> u32` API
- Runtime CPU feature detection
- x86_64 hardware acceleration using SSE4.2 CRC32 instructions
- AArch64 hardware acceleration using the CRC instruction extension
- Portable software fallback for unsupported CPUs
- Criterion benchmarks for public API and backend research
- Experimental three-way x86_64 CRC pipeline using SSE4.2 and PCLMULQDQ

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
| x86_64 | Hardware CRC32C | SSE4.2 | Supported |
| AArch64 | Hardware CRC32C | CRC extension | Supported |
| Other | Portable software implementation | None | Supported |

The x86_64 and AArch64 backends are selected at runtime only when the required CPU feature is detected.

## Testing

Run the production test suite:

```powershell
cargo test
```

Run all tests, including the experimental backend research tests:

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

## Experimental Backend

The repository includes an experimental x86_64 backend that processes CRC32C using three independent SSE4.2 CRC dependency chains and combines them using PCLMULQDQ.

This backend is intended for performance research and benchmarking only. It is not currently used by the public `crc32c()` dispatcher.

Current limitations:

- x86_64 only
- requires SSE4.2 and PCLMULQDQ
- input length must be an exact multiple of 768 bytes
- arbitrary-length tail handling is not yet production-ready

## License

Licensed under either of:

- Apache License, Version 2.0
- MIT License

at your option.

See [LICENSE-APACHE](LICENSE-APACHE) and [LICENSE-MIT](LICENSE-MIT) for details.
