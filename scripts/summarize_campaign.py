#!/usr/bin/env python3
import argparse
from pathlib import Path
import pandas as pd

def numeric(df, cols):
    for c in cols:
        if c in df.columns:
            df[c] = pd.to_numeric(df[c], errors="coerce")

def bool_pass(s):
    return s.astype(str).str.upper().eq("PASS")

def bool_yes(s):
    return s.astype(str).str.lower().eq("yes")

def summarize_campaign(path):
    df = pd.read_csv(path)
    numeric(df, ["n","bits","stage","slot","bit","seed","rms_error","max_abs_error","check_failures","checks_performed","elapsed_ntt_ns","mitigation_time_ns","recomputations"])
    df["observed_bool"] = bool_pass(df["fault_observed"])
    df["detected_bool"] = bool_yes(df["detected"])
    df["corrected_bool"] = bool_yes(df["corrected"])

    group_cols = ["n","bits","mitigation","action"]
    summary = df.groupby(group_cols).agg(
        injections=("n","size"),
        observed_rate=("observed_bool","mean"),
        detection_rate=("detected_bool","mean"),
        correction_rate=("corrected_bool","mean"),
        median_rms=("rms_error","median"),
        mean_check_failures=("check_failures","mean"),
        median_elapsed_ns=("elapsed_ntt_ns","median"),
        median_mitigation_ns=("mitigation_time_ns","median"),
    ).reset_index()

    cond = []
    for keys, g in df.groupby(group_cols):
        obs = g[g["observed_bool"]]
        cond_rate = obs["detected_bool"].mean() if len(obs) else float("nan")
        cond.append((*keys, cond_rate))
    cond = pd.DataFrame(cond, columns=group_cols+["conditional_detection_rate"])
    summary = summary.merge(cond, on=group_cols)

    by_site = df.groupby(group_cols+["fault_site"]).agg(
        injections=("fault_site","size"),
        observed_rate=("observed_bool","mean"),
        detection_rate=("detected_bool","mean"),
        correction_rate=("corrected_bool","mean"),
        mean_check_failures=("check_failures","mean"),
        median_rms=("rms_error","median"),
    ).reset_index()

    by_stage = df.groupby(group_cols+["stage"]).agg(
        injections=("stage","size"),
        observed_rate=("observed_bool","mean"),
        detection_rate=("detected_bool","mean"),
        correction_rate=("corrected_bool","mean"),
        mean_check_failures=("check_failures","mean"),
        median_rms=("rms_error","median"),
    ).reset_index()

    print("\nOverall:")
    print(summary.to_string(index=False))
    print("\nBy site:")
    print(by_site.to_string(index=False))
    print("\nBy stage:")
    print(by_stage.to_string(index=False))

    summary.to_csv(path.with_suffix(".summary.csv"), index=False)
    by_site.to_csv(path.with_suffix(".by_site.csv"), index=False)
    by_stage.to_csv(path.with_suffix(".by_stage.csv"), index=False)
    print("\nWrote summary CSV files next to input.")

def summarize_overhead(path):
    df = pd.read_csv(path)
    numeric(df, ["n","bits","repeat","elapsed_ntt_ns","mitigation_time_ns","checks_performed","check_failures","stage_checks","stage_failures","recomputations","mod_adds","mod_subs","mod_muls","memory_reads","memory_writes"])
    base = df[(df.mitigation=="none") & (df.action=="detect-only")].groupby(["n","bits"])["elapsed_ntt_ns"].median().rename("baseline_elapsed_ns").reset_index()
    summary = df.groupby(["n","bits","mitigation","action"]).agg(
        runs=("repeat","size"),
        median_elapsed_ns=("elapsed_ntt_ns","median"),
        p25_elapsed_ns=("elapsed_ntt_ns", lambda x: x.quantile(0.25)),
        p75_elapsed_ns=("elapsed_ntt_ns", lambda x: x.quantile(0.75)),
        median_mitigation_ns=("mitigation_time_ns","median"),
        median_checks=("checks_performed","median"),
        median_stage_checks=("stage_checks","median"),
        median_recomputations=("recomputations","median"),
    ).reset_index()
    summary = summary.merge(base, on=["n","bits"], how="left")
    summary["overhead_pct_vs_none"] = 100*(summary["median_elapsed_ns"]-summary["baseline_elapsed_ns"])/summary["baseline_elapsed_ns"]
    print(summary.to_string(index=False))
    summary.to_csv(path.with_suffix(".summary.csv"), index=False)
    print("\nWrote", path.with_suffix(".summary.csv"))

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("csv", type=Path)
    ap.add_argument("--kind", choices=["campaign","overhead"], default="campaign")
    args = ap.parse_args()
    if args.kind == "campaign":
        summarize_campaign(args.csv)
    else:
        summarize_overhead(args.csv)

if __name__ == "__main__":
    main()
