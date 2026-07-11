#!/usr/bin/env bash
set -euo pipefail

mkdir -p results/small_complete_campaign
TS="$(date +%Y%m%d_%H%M%S)"
OUT="results/small_complete_campaign/small_complete_${TS}.csv"

NS=(${NS:-64 256 512})
BITS_LIST=(${BITS_LIST:-28 54})
FAULT_OPS=(${FAULT_OPS:-ntt intt})
FAULT_SITES=(${FAULT_SITES:-mul-output add-output sub-output butterfly-output register-write})
MITIGATIONS=(${MITIGATIONS:-butterfly-check stage-checksum})
ACTIONS=(${ACTIONS:-detect-only recompute})
SLOTS_PER_STAGE="${SLOTS_PER_STAGE:-4}"

TOTAL_RUNS=$(( ${#NS[@]} * ${#BITS_LIST[@]} * ${#FAULT_OPS[@]} * ${#FAULT_SITES[@]} * ${#MITIGATIONS[@]} * ${#ACTIONS[@]} * 3 * SLOTS_PER_STAGE * 4 ))
RUN_ID=0

echo "Expected total runs: $TOTAL_RUNS"
echo "Output CSV: $OUT"

echo "run_id,total_runs,n,bits,mitigation,action,fault_op,fault_site,stage,slot,bit,fault_observed,golden_match,detected,corrected,max_abs_error,mean_abs_error,rms_error,relative_l2_error,snr_db,checks_performed,check_failures,stage_checks,stage_failures,s1_failures,s2_failures,recomputations,elapsed_ntt_ns,mitigation_time_ns,mod_adds,mod_subs,mod_muls,memory_reads,memory_writes" > "$OUT"

extract_metric_value() {
  local text="$1"; local label="$2"
  echo "$text" | awk -v label="$label" '
    index($0, label) { split($0, a, ":"); gsub(/^[[:space:]]+|[[:space:]]+$/, "", a[2]); print a[2]; exit }'
}

extract_last() {
  local text="$1"; local label="$2"
  echo "$text" | awk -v label="$label" 'index($0, label) {print $NF; exit}'
}

sample_slots() {
  local n="$1"; local count="$2"
  python3 - "$n" "$count" <<'PY'
import sys
n = int(sys.argv[1]); count = int(sys.argv[2])
if count >= n:
    vals = list(range(n))
else:
    vals = sorted(set(round(i * (n - 1) / max(count - 1, 1)) for i in range(count)))
    x = 0
    while len(vals) < count:
        if x not in vals:
            vals.append(x)
        x += 1
    vals = sorted(vals[:count])
print(" ".join(map(str, vals)))
PY
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
    BIT_POS=(0 4 8 "$bit_hi")

    for mitigation in "${MITIGATIONS[@]}"; do
      for action in "${ACTIONS[@]}"; do
        for fault_op in "${FAULT_OPS[@]}"; do
          for site in "${FAULT_SITES[@]}"; do
            for stage in "${STAGES[@]}"; do
              slots="$(sample_slots "$n" "$SLOTS_PER_STAGE")"
              for slot in $slots; do
                for bit in "${BIT_POS[@]}"; do
                  RUN_ID=$((RUN_ID + 1))
                  echo "[$RUN_ID/$TOTAL_RUNS] n=$n bits=$bits mitigation=$mitigation action=$action op=$fault_op site=$site stage=$stage slot=$slot bit=$bit"

                  set +e
                  result="$(cargo run --quiet -- ckks-demo \
                    --n "$n" --bits "$bits" --scale-bits 10 --ntt-impl radix2 \
                    --fault --fault-op "$fault_op" --fault-site "$site" \
                    --fault-stage "$stage" --fault-slot "$slot" --fault-bit "$bit" \
                    --mitigation "$mitigation" --mitigation-action "$action" --validate 2>&1)"
                  status=$?
                  set -e

                  if [[ "$status" -ne 0 ]]; then
                    echo "[error] command failed; writing ERROR row" >&2
                    echo "$RUN_ID,$TOTAL_RUNS,$n,$bits,$mitigation,$action,$fault_op,$site,$stage,$slot,$bit,ERROR,ERROR,ERROR,ERROR,NA,NA,NA,NA,NA,NA,NA,NA,NA,NA,NA,NA,NA,NA,NA,NA,NA,NA,NA" >> "$OUT"
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

                  echo "$RUN_ID,$TOTAL_RUNS,$n,$bits,$mitigation,$action,$fault_op,$site,$stage,$slot,$bit,$fault_observed,$golden_match,$detected,$corrected,$max_abs_error,$mean_abs_error,$rms_error,$relative_l2_error,$snr_db,$checks,$failures,$stage_checks,$stage_failures,$s1_failures,$s2_failures,$recomputations,$elapsed,$mitigation_ns,$adds,$subs,$muls,$reads,$writes" >> "$OUT"
                done
              done
            done
          done
        done
      done
    done
  done
done

echo "Completed $RUN_ID / $TOTAL_RUNS runs"
echo "Wrote $OUT"
