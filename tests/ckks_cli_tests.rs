use std::process::Command;

fn run_ckks_demo(args: &[&str]) -> String {
    let exe = env!("CARGO_BIN_EXE_ckks_ntt");
    let output = Command::new(exe)
        .args(args)
        .output()
        .expect("failed to run ckks_ntt binary");

    assert!(
        output.status.success(),
        "command failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    String::from_utf8_lossy(&output.stdout).to_string()
}

#[test]
fn cli_ckks_demo_validates_without_fault() {
    let stdout = run_ckks_demo(&[
        "ckks-demo",
        "--n",
        "16",
        "--bits",
        "24",
        "--scale-bits",
        "10",
        "--validate",
    ]);

    assert!(stdout.contains("CKKS Demo Validation"));
    assert!(stdout.contains("Execution valid: PASS"));
    assert!(stdout.contains("Golden match:    PASS"));
    assert!(stdout.contains("Fault observed:  N/A"));
    assert!(stdout.contains("RMS error:         0.00000000e0"));
}

#[test]
fn cli_ckks_demo_validates_with_ntt_fault_and_trace_all() {
    let stdout = run_ckks_demo(&[
        "ckks-demo",
        "--n",
        "16",
        "--bits",
        "24",
        "--scale-bits",
        "10",
        "--fault",
        "--fault-op",
        "ntt",
        "--fault-stage",
        "0",
        "--fault-slot",
        "0",
        "--fault-bit",
        "0",
        "--trace-all",
        "--validate",
        "--print-slots",
        "4",
    ]);

    assert!(stdout.contains("CKKS Demo Validation"));
    assert!(stdout.contains("Execution valid: PASS"));
    assert!(stdout.contains("CKKS Execution Trace"));
    assert!(stdout.contains("Encoded polynomial a"));
    assert!(stdout.contains("Correct NTT(a)"));
    assert!(stdout.contains("Faulty NTT(a)"));
    assert!(stdout.contains("Correct pointwise multiply in NTT domain"));
    assert!(stdout.contains("Faulty pointwise multiply in NTT domain"));
    assert!(stdout.contains("Correct inverse NTT stages"));
    assert!(stdout.contains("Faulty inverse NTT stages"));
    assert!(stdout.contains("Trace decoded correct slots"));
    assert!(stdout.contains("Trace decoded faulty slots"));
}

#[test]
fn cli_ckks_demo_validates_with_mul_fault_and_trace_mul() {
    let stdout = run_ckks_demo(&[
        "ckks-demo",
        "--n",
        "16",
        "--bits",
        "24",
        "--scale-bits",
        "10",
        "--fault",
        "--fault-op",
        "mul",
        "--fault-slot",
        "0",
        "--fault-bit",
        "4",
        "--trace-mul",
        "--validate",
    ]);

    assert!(stdout.contains("Execution valid: PASS"));
    assert!(stdout.contains("Correct pointwise multiply in NTT domain"));
    assert!(stdout.contains("Faulty pointwise multiply in NTT domain"));
}

#[test]
fn cli_ckks_demo_validates_with_intt_fault_and_trace_intt() {
    let stdout = run_ckks_demo(&[
        "ckks-demo",
        "--n",
        "16",
        "--bits",
        "24",
        "--scale-bits",
        "10",
        "--fault",
        "--fault-op",
        "intt",
        "--fault-stage",
        "0",
        "--fault-slot",
        "0",
        "--fault-bit",
        "4",
        "--trace-intt",
        "--validate",
    ]);

    assert!(stdout.contains("Execution valid: PASS"));
    assert!(stdout.contains("Correct inverse NTT stages"));
    assert!(stdout.contains("Faulty inverse NTT stages"));
}
