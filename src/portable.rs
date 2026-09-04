//! Portable software CRC32C.
//!
//! This is the fallback used whenever no hardware CRC instructions are
//! available, so it runs on every non-x86_64/aarch64 target as well as on
//! x86_64 CPUs predating SSE4.2.
//!
//! It uses the slice-by-8 table method: eight precomputed 256-entry tables
//! let us retire eight bytes per iteration using eight *independent* lookups.
//! A bit-at-a-time loop serialises on one shift per bit, which is roughly an
//! order of magnitude slower.

/// The CRC32C (Castagnoli) polynomial in bit-reflected form.
const POLYNOMIAL: u32 = 0x82F6_3B78;

/// Slice-by-8 lookup tables, built at compile time.
///
/// 8 planes * 256 entries * 4 bytes = 8 KiB of read-only data.
///
/// This is a `static` rather than a `const` so the tables exist once in the
/// binary; a `const` would be materialized separately at every use site.
static TABLES: [[u32; 256]; 8] = build_tables();

/// Builds the slice-by-8 tables.
///
/// Written with `while` loops because `for` is not available in `const fn`.
const fn build_tables() -> [[u32; 256]; 8] {
    let mut tables = [[0u32; 256]; 8];

    // Plane 0 is the classic byte-at-a-time table: entry `i` is the CRC state
    // left behind by feeding byte `i` into a zeroed register. This is exactly
    // the inner loop of the naive implementation, just precomputed.
    let mut index = 0;
    while index < 256 {
        let mut entry = index as u32;

        let mut bit = 0;
        while bit < 8 {
            entry = if entry & 1 == 1 {
                (entry >> 1) ^ POLYNOMIAL
            } else {
                entry >> 1
            };

            bit += 1;
        }

        tables[0][index] = entry;
        index += 1;
    }

    // Plane `n` is plane `n - 1` advanced by one further byte position. That
    // is what lets us look up a byte that is still several positions away from
    // the CRC register without having to shift it through first.
    let mut plane = 1;
    while plane < 8 {
        let mut index = 0;

        while index < 256 {
            let previous = tables[plane - 1][index];

            tables[plane][index] = (previous >> 8) ^ tables[0][(previous & 0xFF) as usize];
            index += 1;
        }

        plane += 1;
    }

    tables
}

/// Computes CRC32C in software, with no CPU feature requirements.
///
/// Safe to call on any target. Bytes are combined with explicit little-endian
/// reads, so the result does not depend on the host's byte order.
pub fn crc32c_u32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;

    // `as_chunks` hands back fixed-size arrays rather than slices, so every
    // index below is provably in bounds and no bounds checks survive.
    let (chunks, remainder) = data.as_chunks::<8>();

    for chunk in chunks {
        // Fold the running CRC state into the first four bytes. The remaining
        // four bytes are indexed directly, because the higher planes already
        // account for their distance from the register.
        //
        // Bytes are read little-endian explicitly rather than through a native
        // load, so big-endian targets produce identical results.
        let folded = crc ^ u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);

        // All eight lookups are independent, so the CPU is free to issue them
        // in parallel and XOR the results at the end.
        crc = TABLES[7][(folded & 0xFF) as usize]
            ^ TABLES[6][((folded >> 8) & 0xFF) as usize]
            ^ TABLES[5][((folded >> 16) & 0xFF) as usize]
            ^ TABLES[4][(folded >> 24) as usize]
            ^ TABLES[3][chunk[4] as usize]
            ^ TABLES[2][chunk[5] as usize]
            ^ TABLES[1][chunk[6] as usize]
            ^ TABLES[0][chunk[7] as usize];
    }

    // Fewer than eight bytes remain; finish one byte at a time using plane 0.
    for &byte in remainder {
        crc = (crc >> 8) ^ TABLES[0][((crc ^ u32::from(byte)) & 0xFF) as usize];
    }

    crc ^ 0xFFFF_FFFF
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The original bit-at-a-time implementation.
    ///
    /// Retained as an independent oracle: it shares no tables or control flow
    /// with the slice-by-8 version, so agreement between the two is real
    /// evidence rather than a tautology.
    fn crc32c_bitwise(data: &[u8]) -> u32 {
        let mut crc = 0xFFFF_FFFFu32;

        for byte in data {
            let mut current = u32::from(*byte);

            for _ in 0..8 {
                let feedback = (current ^ crc) & 1;
                crc >>= 1;

                if feedback == 1 {
                    crc ^= POLYNOMIAL;
                }

                current >>= 1;
            }
        }

        crc ^ 0xFFFF_FFFF
    }

    #[test]
    fn portable_known_crc32c_vectors() {
        for (input, expected) in crate::test_vectors::KNOWN {
            let result = crc32c_u32(input);

            assert_eq!(
                result, expected,
                "Portable CRC32 mismatch for input: {input:?}"
            );
        }
    }

    #[test]
    fn portable_matches_bitwise() {
        let mut data = vec![0u8; 520];

        for (i, byte) in data.iter_mut().enumerate() {
            *byte = ((i * 131 + 17) & 0xFF) as u8;
        }

        // Sweep every length across many eight-byte chunk boundaries, from
        // several start offsets so the tail path is exercised from every
        // alignment and against varying data.
        for offset in 0..8 {
            for len in 0..=512 {
                let slice = &data[offset..offset + len];

                assert_eq!(
                    crc32c_u32(slice),
                    crc32c_bitwise(slice),
                    "mismatch at offset {offset} length {len}"
                );
            }
        }
    }
}
