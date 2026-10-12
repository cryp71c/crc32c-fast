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

// Processes full 768-byte blocks through three SSE4.2 CRC chains, combines
// them with PCLMULQDQ, then handles any remaining bytes serially.

/// # Safety
///
/// The caller must ensure that the executing CPU supports SSE4.2
/// and pclmulqdq before calling this function.
#[target_feature(enable = "sse4.2,pclmulqdq")]
pub unsafe fn crc32c_pclmul_u32(data: &[u8]) -> u32 {
    // One block contains three independent 256-byte lanes.
    const LANE_BYTES: usize = 256;
    const PARALLEL_BYTES: usize = LANE_BYTES * 3;

    const LANE_B_OFFSET: usize = LANE_BYTES;
    const LANE_C_OFFSET: usize = LANE_BYTES * 2;

    // Leave each lane's final QWORD for the finish/combine step.
    const CHUNK_BYTES: usize = LANE_BYTES - 8;

    // The main loop ends with ptr_a at block_base + 248, while the next block
    // starts at block_base + 768.
    const BLOCK_ADVANCE: usize = PARALLEL_BYTES - CHUNK_BYTES;

    let remaining = data.len();

    let block_remaining = remaining / PARALLEL_BYTES;
    let tail_remaining = remaining % PARALLEL_BYTES;
    let chunk_remaining = CHUNK_BYTES;
    let ptr_a = data.as_ptr();

    // Lane A carries the running state; B and C are zero-seeded contributions.
    let mut crc32c_accumulator_1: u64 = 0xFFFF_FFFF;
    let crc32c_accumulator_2: u64 = 0;
    let crc32c_accumulator_3: u64 = 0;

    // Folding constants for a 256-byte lane.
    const K1_256: u64 = 0xB9E0_2B86;
    const K2_256: u64 = 0xDD7E_3B0C;

    unsafe {
        //   2  = three-way CRC loop
        //   3  = finish lanes A and B, then combine
        //   4  = 8-byte tail loop      6 = 4-byte tail
        //   5  = tail dispatch         7 = 2-byte tail
        //   8  = 1-byte tail           9 = finalize
        //   30 = prepare the next 768-byte block
        //   31 = done
        core::arch::asm!(
            "test {block_remaining}, {block_remaining}",
            "jz 4f",

            // --- 2: three independent CRC streams -------------------------
            "2:",
                "test {chunk_remaining}, {chunk_remaining}",
                "jz 3f",

                "crc32 {crc32c_accumulator_1:r}, QWORD PTR [{ptr_a}]",
                "crc32 {crc32c_accumulator_2:r}, QWORD PTR [{ptr_a} + {lane_b_offset}]",
                "crc32 {crc32c_accumulator_3:r}, QWORD PTR [{ptr_a} + {lane_c_offset}]",

                "lea {ptr_a}, [{ptr_a}+8]",
                "sub {chunk_remaining}, 8",
                "jmp 2b",


            // --- 3: finish lanes A and B ----------------------------------
            //
            // Arrives with ptr_a = block_base + 248.
            "3:",
                "crc32 {crc32c_accumulator_1:r}, QWORD PTR [{ptr_a}]",
                "crc32 {crc32c_accumulator_2:r}, QWORD PTR [{ptr_a} + {lane_b_offset}]",

                // C[248..256] is deliberately NOT processed. The combine step
                // XORs A and B's contributions into that still-raw QWORD before
                // C consumes it.


            // --- combine the three streams --------------------------------

                "movq {fold_constants}, {k2_256:r}",
                "movq {fold_b}, {k1_256:r}",
                "punpcklqdq {fold_constants}, {fold_b}",

                // folded = (acc1 x K2) XOR (acc2 x K1), carry-less.
                "movq {fold_a}, {crc32c_accumulator_1:r}",
                "pclmulqdq {fold_a}, {fold_constants}, 0x00",
                "movq {fold_b}, {crc32c_accumulator_2:r}",
                "pclmulqdq {fold_b}, {fold_constants}, 0x10",
                "pxor {fold_a}, {fold_b}",

                "movq {combine_word}, {fold_a}",

                // ptr_a is still block_base + 248, so this addresses
                // block_base + 760: the raw final QWORD of lane C.
                "xor {combine_word:r}, QWORD PTR [{ptr_a} + {lane_c_offset}]",

                "mov {crc32c_accumulator_1:r}, {crc32c_accumulator_3:r}",

                "crc32 {crc32c_accumulator_1:r}, {combine_word:r}",
                "lea {ptr_a}, [{ptr_a} + {block_advance}]",

                "sub {block_remaining}, 1",
                "jnz 30f",

                "4:",
                    "cmp {tail_remaining}, 8",
                    "jb 5f",
                    "crc32 {crc32c_accumulator_1:r}, QWORD PTR [{ptr_a}]",
                    "lea {ptr_a}, [{ptr_a}+8]",
                    "sub {tail_remaining}, 8",
                    "jmp 4b",

                "5:",
                    "cmp {tail_remaining}, 0",
                    "je 9f",

                    "test {tail_remaining}, 4",
                    "jnz 6f",

                    "test {tail_remaining}, 2",
                    "jnz 7f",

                    "test {tail_remaining}, 1",
                    "jnz 8f",

                    "jmp 9f",

                "6:",
                    "crc32 {crc32c_accumulator_1:e}, DWORD PTR [{ptr_a}]",
                    "lea {ptr_a}, [{ptr_a}+4]",
                    "sub {tail_remaining}, 4",
                    "jmp 5b",

                "7:",
                    "crc32 {crc32c_accumulator_1:e}, WORD PTR [{ptr_a}]",
                    "lea {ptr_a}, [{ptr_a}+2]",
                    "sub {tail_remaining}, 2",
                    "jmp 5b",

                "8:",
                    "crc32 {crc32c_accumulator_1:e}, BYTE PTR [{ptr_a}]",
                    "lea {ptr_a}, [{ptr_a}+1]",
                    "sub {tail_remaining}, 1",
                    "jmp 5b",

                "9:",
                    "xor {crc32c_accumulator_1:e}, 0xFFFFFFFF",
                    "jmp 31f",


            // --- 30: prepare the next 768-byte block ----------------------
            "30:",
                "mov {chunk_remaining}, {chunk_bytes}",

                // Keep acc1 as the running state; reset the auxiliary lanes.
                "xor {crc32c_accumulator_2:e}, {crc32c_accumulator_2:e}",
                "xor {crc32c_accumulator_3:e}, {crc32c_accumulator_3:e}",
                "jmp 2b",


            // --- 31: complete ---------------------------------------------
            "31:",


            crc32c_accumulator_1 = inout(reg) crc32c_accumulator_1,
            crc32c_accumulator_2 = inout(reg) crc32c_accumulator_2 => _,
            crc32c_accumulator_3 = inout(reg) crc32c_accumulator_3 => _,

            ptr_a = inout(reg) ptr_a => _,
            chunk_remaining = inout(reg) chunk_remaining => _,
            block_remaining = inout(reg) block_remaining => _,
            tail_remaining = inout(reg) tail_remaining => _,

            lane_b_offset = const LANE_B_OFFSET,
            lane_c_offset = const LANE_C_OFFSET,
            chunk_bytes = const CHUNK_BYTES,
            block_advance = const BLOCK_ADVANCE,

            k1_256 = in(reg) K1_256,
            k2_256 = in(reg) K2_256,

            fold_constants = lateout(xmm_reg) _,
            fold_a = lateout(xmm_reg) _,
            fold_b = lateout(xmm_reg) _,

            // MUST be `out`, not `lateout`. combine_word is written before the
            // `[ptr_a + 512]` load below it, and `lateout` lets the register
            // allocator reuse the still-live ptr_a register for it. That
            // produced a real STATUS_ACCESS_VIOLATION.
            combine_word = out(reg) _,

            // Reads through ptr_a but never writes memory, and never touches
            // the stack.
            options(readonly, nostack),
        );
    }

    crc32c_accumulator_1 as u32
}

#[cfg(test)]
mod tests {
    #[test]
    fn x86_64_parallel_many_blocks() {
        if !std::arch::is_x86_feature_detected!("sse4.2")
            || !std::arch::is_x86_feature_detected!("pclmulqdq")
        {
            return;
        }

        let mut buff = vec![0u8; 768 * 64];

        for (i, byte) in buff.iter_mut().enumerate() {
            *byte = ((i * 131 + 17) & 0xFF) as u8;
        }

        let expected = unsafe { super::super::x86_64::crc32c_u32(&buff) };
        let actual = unsafe { super::crc32c_pclmul_u32(&buff) };

        assert_eq!(actual, expected);
    }

    #[test]
    fn x86_64_parallel_known_crc32c_vectors() {
        if !std::arch::is_x86_feature_detected!("sse4.2")
            || !std::arch::is_x86_feature_detected!("pclmulqdq")
        {
            return;
        }

        for (input, expected) in crate::test_vectors::KNOWN {
            let actual = unsafe { super::crc32c_pclmul_u32(input) };

            assert_eq!(actual, expected, "CRC32C mismatch for input: {input:?}");
        }

        let mut buff = [0u8; 768];

        for (i, byte) in buff.iter_mut().enumerate() {
            *byte = (i % 256) as u8;
        }

        let expected = unsafe { super::super::x86_64::crc32c_u32(&buff) };
        let actual = unsafe { super::crc32c_pclmul_u32(&buff) };

        assert_eq!(actual, expected);

        buff.fill(0xFF);

        let expected = unsafe { super::super::x86_64::crc32c_u32(&buff) };
        let actual = unsafe { super::crc32c_pclmul_u32(&buff) };

        assert_eq!(actual, expected);
    }

    #[test]
    fn x86_64_parallel_two_blocks() {
        if !std::arch::is_x86_feature_detected!("sse4.2")
            || !std::arch::is_x86_feature_detected!("pclmulqdq")
        {
            return;
        }

        let mut buff = vec![0u8; 768 * 2];

        for (i, byte) in buff.iter_mut().enumerate() {
            *byte = (i % 251) as u8;
        }

        let expected = unsafe { super::super::x86_64::crc32c_u32(&buff) };
        let actual = unsafe { super::crc32c_pclmul_u32(&buff) };

        assert_eq!(actual, expected);
    }

    #[test]
    fn x86_64_parallel_with_seven_byte_tail() {
        if !std::arch::is_x86_feature_detected!("sse4.2")
            || !std::arch::is_x86_feature_detected!("pclmulqdq")
        {
            return;
        }

        let mut buff = vec![0u8; 768 * 2 + 7];

        for (i, byte) in buff.iter_mut().enumerate() {
            *byte = (i % 251) as u8;
        }

        let expected = unsafe { super::super::x86_64::crc32c_u32(&buff) };
        let actual = unsafe { super::crc32c_pclmul_u32(&buff) };

        assert_eq!(actual, expected);
    }

    #[test]
    fn x86_64_parallel_empty_input() {
        if !std::arch::is_x86_feature_detected!("sse4.2")
            || !std::arch::is_x86_feature_detected!("pclmulqdq")
        {
            return;
        }

        let expected = 0;
        let actual = unsafe { super::crc32c_pclmul_u32(b"") };

        assert_eq!(actual, expected);
    }
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
