// Inline CRC32C integrity checking algorithm.
// This is a aarch64 implementation.
/// # Safety
///
/// The caller must ensure that the executing CPU supports crc
/// before calling this function.
#[target_feature(enable = "crc")]
pub unsafe fn crc32c_u32(data: &[u8]) -> u32 {
    // our data pointer
    let ptr = data.as_ptr();

    // our data length
    let remaining = data.len();

    // accumulator variable
    let mut crc32c_accumulator = 0xFFFFFFFF;

    unsafe {
        /*
        Loop handling, rust doesnt like real lables, just numerics
        This is a 64 bit implementation of crc32 aarch64
            2:   ← fold_bits
            3:   ← fold_tail
            4:   ← hash_done
            5:   ← 1_byte_handler
            6:   ← 2_byte_handler
            7:   ← 4_byte_handler
        */

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
        )
    }

    crc32c_accumulator
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aarch64_known_crc32c_vectors() {
        let test_cases: [(&[u8], u32); 10] = [
            (b"", 0x00000000),
            (b"1", 0x90F599E3), // 1 byte → BYTE PTR tail only
            (b"12", 0x7355C460),
            (b"123", 0x107B2FB2),
            (b"1234", 0xF63AF4EE),
            (b"123456789", 0xE3069283),
            (b"12345", 0x18D12335),    // 5 bytes → 4 + 1
            (b"123456", 0x41357186),   // 6 bytes → 4 + 2
            (b"1234567", 0x124297EA),  // 7 bytes → 4 + 2 + 1
            (b"12345678", 0x6087809A), // 8 bytes → CRC32CX only
        ];

        for (input, expected) in test_cases {
            let result = unsafe { crc32c_u32(input) };

            assert_eq!(result, expected, "CRC32C mismatch for input: {:?}", input);
        }
    }
}
