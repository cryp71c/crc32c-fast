# AI Usage Statement

AI-assisted tools were used during the development of `crc32c-fast`.

I am including this statement voluntarily because I believe software projects should be able to describe how AI contributed to their development in the same way they document other tools, dependencies, testing methods, and sources of outside assistance.

## How AI was used

AI was used as a development and review tool throughout portions of this project, including:

- discussing CRC32C implementation details and CPU architecture behavior
- reviewing Rust and inline assembly for x86_64 and AArch64
- exploring performance characteristics and optimization strategies
- assisting with the experimental multi-chain x86_64 CRC32C implementation
- reviewing safety contracts around CPU feature detection and unsafe code
- suggesting test cases, portability checks, and CI improvements
- reviewing package structure and release preparation
- helping write and refine documentation and code comments
- acting as an additional code-review perspective during pre-release auditing

Both OpenAI ChatGPT and Anthropic Claude were used during development and review. Specifically the `Opus 5` model and `GPT-5.6 Sol` models.

AI-generated suggestions were not treated as authoritative. Changes were inspected, modified where necessary, compiled, tested, benchmarked, and validated against independent CRC32C implementations before being retained.

## Human involvement

The project remains human-directed.

I made the architectural and implementation decisions, performed the development work, reviewed proposed changes, ran the test and benchmark suites, investigated failures, and decided what ultimately entered the codebase.

In particular, performance claims are based on measurements from actual compiled code rather than AI estimates, and correctness is checked against independent implementations and known CRC32C test vectors.

The project contains unsafe Rust and handwritten assembly. AI assistance does not replace the responsibility to understand, review, and validate that code.

## Why disclose this?

AI-assisted software development is becoming increasingly common, but the extent of that assistance is often invisible.

I would rather make it explicit.

My goal with this statement is not to distinguish between "AI-written" and "human-written" lines of code. Development was iterative, and that distinction quickly becomes misleading once code has been discussed, rewritten, benchmarked, debugged, and reviewed multiple times.

Instead, this document describes the role AI actually played in the engineering process and makes clear where responsibility for the resulting software remains.

All final responsibility for the contents of this repository remains with the maintainer.
