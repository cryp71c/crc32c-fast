#[cfg(target_arch = "x86_64")]
use crc32c_fast::__bench::x86_64;

/// Deterministic xorshift64* generator, so a failure reproduces exactly and no
/// dependency is needed just to produce test data.
#[cfg(target_arch = "x86_64")]
struct Rng(u64);

#[cfg(target_arch = "x86_64")]
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

/// Checks the PCLMUL backend against an independent implementation.
///
/// This is what actually validates the PCLMULQDQ folding constants K1/K2. The
/// in-module tests only prove the parallel and serial backends agree with each
/// other; a wrong constant could in principle produce a self-consistent but
/// incorrect checksum. Comparing against the `crc32c` crate rules that out.
#[cfg(target_arch = "x86_64")]
#[test]
fn pclmul_matches_reference_across_block_counts() {
    if !std::arch::is_x86_feature_detected!("sse4.2")
        || !std::arch::is_x86_feature_detected!("pclmulqdq")
    {
        return;
    }

    let mut rng = Rng(0x243F_6A88_85A3_08D3);

    for blocks in 1..=24usize {
        let mut data = vec![0u8; 768 * blocks];
        rng.fill(&mut data);

        let actual = unsafe { x86_64::crc32c_pclmul_u32(&data) };

        assert_eq!(actual, crc32c::crc32c(&data), "block count {blocks}");
    }
}
