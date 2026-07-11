#!/usr/bin/env bash
set -euo pipefail

mkdir -p results/large_ring_campaign
TS="$(date +%Y%m%d_%H%M%S)"
OUT="results/large_ring_campaign/overhead_${TS}.csv"

NS=(2048 4096 8192 16384 32768)
BITS_LIST=(28 54)
MITIGATIONS=(none butterfly-check stage-checksum)
ACTIONS=(detect-only recompute)
REPEATS="${REPEATS:-30}"

echo "n,bits,mitigation,action,repeat,elapsed_ntt_ns,mitigation_time_ns,checks_performed,check_failures,stage_checks,stage_failures,recomputations,mod_adds,mod_subs,mod_muls,memory_reads,memory_writes" > "$OUT"

extract_metric_value() {
  local text="$1"; local label="$2"
  echo "$text" | awk -v label="$label" '
    index($0, label) {
      split($0, a, ":")
      gsub(/^[[:space:]]+|[[:space:]]+$/, "", a[2])
      print a[2]; exit
    }'
}

for n in "${NS[@]}"; do
  for bits in "${BITS_LIST[@]}"; do
    for mitigation in "${MITIGATIONS[@]}"; do
      for action in "${ACTIONS[@]}"; do
        [[ "$mitigation" == "none" && "$action" == "recompute" ]] && continue
        for rep in $(seq 1 "$REPEATS"); do
          echo "[overhead] n=$n bits=$bits mitigation=$mitigation action=$action rep=$rep"

          result="$(cargo run --quiet -- ckks-demo             --n "$n" --bits "$bits" --scale-bits 10 --ntt-impl radix2             --mitigation "$mitigation" --mitigation-action "$action" --validate 2>&1)"

          elapsed="$(extract_metric_value "$result" "Elapsed NTT time")"
          mitigation_ns="$(extract_metric_value "$result" "Mitigation time ns")"
          checks="$(extract_metric_value "$result" "Checks performed")"
          failures="$(extract_metric_value "$result" "Check failures")"
          stage_checks="$(extract_metric_value "$result" "Stage checks")"
          stage_failures="$(extract_metric_value "$result" "Stage failures")"
          recomputations="$(extract_metric_value "$result" "Recomputations")"
          adds="$(extract_metric_value "$result" "Modular adds")"
          subs="$(extract_metric_value "$result" "Modular subs")"
          muls="$(extract_metric_value "$result" "Modular muls")"
          reads="$(extract_metric_value "$result" "Memory reads")"
          writes="$(extract_metric_value "$result" "Memory writes")"

          echo "$n,$bits,$mitigation,$action,$rep,${elapsed:-NA},${mitigation_ns:-0},${checks:-0},${failures:-0},${stage_checks:-0},${stage_failures:-0},${recomputations:-0},${adds:-NA},${subs:-NA},${muls:-NA},${reads:-NA},${writes:-NA}" >> "$OUT"
        done
      done
    done
  done
done

echo "Wrote $OUT"
