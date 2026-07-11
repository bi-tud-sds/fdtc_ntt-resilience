#!/usr/bin/env bash
set -euo pipefail

mkdir -p results/large_ring_campaign
TS="$(date +%Y%m%d_%H%M%S)"
OUT="results/large_ring_campaign/detection_checksum_fixed_${TS}.csv"

echo "n,bits,mitigation,checksum_mode,action,fault_op,fault_site,stage,slot,bit,seed,fault_observed,golden_match,detected,corrected,max_abs_error,mean_abs_error,rms_error,relative_l2_error,snr_db,checks_performed,check_failures,stage_checks,stage_failures,s1_failures,s2_failures,recomputations,elapsed_ntt_ns,mitigation_time_ns,mod_adds,mod_subs,mod_muls,memory_reads,memory_writes" > "$OUT"

NS=(64 128)
BITS_LIST=(28)
MITIGATIONS=(stage-checksum)
CHECKSUM_MODES=(sum sum-index)
ACTIONS=(detect-only)
FAULT_OPS=(ntt)
FAULT_SITES=(mul-output add-output register-write)
SLOTS_PER_STAGE="${SLOTS_PER_STAGE:-32}"
SEEDS=(1 2 3)

sample_slots() {
  local n="$1"; local count="$2"; local seed="$3"
  python3 - "$n" "$count" "$seed" <<'PY'
import random, sys
n=int(sys.argv[1]); count=min(int(sys.argv[2]), n); seed=int(sys.argv[3])
rng=random.Random(seed)
print(" ".join(map(str, sorted(rng.sample(range(n), count)))))
PY
}

extract_metric_value() {
  local text="$1"; local label="$2"
  echo "$text" | awk -v label="$label" '
    index($0, label) {
      split($0, a, ":")
      gsub(/^[[:space:]]+|[[:space:]]+$/, "", a[2])
      print a[2]; exit
    }'
}

extract_last() {
  local text="$1"; local label="$2"
  echo "$text" | awk -v label="$label" 'index($0, label) {print $NF; exit}'
}

for n in "${NS[@]}"; do
  stages_count="$(python3 - <<PY
import math
print(int(math.log2($n)))
PY
)"
  STAGES=(0 $((stages_count / 2)) $((stages_count - 1)))

  for bits in "${BITS_LIST[@]}"; do
    bit_hi=$((bits - 2))
    BIT_POS=(0 8 24 "$bit_hi")

    for mitigation in "${MITIGATIONS[@]}"; do
      for checksum_mode in "${CHECKSUM_MODES[@]}"; do
        for action in "${ACTIONS[@]}"; do
          for fault_op in "${FAULT_OPS[@]}"; do
            for site in "${FAULT_SITES[@]}"; do
              for stage in "${STAGES[@]}"; do
                for seed in "${SEEDS[@]}"; do
                  slots="$(sample_slots "$n" "$SLOTS_PER_STAGE" "$((seed + stage * 1000 + n))")"

                  for slot in $slots; do
                    for bit in "${BIT_POS[@]}"; do
                      echo "[checksum-detection] n=$n bits=$bits checksum=$checksum_mode op=$fault_op site=$site stage=$stage slot=$slot bit=$bit seed=$seed"

                      set +e
                      result="$(cargo run --quiet -- ckks-demo \
                        --n "$n" --bits "$bits" --scale-bits 10 --ntt-impl radix2 \
                        --fault --fault-op "$fault_op" --fault-site "$site" \
                        --fault-stage "$stage" --fault-slot "$slot" --fault-bit "$bit" \
                        --mitigation "$mitigation" \
                        --checksum-mode "$checksum_mode" \
                        --mitigation-action "$action" --validate 2>&1)"
                      status=$?
                      set -e

                      if [[ "$status" -ne 0 ]]; then
                        echo "$n,$bits,$mitigation,$checksum_mode,$action,$fault_op,$site,$stage,$slot,$bit,$seed,ERROR,ERROR,ERROR,ERROR,NA,NA,NA,NA,NA,NA,NA,NA,NA,NA,NA,NA,NA,NA,NA,NA,NA,NA,NA" >> "$OUT"
                        continue
                      fi

                      fault_observed="$(extract_last "$result" "Fault observed:")"
                      golden_match="$(extract_last "$result" "Golden match:")"
                      detected="$(extract_last "$result" "Fault detected:")"
                      corrected="$(extract_last "$result" "Fault corrected:")"

                      max_abs_error="$(extract_metric_value "$result" "Max abs error")"
                      mean_abs_error="$(extract_metric_value "$result" "Mean abs error")"
                      rms_error="$(extract_metric_value "$result" "RMS error")"
                      relative_l2_error="$(extract_metric_value "$result" "Relative L2 error")"
                      snr_db="$(extract_metric_value "$result" "SNR dB")"

                      checks="$(extract_metric_value "$result" "Checks performed")"
                      failures="$(extract_metric_value "$result" "Check failures")"
                      stage_checks="$(extract_metric_value "$result" "Stage checks")"
                      stage_failures="$(extract_metric_value "$result" "Stage failures")"
                      s1_failures="$(extract_metric_value "$result" "S1 failures")"
                      s2_failures="$(extract_metric_value "$result" "S2 failures")"
                      recomputations="$(extract_metric_value "$result" "Recomputations")"

                      elapsed="$(extract_metric_value "$result" "Elapsed NTT time")"
                      mitigation_ns="$(extract_metric_value "$result" "Mitigation time ns")"
                      adds="$(extract_metric_value "$result" "Modular adds")"
                      subs="$(extract_metric_value "$result" "Modular subs")"
                      muls="$(extract_metric_value "$result" "Modular muls")"
                      reads="$(extract_metric_value "$result" "Memory reads")"
                      writes="$(extract_metric_value "$result" "Memory writes")"

                      echo "$n,$bits,$mitigation,$checksum_mode,$action,$fault_op,$site,$stage,$slot,$bit,$seed,$fault_observed,$golden_match,$detected,$corrected,$max_abs_error,$mean_abs_error,$rms_error,$relative_l2_error,$snr_db,$checks,$failures,$stage_checks,$stage_failures,$s1_failures,$s2_failures,$recomputations,$elapsed,$mitigation_ns,$adds,$subs,$muls,$reads,$writes" >> "$OUT"
                    done
                  done
                done
              done
            done
          done
        done
      done
    done
  done
done

echo "Wrote $OUT"
