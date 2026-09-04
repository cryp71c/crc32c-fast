#[cfg(target_arch = "x86_64")]
use crc32c_fast::x86_64;

#[cfg(target_arch = "x86_64")]
#[path = "../benches/experimental/x86_64_parallel.rs"]
mod x86_64_parallel;
