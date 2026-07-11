#!/usr/bin/env python3
import argparse
from pathlib import Path
import pandas as pd
import matplotlib.pyplot as plt

def main():
    ap = argparse.ArgumentParser(description="Analyze CKKS decoded-domain fault sweep CSV.")
    ap.add_argument("--input", required=True)
    ap.add_argument("--output-dir", default="ckks_reports")
    args = ap.parse_args()

    out = Path(args.output_dir)
    out.mkdir(parents=True, exist_ok=True)

    df = pd.read_csv(args.input)

    if "rms_error" in df.columns:
        g = df.groupby(["fault_op", "n"])["rms_error"].mean().reset_index()
        plt.figure(figsize=(6,4))
        for op in sorted(g["fault_op"].unique()):
            d = g[g["fault_op"] == op]
            plt.plot(d["n"], d["rms_error"], marker="o", label=op)
        plt.xscale("log", base=2)
        plt.xlabel("Ring size N")
        plt.ylabel("Mean decoded RMS error")
        plt.legend()
        plt.tight_layout()
        plt.savefig(out / "decoded_rms_vs_n.png")
        plt.close()

    if {"fault_op", "relative_l2_error"}.issubset(df.columns):
        ops = sorted(df["fault_op"].dropna().unique())
        data = [df[df["fault_op"] == op]["relative_l2_error"].dropna().values for op in ops]
        plt.figure(figsize=(6,4))
        plt.boxplot(data)
        plt.xticks(range(1, len(ops)+1), ops)
        plt.ylabel("Decoded relative L2 error")
        plt.tight_layout()
        plt.savefig(out / "decoded_relative_l2_by_op.png")
        plt.close()

    if {"fault_stage", "fault_op", "rms_error"}.issubset(df.columns):
        for op in sorted(df["fault_op"].dropna().unique()):
            d = df[df["fault_op"] == op]
            g = d.groupby("fault_stage")["rms_error"].mean()
            plt.figure(figsize=(6,4))
            plt.plot(g.index, g.values, marker="o")
            plt.xlabel("Fault stage")
            plt.ylabel("Mean decoded RMS error")
            plt.title(op)
            plt.tight_layout()
            plt.savefig(out / f"decoded_rms_vs_stage_{op}.png")
            plt.close()

    df.describe(include="all").to_csv(out / "summary.csv")
    print(f"Wrote analysis outputs to {out}")

if __name__ == "__main__":
    main()
