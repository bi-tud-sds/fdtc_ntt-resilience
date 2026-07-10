# Reproducibility Guide

This document describes how to reproduce the experimental results presented in the paper

> **Directions for Software Resilience for Number Theoretic Transforms**

The repository contains a configurable framework for evaluating software resilience techniques for radix-2 Number Theoretic Transforms (NTTs), including transient fault injection, butterfly consistency invariants, stage-level checksum verification, and selective recomputation.

---

# Requirements

The artifact has been tested with

- Rust (stable)
- Cargo
- Python 3.11 or later

Clone the repository and build the project:

```bash
cargo build --release
```

Verify the implementation:

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all
```

---

# Quick Validation

Execute the CKKS-inspired demonstration without faults:

```bash
cargo run --release -- ckks-demo \
    --n 2048 \
    --bits 54 \
    --validate
```

The execution should terminate with

```
Execution valid: PASS
```

---

# Repository Organization

```
src/            Rust implementation
tests/          Unit and CLI integration tests
scripts/        Experiment automation
analysis/       Analysis utilities
results/        Experimental datasets
docs/           Additional documentation
```

---

# Experimental Campaigns

The paper reports four independent experimental campaigns.

## 1. Butterfly Detection Campaign

Evaluates application-level fault observability and detection coverage of butterfly consistency invariants.

Run:

```bash
./scripts/run_practical_fault_model_campaign.sh
```

Output:

```
results/paper/butterfly_detection.csv
```

Supports:

- Detection coverage
- Sensitivity analysis
- Discussion of application-visible faults

---

## 2. Checksum Detection Campaign

Evaluates stage-level checksum verification using both Sum and Sum+Index invariants.

Run:

```bash
./scripts/run_checksum_detection_campaign.sh
```

Output:

```
results/paper/checksum_detection.csv
```

Supports:

- Comparison between butterfly invariants and stage checksums
- Detection coverage results
- Checksum evaluation tables

---

## 3. Recovery Campaign

Evaluates selective recomputation using butterfly invariants under the transient fault model.

Run:

```bash
python3 scripts/mitigation_campaign.py
```

Output:

```
results/paper/recovery_campaign.csv
```

Supports:

- Recovery evaluation
- Recomputation statistics
- Recovery overhead

---

## 4. Runtime Overhead Campaign

Measures execution overhead introduced by the resilience mechanisms.

Run:

```bash
./scripts/run_overhead_campaign.sh
```

Output:

```
results/paper/runtime_overhead.csv
```

Supports:

- Runtime overhead table
- Performance evaluation

---

# Paper-to-Artifact Mapping

| Paper Component           | Script                                  | Dataset                   |
|---------------------------|-----------------------------------------|---------------------------|
| Butterfly detection       | `run_practical_fault_model_campaign.sh` | `butterfly_detection.csv` |
| Stage checksum evaluation | `run_checksum_detection_campaign.sh`    | `checksum_detection.csv`  |
| Recovery evaluation       | `mitigation_campaign.py`                | `recovery_campaign.csv`   |
| Runtime overhead          | `run_overhead_campaign.sh`              | `runtime_overhead.csv`    |

---

# Example Fault Injection

Inject a transient arithmetic fault into the forward radix-2 NTT:

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
    --mitigation stage-checksum \
    --checksum-mode sum-index \
    --mitigation-action detect-only \
    --validate
```

Expected output:

```
Fault detected:    yes
Execution valid:   PASS
Fault observed:    PASS
```

---

# Experimental Scope

The complete evaluation reported in the paper comprises more than **33,000 experimental runs**, including

- butterfly fault detection,
- stage checksum evaluation,
- selective recovery, and
- runtime overhead measurements.

The datasets included in the repository correspond to the experimental results reported in the paper.

---

# Notes

The encoding and decoding pipeline implemented in this repository follows the structure of the CKKS approximate homomorphic encryption scheme to provide an application-level environment for evaluating transient fault propagation. It is intended as a research and evaluation framework rather than a production homomorphic encryption library.
