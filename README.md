# FT-FHE: Software Resilience for Number Theoretic Transforms

This repository contains the reference implementation accompanying the paper:

> **Directions for Software Resilience for Number Theoretic Transforms**
> *(authors omitted for anonymous review / or insert citation after publication)*

The framework provides a configurable implementation of radix-2 Number Theoretic Transforms (NTTs) with software-implemented resilience mechanisms, systematic fault injection, and end-to-end validation using a CKKS-inspired computation pipeline.

## Features

- Radix-2 Cooley-Tukey NTT and inverse NTT
- Multiple NTT backends
- Configurable arithmetic fault injection
- Fine-grained butterfly consistency invariants
- Stage-level checksum detection (Sum and Sum+Index)
- Selective recomputation for transient fault recovery
- CKKS-inspired encoding and decoding pipeline
- Comprehensive runtime metrics
- Automated experimental campaigns
- Unit and CLI integration tests

---

## Repository Structure

```
src/          Rust implementation
tests/        Unit and CLI tests
scripts/      Experiment and evaluation scripts
analysis/     Analysis utilities
results/      Example campaign outputs
docs/         Additional documentation
```

---

## Building

Requirements:

- Rust (stable toolchain)

Build the project:

```bash
cargo build --release
```

Run all tests:

```bash
cargo test
```

Run formatting and linting:

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
```

---

## Quick Start

Run the CKKS demonstration:

```bash
cargo run --release -- ckks-demo \
    --n 2048 \
    --bits 54 \
    --validate
```

---

## Fault Injection Example

Inject a transient arithmetic fault during the forward NTT:

```bash
cargo run --release -- ckks-demo \
    --n 2048 \
    --bits 54 \
    --fault \
    --fault-op ntt \
    --fault-site butterfly-output \
    --fault-stage 0 \
    --fault-slot 0 \
    --fault-bit 40 \
    --validate
```

---

## Butterfly Invariants

Run with butterfly consistency checking:

```bash
cargo run --release -- ckks-demo \
    --fault \
    --mitigation butterfly-check \
    --mitigation-action recompute
```

---

## Stage Checksums

Run Sum+Index checksum verification:

```bash
cargo run --release -- ckks-demo \
    --fault \
    --mitigation stage-checksum \
    --checksum-mode sum-index \
    --mitigation-action detect-only
```

---

## Reproducing the Experiments

The `scripts/` directory contains the automation used for the paper, including:

- fault-injection campaigns
- checksum evaluation
- runtime-overhead measurements
- practical fault models
- large-ring experiments

Example:

```bash
./scripts/run_checksum_detection_campaign.sh
```

---

## Testing

The repository includes

- unit tests
- CLI integration tests
- release validation

Current status:

- ✅ cargo fmt
- ✅ cargo clippy
- ✅ cargo test

---

## Notes

The encoding and decoding pipeline follows the structure of CKKS to provide an application-level evaluation environment for fault propagation and resilience. It is intended for research and evaluation rather than as a production homomorphic encryption library.

---

## Citation

If you use this software in academic work, please cite:

```
(To be updated after publication.)
```

---

## License

MIT License

Copyright (c) 2026 <Authors>

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
