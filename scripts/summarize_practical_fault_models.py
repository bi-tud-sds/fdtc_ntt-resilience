#!/usr/bin/env python3
import argparse
from pathlib import Path
import pandas as pd

def main(path):
    df=pd.read_csv(path)
    valid=df[~df.fault_observed.astype(str).eq("ERROR")].copy()
    for c in ["n","bits","rms_error","check_failures","recomputations","elapsed_ntt_ns","mitigation_time_ns"]:
        if c in valid:
            valid[c]=pd.to_numeric(valid[c], errors="coerce")
    valid["observed"]=valid.fault_observed.astype(str).str.upper().eq("PASS")
    valid["detected_bool"]=valid.detected.astype(str).str.lower().eq("yes")
    valid["corrected_bool"]=valid.corrected.astype(str).str.lower().eq("yes")
    gcols=["mode","n","bits","mitigation","action"]
    summary=valid.groupby(gcols).agg(
        injections=("mode","size"),
        observability=("observed","mean"),
        detection_all=("detected_bool","mean"),
        correction_all=("corrected_bool","mean"),
        median_rms=("rms_error","median"),
        mean_check_failures=("check_failures","mean"),
        median_recomputations=("recomputations","median"),
        median_elapsed_ns=("elapsed_ntt_ns","median")
    ).reset_index()
    rows=[]
    for keys,g in valid.groupby(gcols):
        obs=g[g.observed]; det=g[g.detected_bool]
        rows.append((*keys,
            obs.detected_bool.mean() if len(obs) else float("nan"),
            det.corrected_bool.mean() if len(det) else float("nan"),
            obs.corrected_bool.mean() if len(obs) else float("nan")))
    cond=pd.DataFrame(rows, columns=gcols+["detection_given_observed","correction_given_detected","correction_given_observed"])
    summary=summary.merge(cond,on=gcols)
    by_site=valid.groupby(gcols+["fault_site"]).agg(
        injections=("mode","size"),
        observability=("observed","mean"),
        detection_all=("detected_bool","mean"),
        correction_all=("corrected_bool","mean"),
        mean_check_failures=("check_failures","mean"),
        median_rms=("rms_error","median")
    ).reset_index()
    pre=str(path.with_suffix(""))
    summary.to_csv(pre+".summary.csv", index=False)
    by_site.to_csv(pre+".by_site.csv", index=False)
    print(summary.to_string(index=False))
    print("Wrote", pre+".summary.csv", pre+".by_site.csv")

if __name__=="__main__":
    ap=argparse.ArgumentParser()
    ap.add_argument("csv", type=Path)
    main(ap.parse_args().csv)
