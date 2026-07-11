#!/usr/bin/env python3
import argparse
from pathlib import Path
import pandas as pd

def numeric(df, cols):
    for c in cols:
        if c in df.columns:
            df[c] = pd.to_numeric(df[c], errors="coerce")

def main(path: Path):
    df = pd.read_csv(path)

    numeric(df, [
        "run_id", "total_runs", "n", "bits", "repeat",
        "elapsed_ntt_ns", "mitigation_time_ns",
        "checks_performed", "check_failures",
        "stage_checks", "stage_failures",
        "s1_failures", "s2_failures", "recomputations",
        "mod_adds", "mod_subs", "mod_muls",
        "memory_reads", "memory_writes",
    ])

    valid = df[pd.to_numeric(df["elapsed_ntt_ns"], errors="coerce").notna()].copy()
    print(f"File: {path}")
    print(f"Rows: {len(df)}")
    print(f"Valid rows: {len(valid)}")
    print(f"Error rows: {len(df)-len(valid)}")

    base = (
        valid[(valid["mitigation"] == "none") & (valid["action"] == "detect-only")]
        .groupby(["n", "bits"])["elapsed_ntt_ns"]
        .median()
        .rename("baseline_elapsed_ns")
        .reset_index()
    )

    summary = valid.groupby(["n", "bits", "mitigation", "action"]).agg(
        runs=("repeat", "size"),
        median_elapsed_ns=("elapsed_ntt_ns", "median"),
        p25_elapsed_ns=("elapsed_ntt_ns", lambda x: x.quantile(0.25)),
        p75_elapsed_ns=("elapsed_ntt_ns", lambda x: x.quantile(0.75)),
        mean_elapsed_ns=("elapsed_ntt_ns", "mean"),
        median_mitigation_ns=("mitigation_time_ns", "median"),
        median_checks=("checks_performed", "median"),
        median_check_failures=("check_failures", "median"),
        median_stage_checks=("stage_checks", "median"),
        median_stage_failures=("stage_failures", "median"),
        median_recomputations=("recomputations", "median"),
        median_mod_adds=("mod_adds", "median"),
        median_mod_subs=("mod_subs", "median"),
        median_mod_muls=("mod_muls", "median"),
        median_memory_reads=("memory_reads", "median"),
        median_memory_writes=("memory_writes", "median"),
    ).reset_index()

    summary = summary.merge(base, on=["n", "bits"], how="left")
    summary["overhead_pct_vs_none"] = 100.0 * (
        summary["median_elapsed_ns"] - summary["baseline_elapsed_ns"]
    ) / summary["baseline_elapsed_ns"]

    print("\n=== Overhead summary ===")
    print(summary.to_string(index=False))

    # A compact table useful for the paper.
    paper = summary[["n", "bits", "mitigation", "action", "median_elapsed_ns", "baseline_elapsed_ns", "overhead_pct_vs_none", "median_mitigation_ns", "median_checks", "median_stage_checks"]].copy()

    prefix = path.with_suffix("")
    summary.to_csv(str(prefix) + ".summary.csv", index=False)
    paper.to_csv(str(prefix) + ".paper_table.csv", index=False)

    print("\nWrote:")
    print(str(prefix) + ".summary.csv")
    print(str(prefix) + ".paper_table.csv")

if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("csv", type=Path)
    args = ap.parse_args()
    main(args.csv)
