#!/usr/bin/env bash
set -euo pipefail
mkdir -p results/practical_fault_models
TS="$(date +%Y%m%d_%H%M%S)"
OUT="results/practical_fault_models/practical_fault_models_${TS}.csv"

#NS=(${NS:-1024 2048})
NS=(${NS:-512 2048 8192 32768})
BITS_LIST=(${BITS_LIST:-28 54})
FAULT_SITES=(${FAULT_SITES:-register-write butterfly-output add-output mul-output})
MITIGATIONS=(${MITIGATIONS:-butterfly-check stage-checksum})
ACTIONS=(${ACTIONS:-detect-only recompute})
MODES=(${MODES:-single adjacent two-single two-adjacent})
SLOTS_PER_STAGE="${SLOTS_PER_STAGE:-2}"

echo "mode,run_id,total_runs,n,bits,mitigation,action,fault_op,fault_site,stage,slot,bit,fault_adjacent,fault2_enabled,fault2_site,fault2_stage,fault2_slot,fault2_bit,fault2_adjacent,fault_observed,golden_match,detected,corrected,max_abs_error,mean_abs_error,rms_error,relative_l2_error,snr_db,checks_performed,check_failures,stage_checks,stage_failures,s1_failures,s2_failures,recomputations,elapsed_ntt_ns,mitigation_time_ns,mod_adds,mod_subs,mod_muls,memory_reads,memory_writes" > "$OUT"

TOTAL_RUNS=0
for n in "${NS[@]}"; do for bits in "${BITS_LIST[@]}"; do for mode in "${MODES[@]}"; do for mitigation in "${MITIGATIONS[@]}"; do for action in "${ACTIONS[@]}"; do for site in "${FAULT_SITES[@]}"; do
  TOTAL_RUNS=$((TOTAL_RUNS + 3 * SLOTS_PER_STAGE * 3))
done; done; done; done; done; done
RUN_ID=0
echo "Expected total runs: $TOTAL_RUNS"
echo "Output CSV: $OUT"

extract_metric_value(){ local text="$1"; local label="$2"; echo "$text" | awk -v label="$label" 'index($0,label){split($0,a,":"); gsub(/^[[:space:]]+|[[:space:]]+$/,"",a[2]); print a[2]; exit}'; }
extract_last(){ local text="$1"; local label="$2"; echo "$text" | awk -v label="$label" 'index($0,label){print $NF; exit}'; }
sample_slots(){ python3 - "$1" "$2" <<'PY'
import sys
n=int(sys.argv[1]); c=int(sys.argv[2])
vals=sorted(set(round(i*(n-1)/max(c-1,1)) for i in range(c))) if c<n else list(range(n))
x=0
while len(vals)<c:
    if x not in vals: vals.append(x)
    x+=1
print(" ".join(map(str, sorted(vals[:c]))))
PY
}
second_site_for(){ case "$1" in register-write) echo butterfly-output;; butterfly-output) echo register-write;; add-output) echo mul-output;; mul-output) echo add-output;; *) echo register-write;; esac; }

for n in "${NS[@]}"; do
  stages_count="$(python3 - <<PY
import math
print(int(math.log2($n)))
PY
)"
  STAGES=(0 $((stages_count/2)) $((stages_count-1)))
  for bits in "${BITS_LIST[@]}"; do
    BIT_POS=(0 8 $((bits-3)))
    for mode in "${MODES[@]}"; do
      for mitigation in "${MITIGATIONS[@]}"; do
        for action in "${ACTIONS[@]}"; do
          for site in "${FAULT_SITES[@]}"; do
            site2="$(second_site_for "$site")"
            for stage in "${STAGES[@]}"; do
              slots="$(sample_slots "$n" "$SLOTS_PER_STAGE")"
              for slot in $slots; do
                slot2=$(((slot + n/4) % n))
                stage2=$(((stage + 1) % stages_count))
                for bit in "${BIT_POS[@]}"; do
                  bit2=$(( bit == 0 ? 4 : bit - 1 ))
                  RUN_ID=$((RUN_ID+1))
                  args=(cargo run --quiet -- ckks-demo --n "$n" --bits "$bits" --scale-bits 10 --ntt-impl radix2 --fault --fault-op ntt --fault-site "$site" --fault-stage "$stage" --fault-slot "$slot" --fault-bit "$bit" --mitigation "$mitigation" --mitigation-action "$action" --validate)
                  fault_adjacent=false; fault2_enabled=false; fault2_adjacent=false
                  if [[ "$mode" == "adjacent" || "$mode" == "two-adjacent" ]]; then args+=(--fault-adjacent); fault_adjacent=true; fi
                  if [[ "$mode" == "two-single" || "$mode" == "two-adjacent" ]]; then args+=(--fault2 --fault2-site "$site2" --fault2-stage "$stage2" --fault2-slot "$slot2" --fault2-bit "$bit2"); fault2_enabled=true; fi
                  if [[ "$mode" == "two-adjacent" ]]; then args+=(--fault2-adjacent); fault2_adjacent=true; fi
                  echo "[$RUN_ID/$TOTAL_RUNS] mode=$mode n=$n bits=$bits mitigation=$mitigation action=$action site=$site stage=$stage slot=$slot bit=$bit site2=$site2 stage2=$stage2 slot2=$slot2 bit2=$bit2"
                  set +e
                  result="$("${args[@]}" 2>&1)"
                  status=$?
                  set -e
                  if [[ "$status" -ne 0 ]]; then
                    echo "$mode,$RUN_ID,$TOTAL_RUNS,$n,$bits,$mitigation,$action,ntt,$site,$stage,$slot,$bit,$fault_adjacent,$fault2_enabled,$site2,$stage2,$slot2,$bit2,$fault2_adjacent,ERROR,ERROR,ERROR,ERROR,NA,NA,NA,NA,NA,NA,NA,NA,NA,NA,NA,NA,NA,NA,NA,NA,NA,NA,NA" >> "$OUT"
                    continue
                  fi
                  row="$mode,$RUN_ID,$TOTAL_RUNS,$n,$bits,$mitigation,$action,ntt,$site,$stage,$slot,$bit,$fault_adjacent,$fault2_enabled,$site2,$stage2,$slot2,$bit2,$fault2_adjacent"
                  for label in "Fault observed:" "Golden match:" "Fault detected:" "Fault corrected:"; do row="$row,$(extract_last "$result" "$label")"; done
                  for label in "Max abs error" "Mean abs error" "RMS error" "Relative L2 error" "SNR dB" "Checks performed" "Check failures" "Stage checks" "Stage failures" "S1 failures" "S2 failures" "Recomputations" "Elapsed NTT time" "Mitigation time ns" "Modular adds" "Modular subs" "Modular muls" "Memory reads" "Memory writes"; do row="$row,$(extract_metric_value "$result" "$label")"; done
                  echo "$row" >> "$OUT"
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
