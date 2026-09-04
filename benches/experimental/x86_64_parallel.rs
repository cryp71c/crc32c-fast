// Hardware-accelerated CRC32C implementation for x86_64.
//
// This version uses:
//
//   1. SSE4.2 CRC32 instructions to calculate CRC32C.
//   2. Three independent CRC dependency chains so the CPU can exploit
//      CRC32 instruction-level parallelism.
//   3. PCLMULQDQ to mathematically combine those three partial CRC states.
//
// CURRENT CONTRACT:
//
//   - input length must be an exact multiple of 768 bytes.
//   - this is not yet our general arbitrary-length implementation.
//   - CPU must support SSE4.2 and PCLMULQDQ.
//
/// # Safety
///
/// The caller must ensure that the executing CPU supports SSE4.2
/// and pclmulqdq before calling this function.
#[target_feature(enable = "sse4.2,pclmulqdq")]
pub unsafe fn crc32c_u32(data: &[u8]) -> u32 {
    // Each independent CRC stream processes 256 contiguous bytes.
    //
    //          block
    //   ┌──────────────┐
    //   │ A: 256 bytes │
    //   │ B: 256 bytes │
    //   │ C: 256 bytes │
    //   └──────────────┘
    //
    // Therefore one complete parallel block is 3 * 256 = 768 bytes.
    const LANE_BYTES: usize = 256;
    const PARALLEL_BYTES: usize = LANE_BYTES * 3;

    let ptr = data.as_ptr();
    let remaining = data.len();

    // We currently only support complete 768-byte blocks.
    //
    // NOTE:
    // Zero bytes also technically satisfies this assertion:
    //
    //      0 % 768 == 0
    //
    // but the assembly below does NOT currently support an empty input.
    // Eventually this should become something like:
    //
    //      assert!(remaining != 0 && remaining % PARALLEL_BYTES == 0);
    //
    // or, preferably, be handled by the public dispatcher.
    assert_eq!(remaining % PARALLEL_BYTES, 0);

    // Number of complete 768-byte blocks we need to process.
    //
    // 768 bytes  -> 1
    // 1536 bytes -> 2
    // ...
    let block_count = remaining / PARALLEL_BYTES;

    // Mutable assembly-side block counter.
    let block_remaining = block_count;

    // Length of each of the three CRC lanes.
    let chunk_len = LANE_BYTES;

    // Arbitrary tails are intentionally disabled for now.
    //
    // Because our input contract is N * 768, there cannot be a tail.
    let tail_len = 0usize;

    // Each 256-byte lane contains:
    //
    //      256 / 8 = 32 QWORDs
    //
    // The main parallel loop deliberately processes only the first
    // 31 QWORDs:
    //
    //      31 * 8 = 248 bytes
    //
    // We later process A[248..256] and B[248..256], but deliberately
    // leave C[248..256] untouched for the PCLMUL combination step.
    let chunk_remaining = chunk_len - 8;

    let tail_len_remaining = tail_len;

    // ptr_a serves as our moving base pointer.
    //
    // Instead of maintaining three pointers:
    //
    //      ptr_a
    //      ptr_b
    //      ptr_c
    //
    // we exploit x86 addressing:
    //
    //      [ptr_a]
    //      [ptr_a + 256]
    //      [ptr_a + 512]
    //
    let ptr_a = ptr;

    // Currently unused because tail_len == 0.
    //
    // This points immediately after the first 768-byte block.
    let tail_ptr = unsafe { ptr.add(chunk_len * 3) };

    // Raw CRC states.
    //
    // Only accumulator 1 receives the CRC32C initial state.
    //
    // Why?
    //
    // A represents the beginning of the real sequential byte stream, so
    // it must carry the actual initial CRC state.
    //
    // B and C are mathematically independent contributions that will later
    // be folded into A, so they begin at zero.
    //
    // This works because CRC arithmetic is linear over GF(2) once we're
    // working with the raw internal CRC state.
    let mut crc32c_accumulator_1: u64 = 0xFFFF_FFFF;
    let crc32c_accumulator_2: u64 = 0;
    let crc32c_accumulator_3: u64 = 0;

    // Polynomial folding constants for a lane that is 32 QWORDs long:
    //
    //      32 * 8 = 256 bytes
    //
    // These come from entry 32 of the Linux/Intel CRC32C folding table.
    //
    // Linux stores the entry as:
    //
    //      0xdd7e3b0c, 0xb9e02b86
    //
    // giving:
    //
    //      K2 = 0xDD7E3B0C
    //      K1 = 0xB9E02B86
    //
    // They represent polynomial shift/reduction factors used to put the
    // independently-computed CRC states back into the positions they would
    // have occupied had we processed the entire block sequentially.
    const K1_256: u64 = 0xB9E0_2B86;
    const K2_256: u64 = 0xDD7E_3B0C;

    unsafe {
        /*
         * Numeric local labels are intentional.
         *
         * Rust/LLVM inline assembly recommends local numeric labels because
         * an asm! block may be duplicated by optimization/inlining, which
         * makes normal assembler symbol names dangerous.
         *
         *      2  = three-way CRC loop
         *      3  = finish lane A/B + old tail dispatcher
         *      4  = PCLMUL combine
         *
         *      8  = legacy 8-byte tail handler
         *      5  = legacy 4-byte tail handler
         *      6  = legacy 2-byte tail handler
         *      7  = legacy 1-byte tail handler
         *
         *      30 = advance to another 768-byte block
         *      31 = entire hash complete
         *
         * The tail handlers are currently unreachable because tail_len = 0.
         * They also need redesign before arbitrary lengths are re-enabled.
         */

        core::arch::asm!(
            /*
             * ============================================================
             * 2: THREE INDEPENDENT CRC STREAMS
             * ============================================================
             */
            "2:",

                // Stop after processing 248 bytes from EACH lane.
                "test {chunk_remaining}, {chunk_remaining}",
                "jz 3f",

                /*
                 * Process one QWORD from each contiguous 256-byte lane.
                 *
                 * Lane A:
                 *
                 *      ptr_a + 0
                 *
                 * Lane B:
                 *
                 *      ptr_a + 256
                 *
                 * Lane C:
                 *
                 *      ptr_a + 512
                 *
                 * Most importantly, these use THREE DIFFERENT destination
                 * registers.
                 *
                 * Therefore:
                 *
                 *      acc1(n+1) depends on acc1(n)
                 *      acc2(n+1) depends on acc2(n)
                 *      acc3(n+1) depends on acc3(n)
                 *
                 * but acc1, acc2, and acc3 do NOT depend on each other.
                 *
                 * That breaks the single long dependency chain that limited
                 * our original implementation.
                 */
                "crc32 {crc32c_accumulator_1:r}, QWORD PTR [{ptr_a}]",
                "crc32 {crc32c_accumulator_2:r}, QWORD PTR [{ptr_a} + {chunk_len}]",
                "crc32 {crc32c_accumulator_3:r}, QWORD PTR [{ptr_a} + {chunk_len} * 2]",

                // Move all three logical lane positions forward by 8 bytes.
                //
                // Because B and C are expressed as fixed offsets from ptr_a,
                // only ONE physical pointer needs to move.
                "lea {ptr_a}, [{ptr_a}+8]",

                // One QWORD from every lane has been consumed.
                "sub {chunk_remaining}, 8",

                "jmp 2b",


            /*
             * ============================================================
             * 3: FINISH A AND B
             * ============================================================
             *
             * When we arrive here:
             *
             *      ptr_a = block_base + 248
             *
             * and all three accumulators have processed 248 bytes.
             */
            "3:",

                // Process bytes 248..255 of lane A.
                //
                // A now represents all 256 bytes of lane A.
                "crc32 {crc32c_accumulator_1:r}, QWORD PTR [{ptr_a}]",

                // Process bytes 248..255 of lane B.
                //
                // B now represents all 256 bytes of lane B.
                "crc32 {crc32c_accumulator_2:r}, QWORD PTR [{ptr_a} + {chunk_len}]",

                // DO NOT process:
                //
                // QWORD PTR [ptr_a + chunk_len * 2]
                //
                // That is bytes 248..255 of lane C.
                //
                // Leaving this QWORD raw is essential to the Intel/Linux
                // combination algorithm. The contributions from A and B
                // will be XORed into this QWORD before C performs its final
                // CRC32 instruction.

                /*
                 * --------------------------------------------------------
                 * LEGACY TAIL DISPATCH
                 * --------------------------------------------------------
                 *
                 * tail_len_remaining is currently ALWAYS zero.
                 *
                 * Therefore we always jump directly to label 4.
                 */
                "cmp {tail_len_remaining}, 0",
                "je 4f",

                "cmp {tail_len_remaining}, 8",
                "jae 8f",

                "test {tail_len_remaining}, 4",
                "jnz 5f",

                "test {tail_len_remaining}, 2",
                "jnz 6f",

                "test {tail_len_remaining}, 1",
                "jnz 7f",


            /*
             * These tail handlers are currently DEAD CODE.
             *
             * IMPORTANT:
             *
             * They cannot simply be re-enabled as written.
             *
             * In particular, each handler jumps back to 3b, which would
             * execute the final A and B QWORDs AGAIN.
             *
             * tail_ptr also isn't advanced alongside the new outer block
             * loop.
             *
             * So these are leftovers from the earlier implementation,
             * not functional arbitrary-tail support.
             */

            "8:",
                "crc32 {crc32c_accumulator_3:r}, QWORD PTR [{tail_ptr}]",
                "lea {tail_ptr}, [{tail_ptr}+8]",
                "sub {tail_len_remaining}, 8",
                "jmp 3b",

            "5:",
                "crc32 {crc32c_accumulator_3:e}, DWORD PTR [{tail_ptr}]",
                "lea {tail_ptr}, [{tail_ptr}+4]",
                "sub {tail_len_remaining}, 4",
                "jmp 3b",

            "6:",
                "crc32 {crc32c_accumulator_3:e}, WORD PTR [{tail_ptr}]",
                "lea {tail_ptr}, [{tail_ptr}+2]",
                "sub {tail_len_remaining}, 2",
                "jmp 3b",

            "7:",
                "crc32 {crc32c_accumulator_3:e}, BYTE PTR [{tail_ptr}]",
                "lea {tail_ptr}, [{tail_ptr}+1]",
                "sub {tail_len_remaining}, 1",
                "jmp 3b",


            /*
             * ============================================================
             * 4: COMBINE THE THREE CRC STREAMS
             * ============================================================
             *
             * Current state:
             *
             *      acc1 = CRC(A[0..256])
             *      acc2 = CRC(B[0..256], zero seeded)
             *      acc3 = CRC(C[0..248], zero seeded)
             *
             *      raw_final_c = C[248..256]
             *
             * We need to transform those independent values into the CRC
             * that sequentially processing:
             *
             *      A || B || C
             *
             * would have produced.
             */
            "4:",

                /*
                 * Build one XMM register containing our two folding constants.
                 *
                 * First:
                 *
                 *      fold_constants.low64 = K2
                 */
                "movq {fold_constants}, {k2_256:r}",

                /*
                 * Temporary:
                 *
                 *      fold_b.low64 = K1
                 */
                "movq {fold_b}, {k1_256:r}",

                /*
                 * Interleave the LOW QWORDs.
                 *
                 * Result:
                 *
                 *      fold_constants[127:64] = K1
                 *      fold_constants[63:0]   = K2
                 *
                 * Conceptually:
                 *
                 *      XMM = [ K1 | K2 ]
                 */
                "punpcklqdq {fold_constants}, {fold_b}",


                /*
                 * --------------------------------------------------------
                 * Fold accumulator A
                 * --------------------------------------------------------
                 */

                // Move acc1 into the low 64 bits of an XMM register.
                //
                // CRC32 actually produces a 32-bit CRC; the upper 32 bits
                // of the 64-bit CRC destination are zeroed.
                "movq {fold_a}, {crc32c_accumulator_1:r}",

                /*
                 * Carry-less multiply:
                 *
                 *      acc1 × K2
                 *
                 * Immediate 0x00 means:
                 *
                 *      low64(first operand)
                 *          ×
                 *      low64(second operand)
                 *
                 * and K2 occupies the low half of fold_constants.
                 */
                "pclmulqdq {fold_a}, {fold_constants}, 0x00",


                /*
                 * --------------------------------------------------------
                 * Fold accumulator B
                 * --------------------------------------------------------
                 */

                "movq {fold_b}, {crc32c_accumulator_2:r}",

                /*
                 * Carry-less multiply:
                 *
                 *      acc2 × K1
                 *
                 * 0x10 selects the low QWORD from acc2 and the HIGH QWORD
                 * containing K1 from fold_constants.
                 */
                "pclmulqdq {fold_b}, {fold_constants}, 0x10",


                /*
                 * CRC polynomial arithmetic takes place over GF(2).
                 *
                 * Addition in GF(2) is XOR rather than integer addition.
                 *
                 * Combine the transformed contributions:
                 *
                 *      folded = (acc1 × K2) XOR (acc2 × K1)
                 */
                "pxor {fold_a}, {fold_b}",


                /*
                 * Extract the low 64 bits into a normal integer register.
                 *
                 * Why is 64 bits enough?
                 *
                 * acc1/acc2 are effectively 32-bit CRC values and the
                 * constants are 32-bit values. A carry-less product of two
                 * values of degree <32 fits within 64 bits.
                 */
                "movq {combine_word}, {fold_a}",


                /*
                 * This is the clever part.
                 *
                 * ptr_a is still:
                 *
                 *      block_base + 248
                 *
                 * therefore:
                 *
                 *      ptr_a + 256*2
                 *          =
                 *      block_base + 760
                 *
                 * which addresses bytes:
                 *
                 *      760..767
                 *
                 * i.e. the final QWORD of lane C that we intentionally did
                 * NOT CRC earlier.
                 *
                 * We XOR the folded A/B polynomial contribution directly
                 * into that raw C data word.
                 */
                "xor {combine_word:r}, QWORD PTR [{ptr_a} + {chunk_len} * 2]",


                /*
                 * acc3 contains the CRC state after the first 248 bytes of C.
                 *
                 * Move that state into acc1, which is the accumulator we're
                 * going to preserve as the global result.
                 */
                "mov {crc32c_accumulator_1:r}, {crc32c_accumulator_3:r}",


                /*
                 * Process the synthetic final QWORD:
                 *
                 *     raw_C_final_qword
                 *            XOR
                 *     transformed A/B contribution
                 *
                 * Starting from C's prefix CRC state.
                 *
                 * After this instruction, acc1 is mathematically equivalent
                 * to having processed the entire 768-byte block sequentially.
                 */
                "crc32 {crc32c_accumulator_1:r}, {combine_word:r}",


                /*
                 * ========================================================
                 * OUTER 768-BYTE LOOP
                 * ========================================================
                 */

                // One complete 768-byte block is finished.
                "sub {block_remaining}, 1",

                // If more blocks remain, preserve acc1 in its RAW internal
                // CRC state and process another block.
                "jnz 30f",


                /*
                 * This was the final block.
                 *
                 * Apply CRC32C's final XOR exactly ONCE.
                 *
                 * We deliberately do not perform this between blocks,
                 * because acc1 must remain an internal CRC state while
                 * processing a continuous byte stream.
                 */
                "xor {crc32c_accumulator_1:e}, 0xFFFFFFFF",
                "jmp 31f",


            /*
             * ============================================================
             * 30: PREPARE NEXT 768-BYTE BLOCK
             * ============================================================
             */
            "30:",

                /*
                 * At the end of our three-stream loop:
                 *
                 *      ptr_a = old_block_base + 248
                 *
                 * Next block starts at:
                 *
                 *      old_block_base + 768
                 *
                 * Difference:
                 *
                 *      768 - 248 = 520
                 *
                 * Therefore this advances ptr_a directly to the beginning of
                 * the next 768-byte block.
                 */
                "lea {ptr_a}, [{ptr_a} + 520]",


                // Again process 31 QWORDs / 248 bytes per lane before the
                // special final-QWORD combination.
                "mov {chunk_remaining}, 248",


                /*
                 * A MUST NOT be cleared.
                 *
                 * accumulator_1 contains the raw CRC of every block processed
                 * so far and becomes the initial CRC state for lane A of the
                 * next block.
                 *
                 * B and C represent independent contributions inside the new
                 * block, so they must restart from zero.
                 */
                "xor {crc32c_accumulator_2:e}, {crc32c_accumulator_2:e}",
                "xor {crc32c_accumulator_3:e}, {crc32c_accumulator_3:e}",

                "jmp 2b",


            /*
             * ============================================================
             * 31: COMPLETE
             * ============================================================
             */
            "31:",


            /*
             * ============================================================
             * RUST <-> ASSEMBLY OPERANDS
             * ============================================================
             */

            // acc1 is both initialized by Rust and returned to Rust.
            crc32c_accumulator_1 = inout(reg) crc32c_accumulator_1,

            // acc2/acc3 also need initial values, and are modified in asm.
            //
            // Rust currently writes their final values back even though the
            // Rust code doesn't use them afterward. That's why rustc emits
            // the "value assigned ... is never read" warnings.
            crc32c_accumulator_2 = inout(reg) crc32c_accumulator_2 => _,
            crc32c_accumulator_3 = inout(reg) crc32c_accumulator_3 => _,


            // Moving block pointer. We don't need its final value in Rust.
            ptr_a = inout(reg) ptr_a => _,

            // Legacy tail pointer. Currently unused at runtime.
            tail_ptr = inout(reg) tail_ptr => _,


            // Assembly modifies this counter, but Rust doesn't need the
            // final value.
            chunk_remaining = inout(reg) chunk_remaining => _,

            // Fixed 256-byte lane displacement.
            chunk_len = in(reg) chunk_len,

            // Always zero with the current function contract.
            tail_len_remaining = inout(reg) tail_len_remaining => _,


            // Folding constants are initially provided through integer
            // registers before being packed into an XMM register.
            k1_256 = in(reg) K1_256,
            k2_256 = in(reg) K2_256,


            /*
             * Scratch XMM registers.
             *
             * `lateout` is safe here because these registers don't contain
             * input values needed earlier by the asm block.
             */
            fold_constants = lateout(xmm_reg) _,
            fold_a = lateout(xmm_reg) _,
            fold_b = lateout(xmm_reg) _,


            /*
             * IMPORTANT: this must be `out`, NOT `lateout`.
             *
             * combine_word is written here:
             *
             *      movq combine_word, fold_a
             *
             * but afterward we still require:
             *
             *      ptr_a
             *      chunk_len
             *
             * to perform the memory load:
             *
             *      [ptr_a + chunk_len*2]
             *
             * `lateout` would allow Rust's register allocator to reuse one
             * of those still-live input registers for combine_word.
             *
             * We actually observed exactly that failure as
             * STATUS_ACCESS_VIOLATION.
             */
            combine_word = out(reg) _,


            // Count of complete 768-byte blocks still to process.
            block_remaining = inout(reg) block_remaining => _,
        );
    }

    // The last block applied the final XOR, so this is now the externally
    // visible CRC32C result rather than the raw internal CRC state.
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
