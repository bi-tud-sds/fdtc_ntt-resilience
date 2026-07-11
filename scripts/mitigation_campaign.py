#!/usr/bin/env python3
import argparse, csv, math, re, subprocess
from pathlib import Path

def csvs(s): return [x.strip() for x in s.split(",") if x.strip()]
def ints(s): return [int(x) for x in csvs(s)]

def stages(spec,n,op):
    if op=="mul": return [0]
    if spec=="all": return list(range(int(math.log2(n))))
    if spec=="sampled":
        m=int(math.log2(n)); return sorted({0, max(0,m//2), max(0,m-1)})
    return ints(spec)

def slots(spec,n):
    if spec=="all": return list(range(n))
    if spec=="sampled": return sorted({x for x in [0,1,n//4,n//2,3*n//4,n-1] if 0<=x<n})
    return ints(spec)

def parse(out,err,code):
    txt=out+"\n"+err
    pats={
      "execution_valid":r"Execution valid:\s*([A-Za-z/]+)",
      "golden_match":r"Golden match:\s*([A-Za-z/]+)",
      "fault_observed":r"Fault observed:\s*([A-Za-z/]+)",
      "rms_error":r"RMS error:\s*([-+0-9.eE]+)",
      "relative_l2_error":r"Relative L2 error:\s*([-+0-9.eE]+)",
      "max_abs_error":r"Max abs error:\s*([-+0-9.eE]+)",
      "mean_abs_error":r"Mean abs error:\s*([-+0-9.eE]+)",
      "snr_db":r"SNR.*?:\s*([-+0-9.eE]+)",
      "elapsed_ns":r"Elapsed.*?ns:\s*([0-9]+)",
      "scratch_bytes":r"Scratch bytes:\s*([0-9]+)",
      "checks_performed":r"Checks performed:\s*([0-9]+)",
      "check_failures":r"Check failures:\s*([0-9]+)",
      "s1_failures":r"S1 failures:\s*([0-9]+)",
      "s2_failures":r"S2 failures:\s*([0-9]+)",
      "recomputations":r"Recomputations:\s*([0-9]+)",
      "mitigation_elapsed_ns":r"Mitigation elapsed.*?ns:\s*([0-9]+)",
    }
    row={"returncode":str(code)}
    for k,p in pats.items():
        m=re.search(p,txt,re.I)
        if m: row[k]=m.group(1)
    if code!=0: row["stderr_tail"]=err[-1000:]
    return row

def cmd(args,n,impl,mit,mode,baseline,op="ntt",operand="a",stage=0,slot=0,bit=0):
    c=["cargo","run"]
    if args.release: c.append("--release")
    c+=["--","ckks-demo","--n",str(n),"--bits",str(args.bits),
        "--scale-bits",str(args.scale_bits),"--ntt-impl",impl,
        "--mitigation",mit,"--mitigation-action",args.mitigation_action,
        "--max-retries",str(args.max_retries)]
    if mit=="stage-checksum": c+=["--checksum-mode",mode]
    if args.validate: c.append("--validate")
    if not baseline:
        c+=["--fault","--fault-op",op,"--fault-operand",operand,
            "--fault-stage",str(stage),"--fault-slot",str(slot),"--fault-bit",str(bit)]
    return c

def run(c,dry):
    if dry:
        print(" ".join(c)); return {"returncode":"0"}
    p=subprocess.run(c,text=True,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
    return parse(p.stdout,p.stderr,p.returncode)

def main():
    ap=argparse.ArgumentParser()
    ap.add_argument("--output",required=True)
    ap.add_argument("--n-values",default="16")
    ap.add_argument("--bits",type=int,default=24)
    ap.add_argument("--scale-bits",type=int,default=10)
    ap.add_argument("--fault-ops",default="ntt,intt,mul")
    ap.add_argument("--operands",default="a,b")
    ap.add_argument("--ntt-impls",default="radix2")
    ap.add_argument("--mitigations",default="none,butterfly-check,stage-checksum")
    ap.add_argument("--checksum-modes",default="sum,sum-index")
    ap.add_argument("--stages",default="sampled")
    ap.add_argument("--slots",default="sampled")
    ap.add_argument("--fault-bits",default="0,8,16,23")
    ap.add_argument("--mitigation-action",default="detect-only",choices=["detect-only","recompute","abort"])
    ap.add_argument("--max-retries",type=int,default=1)
    ap.add_argument("--release",action="store_true")
    ap.add_argument("--validate",action="store_true")
    ap.add_argument("--include-baseline",action="store_true")
    ap.add_argument("--dry-run",action="store_true")
    ap.add_argument("--limit",type=int,default=0)
    args=ap.parse_args()
    out=Path(args.output); out.parent.mkdir(parents=True,exist_ok=True)
    fields=["n","bits","scale_bits","ntt_impl","fault_enabled","fault_op","fault_operand",
            "fault_stage","fault_slot","fault_bit","mitigation","mitigation_action",
            "checksum_mode","command","returncode","execution_valid","golden_match",
            "fault_observed","rms_error","relative_l2_error","max_abs_error","mean_abs_error",
            "snr_db","elapsed_ns","scratch_bytes","checks_performed","check_failures",
            "s1_failures","s2_failures","recomputations","mitigation_elapsed_ns","stderr_tail"]
    count=0
    with out.open("w",newline="") as f:
        w=csv.DictWriter(f,fieldnames=fields,extrasaction="ignore"); w.writeheader()
        for n in ints(args.n_values):
          for impl in csvs(args.ntt_impls):
           for mit in csvs(args.mitigations):
            modes=csvs(args.checksum_modes) if mit=="stage-checksum" else ["none"]
            for mode in modes:
             if args.include_baseline:
              c=cmd(args,n,impl,mit,mode,True)
              row={"n":n,"bits":args.bits,"scale_bits":args.scale_bits,"ntt_impl":impl,
                   "fault_enabled":"false","mitigation":mit,"mitigation_action":args.mitigation_action,
                   "checksum_mode":mode,"command":" ".join(c)}
              row.update(run(c,args.dry_run)); w.writerow(row); count+=1
              if args.limit and count>=args.limit: return
             for op in csvs(args.fault_ops):
              op_operands=csvs(args.operands) if op=="mul" else ["a"]
              for operand in op_operands:
               for st in stages(args.stages,n,op):
                for sl in slots(args.slots,n):
                 for bit in ints(args.fault_bits):
                  c=cmd(args,n,impl,mit,mode,False,op,operand,st,sl,bit)
                  row={"n":n,"bits":args.bits,"scale_bits":args.scale_bits,"ntt_impl":impl,
                       "fault_enabled":"true","fault_op":op,"fault_operand":operand,
                       "fault_stage":st,"fault_slot":sl,"fault_bit":bit,"mitigation":mit,
                       "mitigation_action":args.mitigation_action,"checksum_mode":mode,
                       "command":" ".join(c)}
                  row.update(run(c,args.dry_run)); w.writerow(row); count+=1
                  if args.limit and count>=args.limit: return
    print(f"Wrote {count} rows to {out}")
if __name__=="__main__": main()


# TODO: ensure the innermost experiment loop wraps command generation with:
#     for fault_site in fault_sites:
# so every selected fault site is swept.
