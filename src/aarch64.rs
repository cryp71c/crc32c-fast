/// Computes CRC32C using the AArch64 CRC extension instructions.
///
/// Little-endian only. The `ldr` family loads in the target's byte order, while
/// `crc32c*` consumes the register least-significant-byte-first, so on a
/// big-endian target the bytes would feed in reversed. `crate::crc32c` gates
/// this backend on `target_endian = "little"` for that reason.
///
/// # Safety
///
/// The caller must ensure that the executing CPU supports the `crc` extension
/// before calling this function.
///
/// Nothing enforces this. The `crc32c*` instructions are written directly into
/// the assembly, so they are emitted regardless of `#[target_feature]` and will
/// raise SIGILL on a CPU without the extension. The runtime check in
/// `crate::crc32c` is the only guard.
#[target_feature(enable = "crc")]
pub unsafe fn crc32c_u32(data: &[u8]) -> u32 {
    let ptr = data.as_ptr();
    let remaining = data.len();

    let mut crc32c_accumulator = 0xFFFFFFFF;

    unsafe {
        // Numeric local labels are required; asm! blocks may be duplicated by
        // inlining, which makes named assembler symbols unsafe.
        //
        //   2 = 8-byte main loop      5 = 1-byte tail
        //   3 = tail dispatch         6 = 2-byte tail
        //   4 = done                  7 = 4-byte tail
        core::arch::asm!(
            "2:",
                "cmp {remaining}, #8",
                "b.lo 3f",
                "ldr {data_chunk:x}, [{ptr:x}], #8",
                "crc32cx {crc32c_accumulator:w}, {crc32c_accumulator:w}, {data_chunk:x}",
                "sub {remaining}, {remaining}, #8",
                "b 2b",

            "3:",
                "cbz {remaining:x}, 4f",
                "tbnz {remaining:x}, #2, 7f",
                "tbnz {remaining:x}, #1, 6f",
                "tbnz {remaining:x}, #0, 5f",

                // Unreachable: the main loop guarantees remaining < 8, so one
                // of the three tests above always matches a non-zero value.
                // Branching out anyway means a future change to that invariant
                // cannot silently fall into the 4-byte handler and read out of
                // bounds. Never taken, so it costs nothing.
                "b 4f",

            "7:",
                "ldr {data_chunk:w}, [{ptr:x}], #4",
                "crc32cw {crc32c_accumulator:w}, {crc32c_accumulator:w}, {data_chunk:w}",
                "sub {remaining:x}, {remaining:x}, #4",
                "b 3b",

            "6:",
                "ldrh {data_chunk:w}, [{ptr:x}], #2",
                "crc32ch {crc32c_accumulator:w}, {crc32c_accumulator:w}, {data_chunk:w}",
                "sub {remaining:x}, {remaining:x}, #2",
                "b 3b",

            "5:",
                "ldrb {data_chunk:w}, [{ptr:x}], #1",
                "crc32cb {crc32c_accumulator:w}, {crc32c_accumulator:w}, {data_chunk:w}",
                "sub {remaining:x}, {remaining:x}, #1",
                "b 3b",

            "4:",
                "mvn {crc32c_accumulator:w}, {crc32c_accumulator:w}",

            crc32c_accumulator = inout(reg) crc32c_accumulator,
            ptr = inout(reg) ptr => _,
            remaining = inout(reg) remaining => _,
            data_chunk = out(reg) _,

            // Reads through `ptr` but never writes memory, and never touches
            // the stack. Saying so lets the compiler keep values in registers
            // across the block instead of assuming arbitrary side effects.
            options(readonly, nostack),
        )
    }

    crc32c_accumulator
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aarch64_known_crc32c_vectors() {
        for (input, expected) in crate::test_vectors::KNOWN {
            let result = unsafe { crc32c_u32(input) };

            assert_eq!(result, expected, "CRC32C mismatch for input: {input:?}");
        }
    }
}
