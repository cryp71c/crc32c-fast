/// Computes CRC32C using the x86_64 SSE4.2 CRC instructions.
///
/// # Safety
///
/// The caller must ensure that the executing CPU supports SSE4.2 before
/// calling this function.
///
/// Nothing enforces this. The `crc32` instruction is written directly into the
/// assembly, so it is emitted regardless of `#[target_feature]` and will
/// trigger an illegal-instruction fault (SIGILL on Unix) on a CPU without SSE4.2.
/// The runtime check in `crate::crc32c` is the only guard.
#[target_feature(enable = "sse4.2")]
pub unsafe fn crc32c_u32(data: &[u8]) -> u32 {
    let ptr = data.as_ptr();
    let remaining = data.len();

    let mut crc32c_accumulator: u64 = 0xFFFF_FFFF;

    unsafe {
        // Numeric local labels are required; asm! blocks may be duplicated by
        // inlining, which makes named assembler symbols unsafe.
        //
        //   2 = 8-byte main loop      5 = 4-byte tail
        //   3 = tail dispatch         6 = 2-byte tail
        //   4 = done                  7 = 1-byte tail
        core::arch::asm!(
            "2:",
                "cmp {remaining}, 8",
                "jb 3f",
                "crc32 {crc32c_accumulator:r}, QWORD PTR [{ptr}]",
                "lea {ptr}, [{ptr}+8]",
                "sub {remaining}, 8",
                "jmp 2b",

            "3:",
                "cmp {remaining}, 0",
                "je 4f",

                "test {remaining}, 4",
                "jnz 5f",

                "test {remaining}, 2",
                "jnz 6f",

                "test {remaining}, 1",
                "jnz 7f",

                // Unreachable: the main loop guarantees remaining < 8, so one
                // of the three tests above always matches a non-zero value.
                // Jumping out anyway means a future change to that invariant
                // cannot silently fall into the 4-byte handler and read out of
                // bounds. Never taken, so it costs nothing.
                "jmp 4f",

            "5:",
                "crc32 {crc32c_accumulator:e}, DWORD PTR [{ptr}]",
                "lea {ptr}, [{ptr}+4]",
                "sub {remaining}, 4",
                "jmp 3b",

            "6:",
                "crc32 {crc32c_accumulator:e}, WORD PTR [{ptr}]",
                "lea {ptr}, [{ptr}+2]",
                "sub {remaining}, 2",
                "jmp 3b",

            "7:",
                "crc32 {crc32c_accumulator:e}, BYTE PTR [{ptr}]",
                "lea {ptr}, [{ptr}+1]",
                "sub {remaining}, 1",
                "jmp 3b",

            "4:",
                "xor {crc32c_accumulator:e}, 0xFFFFFFFF",


            crc32c_accumulator = inout(reg) crc32c_accumulator,
            ptr = inout(reg) ptr => _,
            remaining = inout(reg) remaining => _,

            // Reads through `ptr` but never writes memory, and never touches
            // the stack. Saying so lets the compiler keep values in registers
            // across the block instead of assuming arbitrary side effects.
            options(readonly, nostack),
        );
    }

    crc32c_accumulator as u32
}

#[test]
fn x86_64_known_crc32c_vectors() {
    if !std::arch::is_x86_feature_detected!("sse4.2") {
        return;
    }

    for (input, expected) in crate::test_vectors::KNOWN {
        let result = unsafe { crc32c_u32(input) };

        assert_eq!(result, expected, "CRC32C mismatch for input: {input:?}");
    }
}
