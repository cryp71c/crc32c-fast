use crc32c as reference_crc32c;
use crc32c_fast::crc32c;

#[test]
fn matches_reference_implementation() {
    let sizes = [0usize, 1, 2, 3, 4, 7, 8, 9, 15, 16, 17, 255, 256, 257];

    for size in sizes {
        let mut data = vec![0u8; size];

        for (i, byte) in data.iter_mut().enumerate() {
            *byte = (i % 256) as u8;
        }

        let ours = crc32c(&data);

        let reference = reference_crc32c::crc32c(&data);

        assert_eq!(
            ours, reference,
            "CRC32C mismatch at input size {} bytes",
            size
        );
    }
}
