/// Computes CRC32C using the x86_64 SSE4.2 CRC instructions.
///
/// # Safety
///
/// The caller must ensure that the executing CPU supports SSE4.2
/// before calling this function.
#[target_feature(enable = "sse4.2")]
pub unsafe fn crc32c_u32(data: &[u8]) -> u32 {
    // our data pointer
    let ptr = data.as_ptr();

    // our data length
    let remaining = data.len();

    // accumulator variable
    let mut crc32c_accumulator: u64 = 0xFFFF_FFFF;

    unsafe {
        /*
        Loop handling, rust doesnt like real lables, just numerics
            2:   ← fold_bits
            3:   ← fold_tail
            4:   ← hash_done
            5:   ← 4_byte_handler
            6:   ← 2_byte_handler
            7:   ← 1_byte_handler

        */
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
        );
    }

    // return accumulator
    crc32c_accumulator as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn x86_64_known_crc32c_vectors() {
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
            (b"12345678", 0x6087809A), // 8 bytes → 64-bit CRC32 main fold only
        ];

        for (input, expected) in test_cases {
            let result = unsafe { crc32c_u32(input) };

            assert_eq!(result, expected, "CRC32C mismatch for input: {:?}", input);
        }
    }
}
