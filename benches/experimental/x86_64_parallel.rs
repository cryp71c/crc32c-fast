// Hardware-accelerated CRC32C for x86_64 using three independent SSE4.2 CRC
// dependency chains, recombined with PCLMULQDQ.
//
// CONTRACT:
//
//   - input length must be a non-zero exact multiple of 768 bytes
//   - CPU must support SSE4.2 and PCLMULQDQ
//
// This is not yet a general arbitrary-length implementation.

/// # Safety
///
/// The caller must ensure that the executing CPU supports SSE4.2
/// and pclmulqdq before calling this function.
#[target_feature(enable = "sse4.2,pclmulqdq")]
pub unsafe fn crc32c_u32(data: &[u8]) -> u32 {
    // One block is three 256-byte lanes, each processed as an independent CRC
    // stream:
    //
    //   A: bytes   0..256
    //   B: bytes 256..512
    //   C: bytes 512..768
    const LANE_BYTES: usize = 256;
    const PARALLEL_BYTES: usize = LANE_BYTES * 3;

    // Lanes B and C are addressed as fixed displacements from the lane-A
    // pointer, so only one pointer ever has to move. Passed to the asm block as
    // `const` operands, these become immediates and cost no registers.
    const LANE_B_OFFSET: usize = LANE_BYTES;
    const LANE_C_OFFSET: usize = LANE_BYTES * 2;

    // The main loop processes only 31 of each lane's 32 QWORDs. A[248..256] and
    // B[248..256] are handled just after it; C[248..256] is deliberately left
    // raw for the combine step.
    const CHUNK_BYTES: usize = LANE_BYTES - 8;

    // The main loop ends with ptr_a at block_base + 248, while the next block
    // starts at block_base + 768.
    const BLOCK_ADVANCE: usize = PARALLEL_BYTES - CHUNK_BYTES;

    let remaining = data.len();

    assert!(
        remaining != 0 && remaining.is_multiple_of(PARALLEL_BYTES),
        "input length must be a non-zero multiple of {PARALLEL_BYTES} bytes"
    );

    let block_remaining = remaining / PARALLEL_BYTES;
    let chunk_remaining = CHUNK_BYTES;
    let ptr_a = data.as_ptr();

    // Only lane A carries the real CRC32C initial state, because A is the start
    // of the actual byte stream. B and C are independent contributions that get
    // folded into A later, so they start at zero. That is valid because CRC
    // arithmetic is linear over GF(2) once we work with the raw internal state.
    let mut crc32c_accumulator_1: u64 = 0xFFFF_FFFF;
    let crc32c_accumulator_2: u64 = 0;
    let crc32c_accumulator_3: u64 = 0;

    // Folding constants for a 256-byte lane, taken from entry 32 of the
    // Linux/Intel CRC32C folding table, which stores it as:
    //
    //      0xdd7e3b0c, 0xb9e02b86
    //
    // They shift a CRC state back into the position it would have occupied had
    // the whole block been processed sequentially.
    const K1_256: u64 = 0xB9E0_2B86;
    const K2_256: u64 = 0xDD7E_3B0C;

    unsafe {
        // Numeric local labels are required: an asm! block may be duplicated by
        // inlining, which makes named assembler symbols unsafe.
        //
        //   2  = three-way CRC loop
        //   3  = finish lanes A and B, then fall through to the combine
        //   30 = advance to the next 768-byte block
        //   31 = done
        core::arch::asm!(
            // --- 2: three independent CRC streams -------------------------
            "2:",
                "test {chunk_remaining}, {chunk_remaining}",
                "jz 3f",

                // Three *different* destination registers, so acc1, acc2 and
                // acc3 do not depend on each other. That is the whole point:
                // the CPU keeps three CRC32 chains in flight instead of
                // serialising on one.
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
            //
            //   acc1 = CRC(A[0..256])
            //   acc2 = CRC(B[0..256]), zero seeded
            //   acc3 = CRC(C[0..248]), zero seeded
            //
            // Turn those into the CRC that sequentially processing A || B || C
            // would have produced.

                // Pack the constants into one register as [ K1 | K2 ].
                "movq {fold_constants}, {k2_256:r}",
                "movq {fold_b}, {k1_256:r}",
                "punpcklqdq {fold_constants}, {fold_b}",

                // folded = (acc1 x K2) XOR (acc2 x K1), carry-less.
                //
                // The immediate picks which QWORD of each operand to multiply:
                // 0x00 takes both low halves (K2), 0x10 takes the high half of
                // fold_constants (K1). XOR is addition in GF(2).
                "movq {fold_a}, {crc32c_accumulator_1:r}",
                "pclmulqdq {fold_a}, {fold_constants}, 0x00",
                "movq {fold_b}, {crc32c_accumulator_2:r}",
                "pclmulqdq {fold_b}, {fold_constants}, 0x10",
                "pxor {fold_a}, {fold_b}",

                // 64 bits is enough to hold the result: the accumulators and
                // the constants are all degree <32, so the carry-less product
                // cannot exceed it.
                "movq {combine_word}, {fold_a}",

                // ptr_a is still block_base + 248, so this addresses
                // block_base + 760: the raw final QWORD of lane C.
                "xor {combine_word:r}, QWORD PTR [{ptr_a} + {lane_c_offset}]",

                // Continue from C's prefix state, in acc1, which is the
                // accumulator we keep as the running result.
                "mov {crc32c_accumulator_1:r}, {crc32c_accumulator_3:r}",

                // acc1 is now equivalent to having processed all 768 bytes
                // sequentially.
                "crc32 {crc32c_accumulator_1:r}, {combine_word:r}",

                "sub {block_remaining}, 1",
                "jnz 30f",

                // The final XOR is applied exactly once, here at the end.
                // Between blocks acc1 must stay a raw internal CRC state.
                "xor {crc32c_accumulator_1:e}, 0xFFFFFFFF",
                "jmp 31f",


            // --- 30: prepare the next 768-byte block ----------------------
            "30:",
                "lea {ptr_a}, [{ptr_a} + {block_advance}]",
                "mov {chunk_remaining}, {chunk_bytes}",

                // acc1 must NOT be cleared: it holds the running CRC of every
                // block so far and seeds lane A of the next one. B and C are
                // contributions internal to a block, so they restart at zero.
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
        let mut buff = vec![0u8; 768 * 64];

        for (i, byte) in buff.iter_mut().enumerate() {
            *byte = ((i * 131 + 17) & 0xFF) as u8;
        }

        let expected = unsafe { super::super::x86_64::crc32c_u32(&buff) };
        let actual = unsafe { super::crc32c_u32(&buff) };

        assert_eq!(actual, expected);
    }

    #[test]
    fn x86_64_parallel_known_crc32c_vectors() {
        let mut buff = [0u8; 768];

        for (i, byte) in buff.iter_mut().enumerate() {
            *byte = (i % 256) as u8;
        }

        let expected = unsafe { super::super::x86_64::crc32c_u32(&buff) };
        let actual = unsafe { super::crc32c_u32(&buff) };

        assert_eq!(actual, expected);

        buff.fill(0xFF);

        let expected = unsafe { super::super::x86_64::crc32c_u32(&buff) };
        let actual = unsafe { super::crc32c_u32(&buff) };

        assert_eq!(actual, expected);
    }

    #[test]
    fn x86_64_parallel_two_blocks() {
        let mut buff = vec![0u8; 768 * 2];

        for (i, byte) in buff.iter_mut().enumerate() {
            *byte = (i % 251) as u8;
        }

        let expected = unsafe { super::super::x86_64::crc32c_u32(&buff) };
        let actual = unsafe { super::crc32c_u32(&buff) };

        assert_eq!(actual, expected);
    }
}
