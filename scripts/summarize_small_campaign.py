#!/usr/bin/env python3
import argparse
from pathlib import Path
import pandas as pd

def numeric(df, cols):
    for c in cols:
        if c in df.columns:
            df[c] = pd.to_numeric(df[c], errors="coerce")

def pass_bool(s): return s.astype(str).str.upper().eq("PASS")
def yes_bool(s): return s.astype(str).str.lower().eq("yes")

def main(path):
    df = pd.read_csv(path)
    numeric(df, ["run_id","total_runs","n","bits","stage","slot","bit","rms_error","max_abs_error",
                 "checks_performed","check_failures","stage_checks","stage_failures","recomputations",
                 "elapsed_ntt_ns","mitigation_time_ns"])
    valid = df[~df["fault_observed"].astype(str).eq("ERROR")].copy()
    valid["observed_bool"] = pass_bool(valid["fault_observed"])
    valid["detected_bool"] = yes_bool(valid["detected"])
    valid["corrected_bool"] = yes_bool(valid["corrected"])

    print(f"Rows: {len(df)}  Valid: {len(valid)}  Errors: {len(df)-len(valid)}")

    group_cols = ["n","bits","mitigation","action"]
    overall = valid.groupby(group_cols).agg(
        injections=("n","size"),
        observed_rate=("observed_bool","mean"),
        detection_rate_all=("detected_bool","mean"),
        correction_rate_all=("corrected_bool","mean"),
        median_rms=("rms_error","median"),
        mean_check_failures=("check_failures","mean"),
        median_recomputations=("recomputations","median"),
        median_elapsed_ns=("elapsed_ntt_ns","median"),
        median_mitigation_ns=("mitigation_time_ns","median"),
    ).reset_index()

    cond_rows = []
    for keys, g in valid.groupby(group_cols):
        obs = g[g["observed_bool"]]
        det = g[g["detected_bool"]]
        cond_rows.append((*keys,
            obs["detected_bool"].mean() if len(obs) else float("nan"),
            det["corrected_bool"].mean() if len(det) else float("nan"),
            obs["corrected_bool"].mean() if len(obs) else float("nan")))
    cond = pd.DataFrame(cond_rows, columns=group_cols+["detection_rate_observed","correction_given_detected","correction_given_observed"])
    overall = overall.merge(cond, on=group_cols)

    outputs = {"summary": overall}
    for name, cols in {
        "by_site": group_cols + ["fault_site"],
        "by_stage": group_cols + ["stage"],
        "by_bit": group_cols + ["bit"],
        "by_op": group_cols + ["fault_op"],
    }.items():
        outputs[name] = valid.groupby(cols).agg(
            injections=("n","size"),
            observed_rate=("observed_bool","mean"),
            detection_rate_all=("detected_bool","mean"),
            correction_rate_all=("corrected_bool","mean"),
            mean_check_failures=("check_failures","mean"),
            median_rms=("rms_error","median"),
        ).reset_index()

    for name, table in outputs.items():
        print(f"\n=== {name} ===")
        print(table.to_string(index=False))
        table.to_csv(str(path.with_suffix("")) + f".{name}.csv", index=False)

    print("\nWrote summary CSV files next to input.")

if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("csv", type=Path)
    args = ap.parse_args()
    main(args.csv)
