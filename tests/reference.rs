use crc32c as reference_crc32c;
use crc32c_fast::crc32c;

/// Deterministic xorshift64* generator, so a failure reproduces exactly and no
/// dependency is needed just to produce test data.
struct Rng(u64);

impl Rng {
    fn fill(&mut self, buf: &mut [u8]) {
        for byte in buf.iter_mut() {
            let mut x = self.0;
            x ^= x >> 12;
            x ^= x << 25;
            x ^= x >> 27;
            self.0 = x;

            *byte = (x.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 32) as u8;
        }
    }
}

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

/// Sweeps every length up to 1 KiB from sixteen different start offsets.
///
/// The hardware backends consume 8 bytes at a time and then dispatch on the
/// low three bits of the remaining length, so this exercises every path through
/// the tail handlers against every alignment, with data that is not a simple
/// ascending pattern.
#[test]
fn matches_reference_across_lengths_and_alignments() {
    let mut rng = Rng(0x243F_6A88_85A3_08D3);

    let mut data = vec![0u8; 1024 + 16];
    rng.fill(&mut data);

    for offset in 0..16 {
        for len in 0..=1024 {
            let slice = &data[offset..offset + len];

            assert_eq!(
                crc32c(slice),
                reference_crc32c::crc32c(slice),
                "mismatch at offset {offset} length {len}"
            );
        }
    }
}

/// Sizes past the point where any plausible blocking or unrolling scheme would
/// change behaviour, which the fixed sub-257-byte cases above cannot reach.
#[test]
fn matches_reference_at_large_sizes() {
    let mut rng = Rng(0x1319_8A2E_0370_7344);

    for size in [4096usize, 65_536, 1_048_576, 1_048_576 + 7] {
        let mut data = vec![0u8; size];
        rng.fill(&mut data);

        assert_eq!(
            crc32c(&data),
            reference_crc32c::crc32c(&data),
            "mismatch at size {size}"
        );
    }
}
