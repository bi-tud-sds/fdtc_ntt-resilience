#!/usr/bin/env bash
set -euo pipefail

cargo run --quiet -- ckks-demo \
  --n 16 --bits 24 --scale-bits 10 --ntt-impl radix2 \
  --fault --fault-op ntt --fault-site register-write \
  --fault-stage 0 --fault-slot 0 --fault-bit 8 \
  --fault-adjacent \
  --mitigation butterfly-check --mitigation-action detect-only --validate \
  | grep -E "Fault site|Fault adjacent|Fault observed|Checks performed|Check failures|Fault detected"
