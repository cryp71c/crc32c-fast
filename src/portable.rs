pub fn crc32c_u32(data: &[u8]) -> u32 {
    // initialize CRC
    let mut crc32c_portable_u32_accumulator = 0xFFFFFFFF;

    for byte in data {
        let mut u32_byte_copy = u32::from(*byte);

        for _ in 0..8 {
            let feedback = (u32_byte_copy ^ crc32c_portable_u32_accumulator) & 1;
            crc32c_portable_u32_accumulator >>= 1;

            if feedback == 1 {
                crc32c_portable_u32_accumulator ^= 0x82F63B78;
            }

            u32_byte_copy >>= 1;
        }
    }

    crc32c_portable_u32_accumulator ^= 0xFFFFFFFF;

    crc32c_portable_u32_accumulator
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portable_known_crc32c_vectors() {
        let test_cases: [(&[u8], u32); 5] = [
            (b"", 0x00000000),
            (b"12", 0x7355C460),
            (b"123", 0x107B2FB2),
            (b"1234", 0xF63AF4EE),
            (b"123456789", 0xE3069283),
        ];

        for (input, expected) in test_cases {
            let result = crc32c_u32(input);

            assert_eq!(
                result, expected,
                "Portable CRC32 mismatch for input: {:?}",
                input
            );
        }
    }
}
