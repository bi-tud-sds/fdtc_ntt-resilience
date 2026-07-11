use crate::ckks::CkksExecutionTrace;
use crate::metrics::DecodedMetrics;
use crate::mitigation::MitigationMetrics;
use crate::ntt::StageTrace;
use crate::params::RingParams;
use num_complex::Complex64;

const RED: &str = "\x1b[31m";
const RESET: &str = "\x1b[0m";

pub fn print_ring(params: &RingParams) {
    println!("================ Ring Parameters ================");
    println!("Ring:              Z_q[X] / (X^N + 1)");
    println!("N:                 {}", params.n);
    println!("Modulus q:         {}", params.modulus);
    println!("Modulus bits:      {}", params.modulus_bits);
    println!("Primitive 2N root: {}", params.primitive_2n_root);
    println!("Primitive N root:  {}", params.primitive_n_root);
    println!("=================================================");
}

pub fn print_complex_slots(label: &str, slots: &[Complex64], limit: usize) {
    println!("================ {} ================", label);
    for (i, z) in slots.iter().take(limit).enumerate() {
        println!("slot[{i:>3}] = {:+.8} {:+.8}i", z.re, z.im);
    }
    if slots.len() > limit {
        println!("... {} more slots", slots.len() - limit);
    }
    println!("=================================================");
}

pub fn print_decoded_comparison(correct: &[Complex64], faulty: &[Complex64], limit: usize) {
    println!("================ Decoded Correct vs Faulty ================");
    for i in 0..correct.len().min(limit) {
        let e = faulty[i] - correct[i];
        let line = format!(
            "slot[{i:>3}] correct={:+.8}{:+.8}i  faulty={:+.8}{:+.8}i  error_abs={:.8e}",
            correct[i].re,
            correct[i].im,
            faulty[i].re,
            faulty[i].im,
            e.norm()
        );
        if e.norm() > 0.0 {
            println!("{}{}{}", RED, line, RESET);
        } else {
            println!("{}", line);
        }
    }
    if correct.len() > limit {
        println!("... {} more slots", correct.len() - limit);
    }
    println!("============================================================");
}

pub fn print_decoded_metrics(m: &DecodedMetrics) {
    let color = if m.rms_error > 0.0 { RED } else { "" };
    let reset = if m.rms_error > 0.0 { RESET } else { "" };
    println!(
        "{}================ Decoded-Domain Fault Metrics ================{}",
        color, reset
    );
    println!("Slots:             {}", m.slot_count);
    println!("Max abs error:     {:.8e}", m.max_abs_error);
    println!("Mean abs error:    {:.8e}", m.mean_abs_error);
    println!("RMS error:         {:.8e}", m.rms_error);
    println!("Relative L2 error: {:.8e}", m.relative_l2_error);
    println!("SNR dB:            {:.8}", m.snr_db);
    println!(
        "{}============================================================{}",
        color, reset
    );
}

pub fn print_mitigation_metrics(m: &MitigationMetrics) {
    if !m.mitigation_enabled {
        println!("================ Mitigation Metrics ================");
        println!("Mitigation:        none");
        println!("====================================================");
        return;
    }

    let color = if m.fault_detected { RED } else { "" };
    let reset = if m.fault_detected { RESET } else { "" };
    println!(
        "{}================ Mitigation Metrics ================{}",
        color, reset
    );
    println!("Mitigation:        {}", m.mitigation_kind);
    println!("Action:            {}", m.mitigation_action);
    println!("Checksum mode:     {}", m.checksum_mode);
    println!("Checks performed:  {}", m.checks_performed);
    println!("Check failures:    {}", m.check_failures);
    println!("Stage checks:      {}", m.stage_checks_performed);
    println!("Stage failures:    {}", m.stage_check_failures);
    println!("S1 failures:       {}", m.stage_checksum_s1_failures);
    println!("S2 failures:       {}", m.stage_checksum_s2_failures);
    println!(
        "Fault detected:    {}",
        if m.fault_detected { "yes" } else { "no" }
    );
    println!(
        "Fault corrected:   {}",
        if m.fault_corrected { "yes" } else { "no" }
    );
    println!("Recomputations:    {}", m.recomputations);
    println!("Mitigation time ns: {}", m.mitigation_elapsed_ns);
    println!(
        "{}===================================================={}",
        color, reset
    );
}

pub fn print_ckks_trace(trace: &CkksExecutionTrace, limit: usize) {
    if trace.encoded_a.is_none()
        && trace.correct_ntt_a.is_none()
        && trace.correct_mul_ntt.is_none()
        && trace.correct_intt_stages.is_empty()
        && trace.decoded_correct.is_none()
    {
        return;
    }

    println!("================ CKKS Execution Trace ================");

    if let Some(v) = &trace.encoded_a {
        print_u64_vector("Encoded polynomial a", v, limit, false);
    }
    if let Some(v) = &trace.encoded_b {
        print_u64_vector("Encoded polynomial b", v, limit, false);
    }
    if let Some(v) = &trace.twisted_a {
        print_u64_vector("Twisted a before NTT", v, limit, false);
    }
    if let Some(v) = &trace.twisted_b {
        print_u64_vector("Twisted b before NTT", v, limit, false);
    }

    if let Some(v) = &trace.correct_ntt_a {
        print_u64_vector("Correct NTT(a)", v, limit, false);
    }
    if let Some(v) = &trace.faulty_ntt_a {
        print_u64_vector("Faulty NTT(a)", v, limit, true);
    }
    if let Some(v) = &trace.correct_ntt_b {
        print_u64_vector("Correct NTT(b)", v, limit, false);
    }
    if let Some(v) = &trace.faulty_ntt_b {
        print_u64_vector("Faulty NTT(b)", v, limit, true);
    }

    print_stage_traces(
        "Correct forward NTT stages",
        &trace.correct_ntt_stages,
        limit,
        false,
    );
    print_stage_traces(
        "Faulty forward NTT stages",
        &trace.faulty_ntt_stages,
        limit,
        true,
    );

    if let Some(v) = &trace.correct_mul_ntt {
        print_u64_vector("Correct pointwise multiply in NTT domain", v, limit, false);
    }
    if let Some(v) = &trace.faulty_mul_ntt {
        print_u64_vector("Faulty pointwise multiply in NTT domain", v, limit, true);
    }

    print_stage_traces(
        "Correct inverse NTT stages",
        &trace.correct_intt_stages,
        limit,
        false,
    );
    print_stage_traces(
        "Faulty inverse NTT stages",
        &trace.faulty_intt_stages,
        limit,
        true,
    );

    if let Some(v) = &trace.correct_coeffs {
        print_u64_vector("Correct final coefficients", v, limit, false);
    }
    if let Some(v) = &trace.faulty_coeffs {
        print_u64_vector("Faulty final coefficients", v, limit, true);
    }

    if let Some(v) = &trace.decoded_correct {
        print_complex_slots("Trace decoded correct slots", v, limit);
    }
    if let Some(v) = &trace.decoded_faulty {
        println!(
            "{}================ Trace decoded faulty slots ================{}",
            RED, RESET
        );
        for (i, z) in v.iter().take(limit).enumerate() {
            println!("{}slot[{i:>3}] = {:+.8} {:+.8}i{}", RED, z.re, z.im, RESET);
        }
        if v.len() > limit {
            println!("{}... {} more slots{}", RED, v.len() - limit, RESET);
        }
        println!(
            "{}==========================================================={}",
            RED, RESET
        );
    }

    println!("======================================================");
}

fn print_u64_vector(label: &str, v: &[u64], limit: usize, faulted: bool) {
    let prefix = if faulted { RED } else { "" };
    let suffix = if faulted { RESET } else { "" };
    println!(
        "{}---------------- {} ----------------{}",
        prefix, label, suffix
    );
    for (i, x) in v.iter().take(limit).enumerate() {
        println!("{}[{i:>3}] = {}{}", prefix, x, suffix);
    }
    if v.len() > limit {
        println!(
            "{}... {} more coefficients{}",
            prefix,
            v.len() - limit,
            suffix
        );
    }
}

fn print_stage_traces(label: &str, stages: &[StageTrace], limit: usize, mark_faulty: bool) {
    if stages.is_empty() {
        return;
    }

    println!("---------------- {} ----------------", label);
    for stage in stages {
        let faulted = mark_faulty || stage.faulted;
        let prefix = if faulted { RED } else { "" };
        let suffix = if faulted { RESET } else { "" };
        println!(
            "{}Stage {}{}{}",
            prefix,
            stage.stage,
            if stage.faulted {
                " [FAULT INJECTED]"
            } else {
                ""
            },
            suffix
        );
        print_compact_vector("input ", &stage.input, limit, faulted);
        print_compact_vector("output", &stage.output, limit, faulted);
    }
}

fn print_compact_vector(label: &str, v: &[u64], limit: usize, faulted: bool) {
    let prefix = if faulted { RED } else { "" };
    let suffix = if faulted { RESET } else { "" };
    let shown: Vec<String> = v.iter().take(limit).map(|x| x.to_string()).collect();
    if v.len() > limit {
        println!(
            "{}  {}: [{} ... +{} more]{}",
            prefix,
            label,
            shown.join(", "),
            v.len() - limit,
            suffix
        );
    } else {
        println!("{}  {}: [{}]{}", prefix, label, shown.join(", "), suffix);
    }
}
