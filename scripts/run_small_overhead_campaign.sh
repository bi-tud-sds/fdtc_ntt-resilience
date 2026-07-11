#!/usr/bin/env bash
set -euo pipefail

mkdir -p results/small_overhead_campaign
TS="$(date +%Y%m%d_%H%M%S)"
OUT="results/small_overhead_campaign/small_overhead_${TS}.csv"

# Matched to the small complete campaign, but fault-free.
NS=(${NS:-64 256 512})
BITS_LIST=(${BITS_LIST:-28 54})
MITIGATIONS=(${MITIGATIONS:-none butterfly-check stage-checksum})
ACTIONS=(${ACTIONS:-detect-only recompute})
REPEATS="${REPEATS:-30}"

# Skip none+recompute because it is equivalent to none+detect-only.
TOTAL_CONFIGS=0
for n in "${NS[@]}"; do
  for bits in "${BITS_LIST[@]}"; do
    for mitigation in "${MITIGATIONS[@]}"; do
      for action in "${ACTIONS[@]}"; do
        if [[ "$mitigation" == "none" && "$action" == "recompute" ]]; then
          continue
        fi
        TOTAL_CONFIGS=$((TOTAL_CONFIGS + 1))
      done
    done
  done
done

TOTAL_RUNS=$((TOTAL_CONFIGS * REPEATS))
RUN_ID=0

echo "Expected total runs: $TOTAL_RUNS"
echo "Output CSV: $OUT"

echo "run_id,total_runs,n,bits,mitigation,action,repeat,elapsed_ntt_ns,mitigation_time_ns,checks_performed,check_failures,stage_checks,stage_failures,s1_failures,s2_failures,recomputations,mod_adds,mod_subs,mod_muls,memory_reads,memory_writes" > "$OUT"

extract_metric_value() {
  local text="$1"; local label="$2"
  echo "$text" | awk -v label="$label" '
    index($0, label) {
      split($0, a, ":")
      gsub(/^[[:space:]]+|[[:space:]]+$/, "", a[2])
      print a[2]
      exit
    }'
}

for n in "${NS[@]}"; do
  for bits in "${BITS_LIST[@]}"; do
    for mitigation in "${MITIGATIONS[@]}"; do
      for action in "${ACTIONS[@]}"; do
        if [[ "$mitigation" == "none" && "$action" == "recompute" ]]; then
          continue
        fi

        for rep in $(seq 1 "$REPEATS"); do
          RUN_ID=$((RUN_ID + 1))
          echo "[$RUN_ID/$TOTAL_RUNS] n=$n bits=$bits mitigation=$mitigation action=$action repeat=$rep"

          set +e
          result="$(cargo run --quiet -- ckks-demo \
            --n "$n" \
            --bits "$bits" \
            --scale-bits 10 \
            --ntt-impl radix2 \
            --mitigation "$mitigation" \
            --mitigation-action "$action" \
            --validate 2>&1)"
          status=$?
          set -e

          if [[ "$status" -ne 0 ]]; then
            echo "[error] command failed; writing ERROR row" >&2
            echo "$RUN_ID,$TOTAL_RUNS,$n,$bits,$mitigation,$action,$rep,ERROR,ERROR,ERROR,ERROR,ERROR,ERROR,ERROR,ERROR,ERROR,ERROR,ERROR,ERROR,ERROR,ERROR" >> "$OUT"
            continue
          fi

          elapsed="$(extract_metric_value "$result" "Elapsed NTT time")"
          mitigation_ns="$(extract_metric_value "$result" "Mitigation time ns")"
          checks="$(extract_metric_value "$result" "Checks performed")"
          failures="$(extract_metric_value "$result" "Check failures")"
          stage_checks="$(extract_metric_value "$result" "Stage checks")"
          stage_failures="$(extract_metric_value "$result" "Stage failures")"
          s1_failures="$(extract_metric_value "$result" "S1 failures")"
          s2_failures="$(extract_metric_value "$result" "S2 failures")"
          recomputations="$(extract_metric_value "$result" "Recomputations")"
          adds="$(extract_metric_value "$result" "Modular adds")"
          subs="$(extract_metric_value "$result" "Modular subs")"
          muls="$(extract_metric_value "$result" "Modular muls")"
          reads="$(extract_metric_value "$result" "Memory reads")"
          writes="$(extract_metric_value "$result" "Memory writes")"

          echo "$RUN_ID,$TOTAL_RUNS,$n,$bits,$mitigation,$action,$rep,${elapsed:-NA},${mitigation_ns:-0},${checks:-0},${failures:-0},${stage_checks:-0},${stage_failures:-0},${s1_failures:-0},${s2_failures:-0},${recomputations:-0},${adds:-NA},${subs:-NA},${muls:-NA},${reads:-NA},${writes:-NA}" >> "$OUT"
        done
      done
    done
  done
done

echo "Completed $RUN_ID / $TOTAL_RUNS runs"
echo "Wrote $OUT"
