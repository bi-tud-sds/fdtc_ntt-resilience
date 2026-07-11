# Mitigation Experiment Campaign

Copy `scripts/mitigation_campaign.py` into your project root.

## Smoke test

```bash
python3 scripts/mitigation_campaign.py \
  --output results/mitigation_smoke.csv \
  --n-values 16 \
  --bits 24 \
  --scale-bits 10 \
  --fault-ops ntt \
  --ntt-impls radix2 \
  --mitigations none,butterfly-check,stage-checksum \
  --checksum-modes sum,sum-index \
  --stages 0 \
  --slots 0 \
  --fault-bits 0 \
  --include-baseline \
  --validate
```

## Focused campaign

```bash
python3 scripts/mitigation_campaign.py \
  --release \
  --output results/mitigation_focused.csv \
  --n-values 2048,4096 \
  --bits 50 \
  --scale-bits 20 \
  --fault-ops ntt,intt,mul \
  --ntt-impls radix2,stockham \
  --mitigations none,butterfly-check,stage-checksum \
  --checksum-modes sum,sum-index \
  --stages sampled \
  --slots sampled \
  --fault-bits 0,8,16,23 \
  --include-baseline \
  --validate
```

## Full campaign

```bash
python3 scripts/mitigation_campaign.py \
  --release \
  --output results/mitigation_full.csv \
  --n-values 2048,4096,8192,16384 \
  --bits 50 \
  --scale-bits 20 \
  --fault-ops ntt,intt,mul \
  --ntt-impls radix2,dif-radix2,stockham,radix4,four-step \
  --mitigations none,butterfly-check,stage-checksum \
  --checksum-modes sum,sum-index \
  --stages all \
  --slots sampled \
  --fault-bits 0,1,2,4,8,12,16,20,23 \
  --include-baseline \
  --validate
```

## Main questions

1. Detection rate by mitigation.
2. False positive rate on baseline runs.
3. Residual RMS error after mitigation.
4. Runtime/memory overhead.
5. Coverage by operation, stage, bit position, slot, ring size, and NTT backend.
