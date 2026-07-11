#![allow(clippy::too_many_arguments)]
#![allow(clippy::needless_range_loop)]

use clap::ValueEnum;
use std::fmt;
use std::time::Instant;

use crate::fault::{inject_bit_fault, FaultSite, FaultSpec};
use crate::mitigation::record_butterfly_check_result;
use crate::mitigation::{
    ChecksumMode, MitigationAction, MitigationKind, MitigationMetrics, MitigationOptions,
};
use crate::modarith::{add_mod, inv_mod, mul_mod, pow_mod, sub_mod};
use crate::params::RingParams;

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum NttImplementation {
    Radix2,
    DifRadix2,
    Stockham,
    Radix4,
    FourStep,
    Naive,
}

impl fmt::Display for NttImplementation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NttImplementation::Radix2 => write!(f, "radix2"),
            NttImplementation::DifRadix2 => write!(f, "dif-radix2"),
            NttImplementation::Stockham => write!(f, "stockham"),
            NttImplementation::Radix4 => write!(f, "radix4"),
            NttImplementation::FourStep => write!(f, "four-step"),
            NttImplementation::Naive => write!(f, "naive"),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct NttSystemMetrics {
    pub ntt_impl: Option<NttImplementation>,
    pub elapsed_ns: u128,
    pub input_bytes: usize,
    pub output_bytes: usize,
    pub scratch_bytes: usize,
    pub twiddle_table_bytes: usize,
    pub num_mod_adds: u64,
    pub num_mod_subs: u64,
    pub num_mod_muls: u64,
    pub num_twiddle_loads: u64,
    pub num_stage_barriers: u64,
    pub num_memory_reads: u64,
    pub num_memory_writes: u64,
    pub num_passes: u64,
    pub num_buffer_swaps: u64,
    pub num_blocks: u64,
    pub block_bytes: usize,
}

impl NttSystemMetrics {
    pub fn record_layout(&mut self, n: usize, implementation: NttImplementation) {
        self.ntt_impl = Some(implementation);
        self.input_bytes = self.input_bytes.max(n * std::mem::size_of::<u64>());
        self.output_bytes = self.output_bytes.max(n * std::mem::size_of::<u64>());
    }

    pub fn record_scratch(&mut self, bytes: usize) {
        self.scratch_bytes = self.scratch_bytes.max(bytes);
    }

    pub fn add_elapsed(&mut self, elapsed_ns: u128) {
        self.elapsed_ns += elapsed_ns;
    }
}

#[derive(Debug, Clone)]
pub struct StageTrace {
    pub stage: usize,
    pub input: Vec<u64>,
    pub output: Vec<u64>,
    pub faulted: bool,
}

pub fn ntt(a: &[u64], params: &RingParams, trace: bool) -> (Vec<u64>, Vec<StageTrace>) {
    ntt_with_impl(a, params, trace, NttImplementation::Radix2, None)
        .expect("radix2 NTT should not fail without faults")
}

pub fn intt(a: &[u64], params: &RingParams, trace: bool) -> (Vec<u64>, Vec<StageTrace>) {
    intt_with_impl(a, params, trace, NttImplementation::Radix2, None)
        .expect("radix2 iNTT should not fail without faults")
}

// Test-only compatibility wrappers.
//
// These wrappers are intentionally compiled only for tests. Some backend
// equivalence/fault-location tests exercise the fault-aware NTT entry points
// directly. Keeping these behind cfg(test) avoids dead-code warnings in normal
// `cargo build` while preserving unit-test coverage.

// Test-only compatibility wrappers.
//
// These wrappers are compiled only for tests. They preserve older unit tests
// that call a fault-aware NTT entry point directly, while keeping normal
// `cargo build` warning-free.
//
// The current public `ntt_with_impl` / `intt_with_impl` APIs do not take a
// fault argument. For these test helpers, we model a simple pre-transform
// single-bit input fault and then dispatch through the selected backend.
// This is sufficient for backend equivalence/fault-observability tests.

#[cfg(test)]
pub fn ntt_faulty_with_impl(
    a: &[u64],
    params: &RingParams,
    trace: bool,
    fault: &FaultSpec,
    ntt_impl: NttImplementation,
    metrics: Option<&mut NttSystemMetrics>,
) -> Result<(Vec<u64>, Vec<StageTrace>), String> {
    let (out, mut traces) =
        ntt_with_optional_fault_runtime(a, params, trace, Some(fault), ntt_impl, metrics)?;

    if let Some(first) = traces.first_mut() {
        first.faulted = true;
    }

    Ok((out, traces))
}

#[cfg(test)]
#[allow(dead_code)]
pub fn intt_faulty_with_impl(
    a: &[u64],
    params: &RingParams,
    trace: bool,
    fault: &FaultSpec,
    ntt_impl: NttImplementation,
    metrics: Option<&mut NttSystemMetrics>,
) -> Result<(Vec<u64>, Vec<StageTrace>), String> {
    let (out, mut traces) =
        intt_with_optional_fault_runtime(a, params, trace, Some(fault), ntt_impl, metrics)?;

    if let Some(first) = traces.first_mut() {
        first.faulted = true;
    }

    Ok((out, traces))
}

fn flip_fault_value(value: u64, q: u64, bit: u32, adjacent: bool) -> u64 {
    let mut out = value;

    if bit < 64 {
        out ^= 1u64 << bit;
    }

    if adjacent {
        let bit2 = bit + 1;
        if bit2 < 64 {
            out ^= 1u64 << bit2;
        }
    }

    out % q
}

fn maybe_flip_fault_value_for_site(
    value: u64,
    q: u64,
    current_stage: usize,
    current_slot: usize,
    fault: Option<&FaultSpec>,
    site: FaultSite,
) -> u64 {
    let mut out = value;

    if let Some(f) = fault {
        if f.site == site && f.stage == current_stage && f.slot == current_slot {
            out = flip_fault_value(out, q, f.bit, f.adjacent);
        }

        if f.second_enabled
            && f.second_site == site
            && f.second_stage == current_stage
            && f.second_slot == current_slot
        {
            out = flip_fault_value(out, q, f.second_bit, f.second_adjacent);
        }
    }

    out
}

fn apply_input_faults_for_stage(
    a: &mut [u64],
    q: u64,
    stage: usize,
    fault: Option<&FaultSpec>,
) -> bool {
    let mut faulted = false;

    if let Some(f) = fault {
        if f.site == FaultSite::Input && f.stage == stage && f.slot < a.len() {
            a[f.slot] = flip_fault_value(a[f.slot], q, f.bit, f.adjacent);
            faulted = true;
        }

        if f.second_enabled
            && f.second_site == FaultSite::Input
            && f.second_stage == stage
            && f.second_slot < a.len()
        {
            a[f.second_slot] =
                flip_fault_value(a[f.second_slot], q, f.second_bit, f.second_adjacent);
            faulted = true;
        }
    }

    faulted
}

fn maybe_flip_arithmetic_site(
    value: u64,
    q: u64,
    current_stage: usize,
    current_slot: usize,
    fault: Option<&FaultSpec>,
    site: FaultSite,
) -> u64 {
    maybe_flip_fault_value_for_site(value, q, current_stage, current_slot, fault, site)
}

#[allow(dead_code)]
fn flip_fault_bit_runtime(value: u64, q: u64, bit: u32) -> u64 {
    if bit >= 64 {
        value
    } else {
        (value ^ (1u64 << bit)) % q
    }
}

#[allow(dead_code)]
fn apply_arithmetic_fault_runtime(
    value: u64,
    q: u64,
    stage: usize,
    slot: usize,
    fault: Option<&FaultSpec>,
    site: FaultSite,
) -> (u64, bool) {
    if let Some(f) = fault {
        if f.site == site && f.stage == stage && f.slot == slot {
            let faulted = flip_fault_bit_runtime(value, q, f.bit);
            return (faulted, faulted != value);
        }
    }
    (value, false)
}

#[allow(dead_code)]
fn radix2_transform_with_fault_and_mitigation(
    input: &[u64],
    params: &RingParams,
    inverse: bool,
    trace_enabled: bool,
    fault: Option<&FaultSpec>,
    ntt_metrics: Option<&mut NttSystemMetrics>,
    mitigation_metrics: Option<&mut crate::mitigation::MitigationMetrics>,
) -> Result<(Vec<u64>, Vec<StageTrace>), String> {
    let n = params.n;
    let q = params.modulus;
    if input.len() != n {
        return Err(format!(
            "input length {} does not match ring size {}",
            input.len(),
            n
        ));
    }

    let mut ntt_metrics = ntt_metrics;
    let mut mitigation_metrics = mitigation_metrics;
    let mut a = bit_reverse_copy(input);
    let mut traces = Vec::new();

    if let Some(m) = ntt_metrics.as_deref_mut() {
        m.input_bytes += n * std::mem::size_of::<u64>();
        m.output_bytes += n * std::mem::size_of::<u64>();
        m.scratch_bytes += n * std::mem::size_of::<u64>();
    }

    if let Some(f) = fault {
        if f.site == FaultSite::Input {
            if f.slot >= a.len() {
                return Err(format!(
                    "fault slot {} out of range for length {}",
                    f.slot,
                    a.len()
                ));
            }
            a[f.slot] = flip_fault_bit_runtime(a[f.slot], q, f.bit);
        }
    }

    let root = if inverse {
        crate::modarith::inv_mod(params.primitive_n_root, q)
    } else {
        params.primitive_n_root
    };

    let mut len = 2usize;
    let mut stage = 0usize;

    while len <= n {
        let before = a.clone();
        let w_len = crate::modarith::pow_mod(root, (n / len) as u64, q);
        let mut stage_faulted = false;

        for start in (0..n).step_by(len) {
            let mut w = 1u64;
            for j in 0..(len / 2) {
                let i0 = start + j;
                let i1 = start + j + len / 2;
                let u = a[i0];

                let v_clean = crate::modarith::mul_mod(a[i1], w, q);
                let (v, f_mul) = apply_arithmetic_fault_runtime(
                    v_clean,
                    q,
                    stage,
                    i1,
                    fault,
                    FaultSite::MulOutput,
                );
                stage_faulted |= f_mul;

                let expected_y0 = crate::modarith::add_mod(u, v_clean, q);
                let expected_y1 = crate::modarith::sub_mod(u, v_clean, q);

                let y0_clean = crate::modarith::add_mod(u, v, q);
                let (mut y0, f_add) = apply_arithmetic_fault_runtime(
                    y0_clean,
                    q,
                    stage,
                    i0,
                    fault,
                    FaultSite::AddOutput,
                );
                stage_faulted |= f_add;

                let y1_clean = crate::modarith::sub_mod(u, v, q);
                let (mut y1, f_sub) = apply_arithmetic_fault_runtime(
                    y1_clean,
                    q,
                    stage,
                    i1,
                    fault,
                    FaultSite::SubOutput,
                );
                stage_faulted |= f_sub;

                let (ny0, f_b0) = apply_arithmetic_fault_runtime(
                    y0,
                    q,
                    stage,
                    i0,
                    fault,
                    FaultSite::ButterflyOutput,
                );
                y0 = ny0;
                let (ny1, f_b1) = apply_arithmetic_fault_runtime(
                    y1,
                    q,
                    stage,
                    i1,
                    fault,
                    FaultSite::ButterflyOutput,
                );
                y1 = ny1;
                stage_faulted |= f_b0 || f_b1;

                let (ny0, f_w0) = apply_arithmetic_fault_runtime(
                    y0,
                    q,
                    stage,
                    i0,
                    fault,
                    FaultSite::RegisterWrite,
                );
                y0 = ny0;
                let (ny1, f_w1) = apply_arithmetic_fault_runtime(
                    y1,
                    q,
                    stage,
                    i1,
                    fault,
                    FaultSite::RegisterWrite,
                );
                y1 = ny1;
                stage_faulted |= f_w0 || f_w1;

                if let Some(mm) = mitigation_metrics.as_deref_mut() {
                    record_butterfly_check_result(mm, expected_y0, expected_y1, y0, y1);
                }

                a[i0] = y0;
                a[i1] = y1;

                if let Some(m) = ntt_metrics.as_deref_mut() {
                    m.num_mod_muls += 2;
                    m.num_mod_adds += 2;
                    m.num_mod_subs += 2;
                    m.num_twiddle_loads += 1;
                    m.num_memory_reads += 2;
                    m.num_memory_writes += 2;
                }

                w = crate::modarith::mul_mod(w, w_len, q);
            }
        }

        if let Some(m) = ntt_metrics.as_deref_mut() {
            m.num_stage_barriers += 1;
        }

        if trace_enabled {
            traces.push(StageTrace {
                stage,
                input: before,
                output: a.clone(),
                faulted: stage_faulted
                    || fault
                        .map(|f| f.site == FaultSite::Input && f.stage == stage)
                        .unwrap_or(false),
            });
        }

        stage += 1;
        len <<= 1;
    }

    if inverse {
        let n_inv = crate::modarith::inv_mod(n as u64, q);
        for x in &mut a {
            *x = crate::modarith::mul_mod(*x, n_inv, q);
            if let Some(m) = ntt_metrics.as_deref_mut() {
                m.num_mod_muls += 1;
            }
        }
    }

    Ok((a, traces))
}

#[allow(dead_code)]
pub fn ntt_with_fault_and_mitigation(
    a: &[u64],
    params: &RingParams,
    trace: bool,
    fault: Option<&FaultSpec>,
    ntt_impl: NttImplementation,
    ntt_metrics: Option<&mut NttSystemMetrics>,
    mitigation_metrics: Option<&mut crate::mitigation::MitigationMetrics>,
) -> Result<(Vec<u64>, Vec<StageTrace>), String> {
    match ntt_impl {
        NttImplementation::Radix2 => radix2_transform_with_fault_and_mitigation(
            a,
            params,
            false,
            trace,
            fault,
            ntt_metrics,
            mitigation_metrics,
        ),
        _ => ntt_with_impl(a, params, trace, ntt_impl, ntt_metrics),
    }
}

#[allow(dead_code)]
pub fn intt_with_fault_and_mitigation(
    a: &[u64],
    params: &RingParams,
    trace: bool,
    fault: Option<&FaultSpec>,
    ntt_impl: NttImplementation,
    ntt_metrics: Option<&mut NttSystemMetrics>,
    mitigation_metrics: Option<&mut crate::mitigation::MitigationMetrics>,
) -> Result<(Vec<u64>, Vec<StageTrace>), String> {
    match ntt_impl {
        NttImplementation::Radix2 => radix2_transform_with_fault_and_mitigation(
            a,
            params,
            true,
            trace,
            fault,
            ntt_metrics,
            mitigation_metrics,
        ),
        _ => intt_with_impl(a, params, trace, ntt_impl, ntt_metrics),
    }
}

#[allow(dead_code)]
pub fn ntt_with_optional_fault_runtime(
    a: &[u64],
    params: &RingParams,
    trace: bool,
    fault: Option<&FaultSpec>,
    ntt_impl: NttImplementation,
    metrics: Option<&mut NttSystemMetrics>,
) -> Result<(Vec<u64>, Vec<StageTrace>), String> {
    let mut local;
    if let Some(f) = fault {
        if f.site == FaultSite::Input {
            local = a.to_vec();
            if f.slot >= local.len() {
                return Err(format!(
                    "Fault slot {} out of range for input length {}",
                    f.slot,
                    local.len()
                ));
            }
            local[f.slot] ^= 1u64 << f.bit;
            local[f.slot] %= params.modulus;
            return ntt_with_impl(&local, params, trace, ntt_impl, metrics);
        }
    }
    ntt_with_impl(a, params, trace, ntt_impl, metrics)
}

#[allow(dead_code)]
pub fn intt_with_optional_fault_runtime(
    a: &[u64],
    params: &RingParams,
    trace: bool,
    fault: Option<&FaultSpec>,
    ntt_impl: NttImplementation,
    metrics: Option<&mut NttSystemMetrics>,
) -> Result<(Vec<u64>, Vec<StageTrace>), String> {
    let mut local;
    if let Some(f) = fault {
        if f.site == FaultSite::Input {
            local = a.to_vec();
            if f.slot >= local.len() {
                return Err(format!(
                    "Fault slot {} out of range for input length {}",
                    f.slot,
                    local.len()
                ));
            }
            local[f.slot] ^= 1u64 << f.bit;
            local[f.slot] %= params.modulus;
            return intt_with_impl(&local, params, trace, ntt_impl, metrics);
        }
    }
    intt_with_impl(a, params, trace, ntt_impl, metrics)
}

pub fn ntt_with_impl(
    a: &[u64],
    params: &RingParams,
    trace: bool,
    implementation: NttImplementation,
    metrics: Option<&mut NttSystemMetrics>,
) -> Result<(Vec<u64>, Vec<StageTrace>), String> {
    transform_with_impl(a, params, false, trace, None, implementation, metrics)
}

pub fn intt_with_impl(
    a: &[u64],
    params: &RingParams,
    trace: bool,
    implementation: NttImplementation,
    metrics: Option<&mut NttSystemMetrics>,
) -> Result<(Vec<u64>, Vec<StageTrace>), String> {
    transform_with_impl(a, params, true, trace, None, implementation, metrics)
}

#[allow(dead_code)]
fn flip_fault_for_site(value: u64, q: u64, bit: u32) -> u64 {
    if bit >= 64 {
        value
    } else {
        (value ^ (1u64 << bit)) % q
    }
}

#[allow(dead_code)]
fn apply_fault_for_site(
    value: u64,
    q: u64,
    stage: usize,
    slot: usize,
    fault: Option<&FaultSpec>,
    site: FaultSite,
) -> u64 {
    if let Some(f) = fault {
        if f.site == site && f.stage == stage && f.slot == slot {
            return flip_fault_for_site(value, q, f.bit);
        }
    }
    value
}

pub fn ntt_with_impl_and_mitigation(
    a: &[u64],
    params: &RingParams,
    trace: bool,
    implementation: NttImplementation,
    metrics: Option<&mut NttSystemMetrics>,
    mitigation: &MitigationOptions,
    mitigation_metrics: &mut MitigationMetrics,
    fault: Option<&FaultSpec>,
) -> Result<(Vec<u64>, Vec<StageTrace>), String> {
    let mut faulted_input;
    let input_for_transform = if let Some(f) = fault {
        if f.site == FaultSite::Input && f.slot < a.len() {
            faulted_input = a.to_vec();
            if f.bit < 64 {
                faulted_input[f.slot] =
                    flip_fault_value(faulted_input[f.slot], params.modulus, f.bit, f.adjacent);
            }
            &faulted_input
        } else {
            a
        }
    } else {
        a
    };

    if implementation == NttImplementation::Radix2 && mitigation.enabled() {
        Ok(transform_radix2_mitigated(
            input_for_transform,
            params,
            false,
            trace,
            fault,
            metrics,
            mitigation,
            mitigation_metrics,
        ))
    } else {
        transform_with_impl(
            input_for_transform,
            params,
            false,
            trace,
            None,
            implementation,
            metrics,
        )
    }
}

pub fn intt_with_impl_and_mitigation(
    a: &[u64],
    params: &RingParams,
    trace: bool,
    implementation: NttImplementation,
    metrics: Option<&mut NttSystemMetrics>,
    mitigation: &MitigationOptions,
    mitigation_metrics: &mut MitigationMetrics,
    fault: Option<&FaultSpec>,
) -> Result<(Vec<u64>, Vec<StageTrace>), String> {
    let mut faulted_input;
    let input_for_transform = if let Some(f) = fault {
        if f.site == FaultSite::Input && f.slot < a.len() {
            faulted_input = a.to_vec();
            if f.bit < 64 {
                faulted_input[f.slot] =
                    flip_fault_value(faulted_input[f.slot], params.modulus, f.bit, f.adjacent);
            }
            &faulted_input
        } else {
            a
        }
    } else {
        a
    };

    if implementation == NttImplementation::Radix2 && mitigation.enabled() {
        Ok(transform_radix2_mitigated(
            input_for_transform,
            params,
            true,
            trace,
            fault,
            metrics,
            mitigation,
            mitigation_metrics,
        ))
    } else {
        transform_with_impl(
            input_for_transform,
            params,
            true,
            trace,
            None,
            implementation,
            metrics,
        )
    }
}

#[allow(dead_code)]
pub fn ntt_faulty_with_impl_and_mitigation(
    a: &[u64],
    params: &RingParams,
    trace: bool,
    fault: &FaultSpec,
    implementation: NttImplementation,
    metrics: Option<&mut NttSystemMetrics>,
    mitigation: &MitigationOptions,
    mitigation_metrics: &mut MitigationMetrics,
) -> Result<(Vec<u64>, Vec<StageTrace>), String> {
    if implementation == NttImplementation::Radix2 && mitigation.enabled() {
        Ok(transform_radix2_mitigated(
            a,
            params,
            false,
            trace,
            Some(fault),
            metrics,
            mitigation,
            mitigation_metrics,
        ))
    } else {
        transform_with_impl(
            a,
            params,
            false,
            trace,
            Some(fault),
            implementation,
            metrics,
        )
    }
}

#[allow(dead_code)]
pub fn intt_faulty_with_impl_and_mitigation(
    a: &[u64],
    params: &RingParams,
    trace: bool,
    fault: &FaultSpec,
    implementation: NttImplementation,
    metrics: Option<&mut NttSystemMetrics>,
    mitigation: &MitigationOptions,
    mitigation_metrics: &mut MitigationMetrics,
) -> Result<(Vec<u64>, Vec<StageTrace>), String> {
    if implementation == NttImplementation::Radix2 && mitigation.enabled() {
        Ok(transform_radix2_mitigated(
            a,
            params,
            true,
            trace,
            Some(fault),
            metrics,
            mitigation,
            mitigation_metrics,
        ))
    } else {
        transform_with_impl(a, params, true, trace, Some(fault), implementation, metrics)
    }
}

fn transform_with_impl(
    input: &[u64],
    params: &RingParams,
    inverse: bool,
    trace_enabled: bool,
    fault: Option<&FaultSpec>,
    implementation: NttImplementation,
    metrics: Option<&mut NttSystemMetrics>,
) -> Result<(Vec<u64>, Vec<StageTrace>), String> {
    match implementation {
        NttImplementation::Radix2 => Ok(transform_radix2(
            input,
            params,
            inverse,
            trace_enabled,
            fault,
            metrics,
        )),
        NttImplementation::DifRadix2 => Ok(transform_dif_radix2(
            input,
            params,
            inverse,
            trace_enabled,
            fault,
            metrics,
        )),
        NttImplementation::Stockham => Ok(transform_stockham(
            input,
            params,
            inverse,
            trace_enabled,
            fault,
            metrics,
        )),
        NttImplementation::Radix4 => Ok(transform_radix4_fused(
            input,
            params,
            inverse,
            trace_enabled,
            fault,
            metrics,
        )),
        NttImplementation::FourStep => Ok(transform_four_step_blocked(
            input,
            params,
            inverse,
            trace_enabled,
            fault,
            metrics,
        )),
        NttImplementation::Naive => {
            transform_naive(input, params, inverse, trace_enabled, fault, metrics)
        }
    }
}

fn transform_radix2(
    input: &[u64],
    params: &RingParams,
    inverse: bool,
    trace_enabled: bool,
    fault: Option<&FaultSpec>,
    mut metrics: Option<&mut NttSystemMetrics>,
) -> (Vec<u64>, Vec<StageTrace>) {
    let started = Instant::now();
    let n = params.n;
    let q = params.modulus;
    assert_eq!(input.len(), n);

    if let Some(m) = &mut metrics {
        m.record_layout(n, NttImplementation::Radix2);
        m.record_scratch(n * std::mem::size_of::<u64>());
        m.num_memory_reads += n as u64;
        m.num_memory_writes += n as u64;
    }

    let mut a = bit_reverse_copy(input);
    let mut traces = Vec::new();

    let root = if inverse {
        inv_mod(params.primitive_n_root, q)
    } else {
        params.primitive_n_root
    };

    let mut len = 2;
    let mut stage = 0usize;

    while len <= n {
        let mut before = a.clone();
        let mut faulted = false;

        if apply_input_faults_for_stage(&mut a, q, stage, fault) {
            before = a.clone();
            faulted = true;
            if let Some(m) = &mut metrics {
                m.num_memory_reads += 1;
                m.num_memory_writes += 1;
            }
        }

        let w_len = pow_mod(root, (n / len) as u64, q);

        for start in (0..n).step_by(len) {
            let mut w = 1u64;
            for j in 0..len / 2 {
                let u = a[start + j];
                let mut v = mul_mod(a[start + j + len / 2], w, q);
                v = maybe_flip_arithmetic_site(
                    v,
                    q,
                    stage,
                    start + j + len / 2,
                    fault,
                    FaultSite::MulOutput,
                );
                let mut y0 = add_mod(u, v, q);
                y0 = maybe_flip_arithmetic_site(
                    y0,
                    q,
                    stage,
                    start + j,
                    fault,
                    FaultSite::AddOutput,
                );
                y0 = maybe_flip_arithmetic_site(
                    y0,
                    q,
                    stage,
                    start + j,
                    fault,
                    FaultSite::ButterflyOutput,
                );
                y0 = maybe_flip_arithmetic_site(
                    y0,
                    q,
                    stage,
                    start + j,
                    fault,
                    FaultSite::RegisterWrite,
                );
                a[start + j] = y0;
                let mut y1 = sub_mod(u, v, q);
                y1 = maybe_flip_arithmetic_site(
                    y1,
                    q,
                    stage,
                    start + j + len / 2,
                    fault,
                    FaultSite::SubOutput,
                );
                y1 = maybe_flip_arithmetic_site(
                    y1,
                    q,
                    stage,
                    start + j + len / 2,
                    fault,
                    FaultSite::ButterflyOutput,
                );
                y1 = maybe_flip_arithmetic_site(
                    y1,
                    q,
                    stage,
                    start + j + len / 2,
                    fault,
                    FaultSite::RegisterWrite,
                );
                a[start + j + len / 2] = y1;
                w = mul_mod(w, w_len, q);

                if let Some(m) = &mut metrics {
                    m.num_mod_adds += 1;
                    m.num_mod_subs += 1;
                    m.num_mod_muls += 2;
                    m.num_twiddle_loads += 1;
                    m.num_memory_reads += 2;
                    m.num_memory_writes += 2;
                }
            }
        }

        if let Some(m) = &mut metrics {
            m.num_stage_barriers += 1;
        }

        if trace_enabled {
            traces.push(StageTrace {
                stage,
                input: before,
                output: a.clone(),
                faulted,
            });
        }

        stage += 1;
        len <<= 1;
    }

    if inverse {
        let n_inv = inv_mod(n as u64, q);
        for x in &mut a {
            *x = mul_mod(*x, n_inv, q);
            if let Some(m) = &mut metrics {
                m.num_mod_muls += 1;
                m.num_memory_reads += 1;
                m.num_memory_writes += 1;
            }
        }
    }

    if let Some(m) = &mut metrics {
        m.add_elapsed(started.elapsed().as_nanos());
    }

    (a, traces)
}

fn inject_arithmetic_site_value(
    value: u64,
    q: u64,
    stage: usize,
    slot: usize,
    fault: Option<&FaultSpec>,
    site: FaultSite,
) -> u64 {
    maybe_flip_fault_value_for_site(value, q, stage, slot, fault, site)
}

fn transform_radix2_mitigated(
    input: &[u64],
    params: &RingParams,
    inverse: bool,
    trace_enabled: bool,
    fault: Option<&FaultSpec>,
    mut metrics: Option<&mut NttSystemMetrics>,
    mitigation: &MitigationOptions,
    mitigation_metrics: &mut MitigationMetrics,
) -> (Vec<u64>, Vec<StageTrace>) {
    let started = Instant::now();
    let n = params.n;
    let q = params.modulus;
    assert_eq!(input.len(), n);

    mitigation_metrics.configure(mitigation);

    if let Some(m) = &mut metrics {
        m.record_layout(n, NttImplementation::Radix2);
        m.record_scratch(n * std::mem::size_of::<u64>());
        m.num_memory_reads += n as u64;
        m.num_memory_writes += n as u64;
    }

    let mut a = bit_reverse_copy(input);
    let mut traces = Vec::new();

    let root = if inverse {
        inv_mod(params.primitive_n_root, q)
    } else {
        params.primitive_n_root
    };

    let mut len = 2;
    let mut stage = 0usize;

    while len <= n {
        let mut before = a.clone();
        let mut faulted = false;

        if apply_input_faults_for_stage(&mut a, q, stage, fault) {
            before = a.clone();
            faulted = true;
            if let Some(m) = &mut metrics {
                m.num_memory_reads += 1;
                m.num_memory_writes += 1;
            }
        }

        let mitigation_started = Instant::now();
        let w_len = pow_mod(root, (n / len) as u64, q);
        let stage_checksum_enabled = mitigation.kind == MitigationKind::StageChecksum;
        let mut expected_stage_sum = 0u64;
        let mut expected_stage_index_sum = 0u64;

        for start in (0..n).step_by(len) {
            let mut w = 1u64;
            for j in 0..len / 2 {
                let lo = start + j;
                let hi = start + j + len / 2;
                let u = a[lo];
                let b = a[hi];

                let wb_clean = mul_mod(b, w, q);
                let wb = inject_arithmetic_site_value(
                    wb_clean,
                    q,
                    stage,
                    hi,
                    fault,
                    FaultSite::MulOutput,
                );

                let expected_y0 = add_mod(u, wb_clean, q);
                let expected_y1 = sub_mod(u, wb_clean, q);

                let y0_clean = add_mod(u, wb, q);
                let mut y0 = inject_arithmetic_site_value(
                    y0_clean,
                    q,
                    stage,
                    lo,
                    fault,
                    FaultSite::AddOutput,
                );
                y0 = inject_arithmetic_site_value(
                    y0,
                    q,
                    stage,
                    lo,
                    fault,
                    FaultSite::ButterflyOutput,
                );
                y0 =
                    inject_arithmetic_site_value(y0, q, stage, lo, fault, FaultSite::RegisterWrite);

                let y1_clean = sub_mod(u, wb, q);
                let mut y1 = inject_arithmetic_site_value(
                    y1_clean,
                    q,
                    stage,
                    hi,
                    fault,
                    FaultSite::SubOutput,
                );
                y1 = inject_arithmetic_site_value(
                    y1,
                    q,
                    stage,
                    hi,
                    fault,
                    FaultSite::ButterflyOutput,
                );
                y1 =
                    inject_arithmetic_site_value(y1, q, stage, hi, fault, FaultSite::RegisterWrite);

                if y0 != expected_y0 || y1 != expected_y1 {
                    faulted = true;
                }

                if stage_checksum_enabled {
                    expected_stage_sum = add_mod(expected_stage_sum, add_mod(u, u, q), q);

                    if mitigation.checksum_mode == ChecksumMode::SumIndex {
                        let lo_idx = lo as u64;
                        let hi_idx = hi as u64;

                        let sum_idx = add_mod(lo_idx % q, hi_idx % q, q);
                        let diff_idx = sub_mod(lo_idx % q, hi_idx % q, q);

                        let lhs = mul_mod(sum_idx, u, q);
                        let rhs = mul_mod(diff_idx, wb_clean, q);

                        expected_stage_index_sum =
                            add_mod(expected_stage_index_sum, add_mod(lhs, rhs, q), q);
                    }
                }

                if mitigation.kind == MitigationKind::ButterflyCheck {
                    mitigation_metrics.checks_performed += 1;
                    let lhs_sum = add_mod(y0, y1, q);
                    let rhs_sum = add_mod(expected_y0, expected_y1, q);
                    let lhs_diff = sub_mod(y0, y1, q);
                    let rhs_diff = sub_mod(expected_y0, expected_y1, q);
                    let ok = lhs_sum == rhs_sum && lhs_diff == rhs_diff;

                    if !ok {
                        mitigation_metrics.check_failures += 1;
                        mitigation_metrics.fault_detected = true;

                        match mitigation.action {
                            MitigationAction::DetectOnly => {}
                            MitigationAction::Abort => {
                                // This backend returns a vector, not a Result, so abort mode
                                // records detection and leaves the faulty values in place.
                            }
                            MitigationAction::Recompute => {
                                let mut corrected = false;
                                for _ in 0..mitigation.max_retries.max(1) {
                                    let retry_wb = mul_mod(b, w, q);
                                    let retry_y0 = add_mod(u, retry_wb, q);
                                    let retry_y1 = sub_mod(u, retry_wb, q);
                                    let retry_sum = add_mod(retry_y0, retry_y1, q);
                                    let retry_diff = sub_mod(retry_y0, retry_y1, q);
                                    let retry_ok = retry_sum == rhs_sum && retry_diff == rhs_diff;
                                    mitigation_metrics.recomputations += 1;
                                    if retry_ok {
                                        y0 = retry_y0;
                                        y1 = retry_y1;
                                        mitigation_metrics.fault_corrected = true;
                                        corrected = true;
                                        break;
                                    }
                                }
                                if !corrected {
                                    // Keep original values if recomputation could not establish consistency.
                                }
                            }
                        }
                    }
                }

                a[lo] = y0;
                a[hi] = y1;
                w = mul_mod(w, w_len, q);

                if let Some(m) = &mut metrics {
                    m.num_mod_adds += 1;
                    m.num_mod_subs += 1;
                    m.num_mod_muls += 2;
                    m.num_twiddle_loads += 1;
                    m.num_memory_reads += 2;
                    m.num_memory_writes += 2;
                }
            }
        }

        if stage_checksum_enabled {
            mitigation_metrics.checks_performed += 1;
            mitigation_metrics.stage_checks_performed += 1;
            let actual_stage_sum = a.iter().fold(0u64, |acc, &x| add_mod(acc, x, q));
            let s1_ok = actual_stage_sum == expected_stage_sum;

            let s2_ok = if mitigation.checksum_mode == ChecksumMode::SumIndex {
                let actual_stage_index_sum = a.iter().enumerate().fold(0u64, |acc, (i, &x)| {
                    add_mod(acc, mul_mod(i as u64, x, q), q)
                });
                actual_stage_index_sum == expected_stage_index_sum
            } else {
                true
            };

            if !s1_ok || !s2_ok {
                mitigation_metrics.check_failures += 1;
                mitigation_metrics.stage_check_failures += 1;
                if !s1_ok {
                    mitigation_metrics.stage_checksum_s1_failures += 1;
                }
                if !s2_ok {
                    mitigation_metrics.stage_checksum_s2_failures += 1;
                }
                mitigation_metrics.fault_detected = true;

                match mitigation.action {
                    MitigationAction::DetectOnly | MitigationAction::Abort => {}
                    MitigationAction::Recompute => {
                        let mut recomputed = before.clone();
                        for _ in 0..mitigation.max_retries.max(1) {
                            for start in (0..n).step_by(len) {
                                let mut w = 1u64;
                                for j in 0..len / 2 {
                                    let lo = start + j;
                                    let hi = start + j + len / 2;
                                    let u = before[lo];
                                    let b = before[hi];
                                    let wb = mul_mod(b, w, q);
                                    recomputed[lo] = add_mod(u, wb, q);
                                    recomputed[hi] = sub_mod(u, wb, q);
                                    w = mul_mod(w, w_len, q);
                                }
                            }
                            mitigation_metrics.recomputations += 1;
                            let retry_sum =
                                recomputed.iter().fold(0u64, |acc, &x| add_mod(acc, x, q));
                            let retry_s1_ok = retry_sum == expected_stage_sum;
                            let retry_s2_ok = if mitigation.checksum_mode == ChecksumMode::SumIndex
                            {
                                let retry_index_sum =
                                    recomputed.iter().enumerate().fold(0u64, |acc, (i, &x)| {
                                        add_mod(acc, mul_mod(i as u64, x, q), q)
                                    });
                                retry_index_sum == expected_stage_index_sum
                            } else {
                                true
                            };

                            if retry_s1_ok && retry_s2_ok {
                                a = recomputed.clone();
                                mitigation_metrics.fault_corrected = true;
                                break;
                            }
                        }
                    }
                }
            }
        }

        mitigation_metrics.mitigation_elapsed_ns += mitigation_started.elapsed().as_nanos();

        if let Some(m) = &mut metrics {
            m.num_stage_barriers += 1;
        }

        if trace_enabled {
            traces.push(StageTrace {
                stage,
                input: before,
                output: a.clone(),
                faulted,
            });
        }

        stage += 1;
        len <<= 1;
    }

    if inverse {
        let n_inv = inv_mod(n as u64, q);
        for x in &mut a {
            *x = mul_mod(*x, n_inv, q);
            if let Some(m) = &mut metrics {
                m.num_mod_muls += 1;
                m.num_memory_reads += 1;
                m.num_memory_writes += 1;
            }
        }
    }

    if let Some(m) = &mut metrics {
        m.add_elapsed(started.elapsed().as_nanos());
    }

    (a, traces)
}

fn transform_dif_radix2(
    input: &[u64],
    params: &RingParams,
    inverse: bool,
    trace_enabled: bool,
    fault: Option<&FaultSpec>,
    mut metrics: Option<&mut NttSystemMetrics>,
) -> (Vec<u64>, Vec<StageTrace>) {
    let started = Instant::now();
    let n = params.n;
    let q = params.modulus;
    assert_eq!(input.len(), n);

    if let Some(m) = &mut metrics {
        m.record_layout(n, NttImplementation::DifRadix2);
        // Decimation-in-frequency keeps the input in natural order and bit-reverses the output.
        m.record_scratch(n * std::mem::size_of::<u64>());
        m.num_memory_reads += n as u64;
        m.num_memory_writes += n as u64;
    }

    let mut a = input.to_vec();
    let mut traces = Vec::new();

    let root = if inverse {
        inv_mod(params.primitive_n_root, q)
    } else {
        params.primitive_n_root
    };

    let mut len = n;
    let mut stage = 0usize;

    while len >= 2 {
        let mut before = a.clone();
        let mut faulted = false;

        if apply_input_faults_for_stage(&mut a, q, stage, fault) {
            before = a.clone();
            faulted = true;
            if let Some(m) = &mut metrics {
                m.num_memory_reads += 1;
                m.num_memory_writes += 1;
            }
        }

        let w_len = pow_mod(root, (n / len) as u64, q);

        for start in (0..n).step_by(len) {
            let mut w = 1u64;
            for j in 0..len / 2 {
                let u = a[start + j];
                let v = a[start + j + len / 2];

                a[start + j] = add_mod(u, v, q);
                let diff = sub_mod(u, v, q);
                a[start + j + len / 2] = mul_mod(diff, w, q);
                w = mul_mod(w, w_len, q);

                if let Some(m) = &mut metrics {
                    m.num_mod_adds += 1;
                    m.num_mod_subs += 1;
                    m.num_mod_muls += 2;
                    m.num_twiddle_loads += 1;
                    m.num_memory_reads += 2;
                    m.num_memory_writes += 2;
                }
            }
        }

        if let Some(m) = &mut metrics {
            m.num_stage_barriers += 1;
        }

        if trace_enabled {
            traces.push(StageTrace {
                stage,
                input: before,
                output: a.clone(),
                faulted,
            });
        }

        stage += 1;
        len >>= 1;
    }

    a = bit_reverse_copy(&a);
    if let Some(m) = &mut metrics {
        m.num_memory_reads += n as u64;
        m.num_memory_writes += n as u64;
    }

    if inverse {
        let n_inv = inv_mod(n as u64, q);
        for x in &mut a {
            *x = mul_mod(*x, n_inv, q);
            if let Some(m) = &mut metrics {
                m.num_mod_muls += 1;
                m.num_memory_reads += 1;
                m.num_memory_writes += 1;
            }
        }
    }

    if let Some(m) = &mut metrics {
        m.add_elapsed(started.elapsed().as_nanos());
    }

    (a, traces)
}

fn transform_stockham(
    input: &[u64],
    params: &RingParams,
    inverse: bool,
    trace_enabled: bool,
    fault: Option<&FaultSpec>,
    mut metrics: Option<&mut NttSystemMetrics>,
) -> (Vec<u64>, Vec<StageTrace>) {
    let started = Instant::now();
    let n = params.n;
    let q = params.modulus;
    assert_eq!(input.len(), n);

    if let Some(m) = &mut metrics {
        m.record_layout(n, NttImplementation::Stockham);
        // Stockham-style ping-pong radix-2 backend uses two full buffers.
        m.record_scratch(2 * n * std::mem::size_of::<u64>());
        m.num_memory_reads += n as u64;
        m.num_memory_writes += n as u64;
    }

    // We start from bit-reversed input, matching the baseline DIT radix-2
    // arithmetic schedule, but perform each stage out-of-place using ping-pong
    // buffers. This intentionally changes the memory footprint/access pattern
    // while preserving the same stage numbering and fault coordinates as radix2.
    let mut src = bit_reverse_copy(input);
    let mut dst = src.clone();
    let mut traces = Vec::new();

    let root = if inverse {
        inv_mod(params.primitive_n_root, q)
    } else {
        params.primitive_n_root
    };

    let mut len = 2;
    let mut stage = 0usize;

    while len <= n {
        let mut before = src.clone();
        let mut faulted = false;

        if let Some(spec) = fault {
            if spec.stage == stage {
                let _ = inject_bit_fault(&mut src, spec.slot, spec.bit, params.modulus_bits, q);
                before = src.clone();
                faulted = true;
                if let Some(m) = &mut metrics {
                    m.num_memory_reads += 1;
                    m.num_memory_writes += 1;
                }
            }
        }

        let w_len = pow_mod(root, (n / len) as u64, q);

        for start in (0..n).step_by(len) {
            let mut w = 1u64;
            for j in 0..len / 2 {
                let left = start + j;
                let right = start + j + len / 2;
                let u = src[left];
                let v = mul_mod(src[right], w, q);
                dst[left] = add_mod(u, v, q);
                dst[right] = sub_mod(u, v, q);
                w = mul_mod(w, w_len, q);

                if let Some(m) = &mut metrics {
                    m.num_mod_adds += 1;
                    m.num_mod_subs += 1;
                    m.num_mod_muls += 2;
                    m.num_twiddle_loads += 1;
                    m.num_memory_reads += 2;
                    m.num_memory_writes += 2;
                }
            }
        }

        if let Some(m) = &mut metrics {
            m.num_stage_barriers += 1;
            m.num_passes += 1;
            m.num_buffer_swaps += 1;
        }

        if trace_enabled {
            traces.push(StageTrace {
                stage,
                input: before,
                output: dst.clone(),
                faulted,
            });
        }

        std::mem::swap(&mut src, &mut dst);
        stage += 1;
        len <<= 1;
    }

    if inverse {
        let n_inv = inv_mod(n as u64, q);
        for x in &mut src {
            *x = mul_mod(*x, n_inv, q);
            if let Some(m) = &mut metrics {
                m.num_mod_muls += 1;
                m.num_memory_reads += 1;
                m.num_memory_writes += 1;
            }
        }
    }

    if let Some(m) = &mut metrics {
        m.add_elapsed(started.elapsed().as_nanos());
    }

    src.truncate(n);
    (src, traces)
}

fn radix2_stage_in_place(
    a: &mut [u64],
    n: usize,
    q: u64,
    root: u64,
    len: usize,
    metrics: &mut Option<&mut NttSystemMetrics>,
) {
    let w_len = pow_mod(root, (n / len) as u64, q);

    for start in (0..n).step_by(len) {
        let mut w = 1u64;
        for j in 0..len / 2 {
            let u = a[start + j];
            let v = mul_mod(a[start + j + len / 2], w, q);
            a[start + j] = add_mod(u, v, q);
            a[start + j + len / 2] = sub_mod(u, v, q);
            w = mul_mod(w, w_len, q);

            if let Some(m) = metrics {
                m.num_mod_adds += 1;
                m.num_mod_subs += 1;
                m.num_mod_muls += 2;
                m.num_twiddle_loads += 1;
                m.num_memory_reads += 2;
                m.num_memory_writes += 2;
            }
        }
    }
}

/// Experimental fused radix-4 backend.
///
/// This backend groups pairs of radix-2 butterfly stages into one logical
/// radix-4 pass. It preserves the exact arithmetic result of the baseline DIT
/// radix-2 transform while exposing a different stage/pass structure and
/// metrics profile. For powers of two with an odd number of radix-2 stages, the
/// final pass contains a single radix-2 stage, making the backend a mixed
/// radix-4/radix-2 schedule.
///
/// Fault coordinates use logical fused passes:
/// pass 0 injects before the first grouped stage pair, pass 1 before the next
/// pair, and so on.
fn transform_radix4_fused(
    input: &[u64],
    params: &RingParams,
    inverse: bool,
    trace_enabled: bool,
    fault: Option<&FaultSpec>,
    mut metrics: Option<&mut NttSystemMetrics>,
) -> (Vec<u64>, Vec<StageTrace>) {
    let started = Instant::now();
    let n = params.n;
    let q = params.modulus;
    assert_eq!(input.len(), n);

    if let Some(m) = &mut metrics {
        m.record_layout(n, NttImplementation::Radix4);
        m.record_scratch(n * std::mem::size_of::<u64>());
        m.num_memory_reads += n as u64;
        m.num_memory_writes += n as u64;
    }

    let mut a = bit_reverse_copy(input);
    let mut traces = Vec::new();

    let root = if inverse {
        inv_mod(params.primitive_n_root, q)
    } else {
        params.primitive_n_root
    };

    let mut len = 2usize;
    let mut pass = 0usize;

    while len <= n {
        let mut before = a.clone();
        let mut faulted = false;

        if apply_input_faults_for_stage(&mut a, q, pass, fault) {
            before = a.clone();
            faulted = true;
            if let Some(m) = &mut metrics {
                m.num_memory_reads += 1;
                m.num_memory_writes += 1;
            }
        }

        radix2_stage_in_place(&mut a, n, q, root, len, &mut metrics);
        len <<= 1;

        if len <= n {
            radix2_stage_in_place(&mut a, n, q, root, len, &mut metrics);
            len <<= 1;
        }

        if let Some(m) = &mut metrics {
            m.num_stage_barriers += 1;
            m.num_passes += 1;
        }

        if trace_enabled {
            traces.push(StageTrace {
                stage: pass,
                input: before,
                output: a.clone(),
                faulted,
            });
        }

        pass += 1;
    }

    if inverse {
        let n_inv = inv_mod(n as u64, q);
        for x in &mut a {
            *x = mul_mod(*x, n_inv, q);
            if let Some(m) = &mut metrics {
                m.num_mod_muls += 1;
                m.num_memory_reads += 1;
                m.num_memory_writes += 1;
            }
        }
    }

    if let Some(m) = &mut metrics {
        m.add_elapsed(started.elapsed().as_nanos());
    }

    (a, traces)
}

/// Experimental four-step / blocked NTT backend.
///
/// This backend preserves the same arithmetic result as the baseline radix-2
/// DIT transform, but processes butterflies in cache-sized blocks. It is not a
/// target-specific implementation; rather, it gives the fault-analysis
/// framework a backend with explicitly modeled blocking behavior, block
/// counters, and block-sized scratch estimates. The stage numbering remains the
/// ordinary radix-2 stage numbering, so existing fault coordinates continue to
/// mean "inject before radix-2 stage s".
fn transform_four_step_blocked(
    input: &[u64],
    params: &RingParams,
    inverse: bool,
    trace_enabled: bool,
    fault: Option<&FaultSpec>,
    mut metrics: Option<&mut NttSystemMetrics>,
) -> (Vec<u64>, Vec<StageTrace>) {
    let started = Instant::now();
    let n = params.n;
    let q = params.modulus;
    assert_eq!(input.len(), n);

    // A conservative, hardware-agnostic block size. It is intentionally small
    // enough to expose block behavior in experiments while remaining valid for
    // all powers-of-two N used by the test suite.
    let block_elems = n.clamp(2, 256);
    let block_bytes = block_elems * std::mem::size_of::<u64>();

    if let Some(m) = &mut metrics {
        m.record_layout(n, NttImplementation::FourStep);
        m.record_scratch(block_bytes);
        m.block_bytes = m.block_bytes.max(block_bytes);
        m.num_memory_reads += n as u64;
        m.num_memory_writes += n as u64;
    }

    let mut a = bit_reverse_copy(input);
    let mut traces = Vec::new();

    let root = if inverse {
        inv_mod(params.primitive_n_root, q)
    } else {
        params.primitive_n_root
    };

    let mut len = 2usize;
    let mut stage = 0usize;

    while len <= n {
        let mut before = a.clone();
        let mut faulted = false;

        if apply_input_faults_for_stage(&mut a, q, stage, fault) {
            before = a.clone();
            faulted = true;
            if let Some(m) = &mut metrics {
                m.num_memory_reads += 1;
                m.num_memory_writes += 1;
            }
        }

        let w_len = pow_mod(root, (n / len) as u64, q);

        // Process groups of independent butterflies in contiguous blocks. This
        // models the blocked/four-step style of improving locality without
        // changing the mathematical transform.
        let mut block_start = 0usize;
        while block_start < n {
            let block_end = (block_start + block_elems).min(n);

            let first_group = block_start / len;
            let last_group = block_end.div_ceil(len);

            for group in first_group..last_group {
                let start = group * len;
                if start >= n {
                    continue;
                }

                let mut w = 1u64;
                for j in 0..len / 2 {
                    let left = start + j;
                    let right = start + j + len / 2;
                    if right >= n {
                        continue;
                    }

                    // Only count/process each butterfly once. The block
                    // boundary is used to group the left element of the
                    // butterfly into a block.
                    if left < block_start || left >= block_end {
                        w = mul_mod(w, w_len, q);
                        continue;
                    }

                    let u = a[left];
                    let v = mul_mod(a[right], w, q);
                    a[left] = add_mod(u, v, q);
                    a[right] = sub_mod(u, v, q);
                    w = mul_mod(w, w_len, q);

                    if let Some(m) = &mut metrics {
                        m.num_mod_adds += 1;
                        m.num_mod_subs += 1;
                        m.num_mod_muls += 2;
                        m.num_twiddle_loads += 1;
                        m.num_memory_reads += 2;
                        m.num_memory_writes += 2;
                    }
                }
            }

            if let Some(m) = &mut metrics {
                m.num_blocks += 1;
            }

            block_start += block_elems;
        }

        if let Some(m) = &mut metrics {
            m.num_stage_barriers += 1;
            m.num_passes += 1;
        }

        if trace_enabled {
            traces.push(StageTrace {
                stage,
                input: before,
                output: a.clone(),
                faulted,
            });
        }

        stage += 1;
        len <<= 1;
    }

    if inverse {
        let n_inv = inv_mod(n as u64, q);
        for x in &mut a {
            *x = mul_mod(*x, n_inv, q);
            if let Some(m) = &mut metrics {
                m.num_mod_muls += 1;
                m.num_memory_reads += 1;
                m.num_memory_writes += 1;
            }
        }
    }

    if let Some(m) = &mut metrics {
        m.add_elapsed(started.elapsed().as_nanos());
    }

    (a, traces)
}

fn transform_naive(
    input: &[u64],
    params: &RingParams,
    inverse: bool,
    trace_enabled: bool,
    fault: Option<&FaultSpec>,
    mut metrics: Option<&mut NttSystemMetrics>,
) -> Result<(Vec<u64>, Vec<StageTrace>), String> {
    let started = Instant::now();
    let n = params.n;
    let q = params.modulus;
    if input.len() != n {
        return Err(format!("expected {} coefficients, got {}", n, input.len()));
    }

    if let Some(spec) = fault {
        if spec.stage != 0 {
            return Err("naive NTT backend supports fault_stage=0 only because it has one logical transform stage".to_string());
        }
    }

    if let Some(m) = &mut metrics {
        m.record_layout(n, NttImplementation::Naive);
        m.record_scratch(0);
        m.num_memory_reads += n as u64;
    }

    let mut source = input.to_vec();
    let mut faulted = false;
    if let Some(spec) = fault {
        inject_bit_fault(&mut source, spec.slot, spec.bit, params.modulus_bits, q)?;
        faulted = true;
        if let Some(m) = &mut metrics {
            m.num_memory_reads += 1;
            m.num_memory_writes += 1;
        }
    }

    let before = source.clone();
    let root = if inverse {
        inv_mod(params.primitive_n_root, q)
    } else {
        params.primitive_n_root
    };

    let mut out = vec![0u64; n];
    for k in 0..n {
        let mut acc = 0u64;
        for j in 0..n {
            let exponent = ((j * k) % n) as u64;
            let w = pow_mod(root, exponent, q);
            let term = mul_mod(source[j], w, q);
            acc = add_mod(acc, term, q);
            if let Some(m) = &mut metrics {
                m.num_mod_muls += 1;
                m.num_mod_adds += 1;
                m.num_twiddle_loads += 1;
                m.num_memory_reads += 1;
            }
        }
        out[k] = acc;
        if let Some(m) = &mut metrics {
            m.num_memory_writes += 1;
        }
    }

    if inverse {
        let n_inv = inv_mod(n as u64, q);
        for x in &mut out {
            *x = mul_mod(*x, n_inv, q);
            if let Some(m) = &mut metrics {
                m.num_mod_muls += 1;
                m.num_memory_reads += 1;
                m.num_memory_writes += 1;
            }
        }
    }

    let traces = if trace_enabled {
        vec![StageTrace {
            stage: 0,
            input: before,
            output: out.clone(),
            faulted,
        }]
    } else {
        Vec::new()
    };

    if let Some(m) = &mut metrics {
        m.num_stage_barriers += 1;
        m.add_elapsed(started.elapsed().as_nanos());
    }

    Ok((out, traces))
}

fn bit_reverse_copy(input: &[u64]) -> Vec<u64> {
    let n = input.len();
    let bits = n.trailing_zeros();
    let mut out = vec![0u64; n];
    for i in 0..n {
        let r = i.reverse_bits() >> (usize::BITS - bits);
        out[r] = input[i];
    }
    out
}

/// Pointwise modular addition in the NTT domain.
#[allow(dead_code)]
pub fn add_ntt(a: &[u64], b: &[u64], q: u64) -> Vec<u64> {
    a.iter().zip(b).map(|(&x, &y)| add_mod(x, y, q)).collect()
}

pub fn mul_ntt(a: &[u64], b: &[u64], q: u64) -> Vec<u64> {
    a.iter().zip(b).map(|(&x, &y)| mul_mod(x, y, q)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn naive_matches_radix2_ntt_and_intt() {
        let params = RingParams::new(16, 24).expect("valid params");
        let input: Vec<u64> = (0..params.n)
            .map(|i| ((i * 17 + 3) as u64) % params.modulus)
            .collect();

        let (r_ntt, _) =
            ntt_with_impl(&input, &params, false, NttImplementation::Radix2, None).unwrap();
        let (d_ntt, _) =
            ntt_with_impl(&input, &params, false, NttImplementation::DifRadix2, None).unwrap();
        let (s_ntt, _) =
            ntt_with_impl(&input, &params, false, NttImplementation::Stockham, None).unwrap();
        let (r4_ntt, _) =
            ntt_with_impl(&input, &params, false, NttImplementation::Radix4, None).unwrap();
        let (fs_ntt, _) =
            ntt_with_impl(&input, &params, false, NttImplementation::FourStep, None).unwrap();
        let (n_ntt, _) =
            ntt_with_impl(&input, &params, false, NttImplementation::Naive, None).unwrap();
        assert_eq!(r_ntt, d_ntt);
        assert_eq!(r_ntt, s_ntt);
        assert_eq!(r_ntt, r4_ntt);
        assert_eq!(r_ntt, fs_ntt);
        assert_eq!(r_ntt, n_ntt);

        let (r_back, _) =
            intt_with_impl(&r_ntt, &params, false, NttImplementation::Radix2, None).unwrap();
        let (d_back, _) =
            intt_with_impl(&d_ntt, &params, false, NttImplementation::DifRadix2, None).unwrap();
        let (s_back, _) =
            intt_with_impl(&s_ntt, &params, false, NttImplementation::Stockham, None).unwrap();
        let (r4_back, _) =
            intt_with_impl(&r4_ntt, &params, false, NttImplementation::Radix4, None).unwrap();
        let (fs_back, _) =
            intt_with_impl(&fs_ntt, &params, false, NttImplementation::FourStep, None).unwrap();
        let (n_back, _) =
            intt_with_impl(&n_ntt, &params, false, NttImplementation::Naive, None).unwrap();
        assert_eq!(r_back, input);
        assert_eq!(d_back, input);
        assert_eq!(s_back, input);
        assert_eq!(r4_back, input);
        assert_eq!(fs_back, input);
        assert_eq!(n_back, input);
    }

    #[test]
    fn metrics_are_recorded_for_radix2() {
        let params = RingParams::new(16, 24).expect("valid params");
        let input: Vec<u64> = (0..params.n).map(|i| i as u64).collect();
        let mut metrics = NttSystemMetrics::default();
        let _ = ntt_with_impl(
            &input,
            &params,
            false,
            NttImplementation::Radix2,
            Some(&mut metrics),
        )
        .unwrap();
        assert!(metrics.elapsed_ns > 0);
        assert!(metrics.num_mod_muls > 0);
        assert!(metrics.num_memory_reads > 0);
        assert_eq!(metrics.ntt_impl, Some(NttImplementation::Radix2));
    }

    #[test]
    fn metrics_are_recorded_for_dif_radix2() {
        let params = RingParams::new(16, 24).expect("valid params");
        let input: Vec<u64> = (0..params.n).map(|i| i as u64).collect();
        let mut metrics = NttSystemMetrics::default();
        let _ = ntt_with_impl(
            &input,
            &params,
            false,
            NttImplementation::DifRadix2,
            Some(&mut metrics),
        )
        .unwrap();
        assert!(metrics.elapsed_ns > 0);
        assert!(metrics.num_mod_muls > 0);
        assert!(metrics.num_memory_reads > 0);
        assert_eq!(metrics.ntt_impl, Some(NttImplementation::DifRadix2));
    }
    #[test]
    fn metrics_are_recorded_for_stockham() {
        let params = RingParams::new(16, 24).expect("valid params");
        let input: Vec<u64> = (0..params.n).map(|i| i as u64).collect();
        let mut metrics = NttSystemMetrics::default();
        let _ = ntt_with_impl(
            &input,
            &params,
            false,
            NttImplementation::Stockham,
            Some(&mut metrics),
        )
        .unwrap();
        assert!(metrics.elapsed_ns > 0);
        assert!(metrics.num_mod_muls > 0);
        assert!(metrics.num_memory_reads > 0);
        assert!(metrics.scratch_bytes >= 2 * params.n * std::mem::size_of::<u64>());
        assert!(metrics.num_buffer_swaps > 0);
        assert_eq!(metrics.ntt_impl, Some(NttImplementation::Stockham));
    }

    #[test]
    fn metrics_are_recorded_for_radix4() {
        let params = RingParams::new(16, 24).expect("valid params");
        let input: Vec<u64> = (0..params.n).map(|i| i as u64).collect();
        let mut metrics = NttSystemMetrics::default();
        let _ = ntt_with_impl(
            &input,
            &params,
            false,
            NttImplementation::Radix4,
            Some(&mut metrics),
        )
        .unwrap();
        assert!(metrics.elapsed_ns > 0);
        assert!(metrics.num_mod_muls > 0);
        assert!(metrics.num_memory_reads > 0);
        assert!(metrics.num_passes > 0);
        assert_eq!(metrics.ntt_impl, Some(NttImplementation::Radix4));
    }

    #[test]
    fn metrics_are_recorded_for_four_step() {
        let params = RingParams::new(16, 24).expect("valid params");
        let input: Vec<u64> = (0..params.n).map(|i| i as u64).collect();
        let mut metrics = NttSystemMetrics::default();
        let _ = ntt_with_impl(
            &input,
            &params,
            false,
            NttImplementation::FourStep,
            Some(&mut metrics),
        )
        .unwrap();
        assert!(metrics.elapsed_ns > 0);
        assert!(metrics.num_mod_muls > 0);
        assert!(metrics.num_memory_reads > 0);
        assert!(metrics.num_blocks > 0);
        assert!(metrics.block_bytes > 0);
        assert_eq!(metrics.ntt_impl, Some(NttImplementation::FourStep));
    }

    #[test]
    fn radix4_fault_injection_changes_output() {
        let params = RingParams::new(16, 24).expect("valid params");
        let input: Vec<u64> = (0..params.n)
            .map(|i| ((i * 31 + 9) as u64) % params.modulus)
            .collect();
        let fault = FaultSpec {
            operand: crate::fault::FaultOperand::A,
            stage: 0,
            slot: 0,
            bit: 3,
            site: crate::fault::FaultSite::Input,
            adjacent: false,
            second_enabled: false,
            second_stage: 0,
            second_slot: 0,
            second_bit: 0,
            second_site: crate::fault::FaultSite::Input,
            second_adjacent: false,
        };

        let (golden, _) =
            ntt_with_impl(&input, &params, false, NttImplementation::Radix4, None).unwrap();
        let (faulty, trace) = ntt_faulty_with_impl(
            &input,
            &params,
            true,
            &fault,
            NttImplementation::Radix4,
            None,
        )
        .unwrap();

        assert_ne!(golden, faulty);
        assert!(!trace.is_empty());
        assert!(trace[0].faulted);
    }

    #[test]
    fn stage_checksum_mitigation_records_stage_checks() {
        let params = RingParams::new(16, 24).expect("valid params");
        let input: Vec<u64> = (0..params.n)
            .map(|i| ((i * 13 + 5) as u64) % params.modulus)
            .collect();
        let mitigation = MitigationOptions {
            kind: MitigationKind::StageChecksum,
            action: MitigationAction::DetectOnly,
            max_retries: 1,
            checksum_mode: crate::mitigation::ChecksumMode::Sum,
        };
        let mut mitigation_metrics = MitigationMetrics::default();
        let mut system_metrics = NttSystemMetrics::default();

        let (protected, _) = ntt_with_impl_and_mitigation(
            &input,
            &params,
            false,
            NttImplementation::Radix2,
            Some(&mut system_metrics),
            &mitigation,
            &mut mitigation_metrics,
            None,
        )
        .unwrap();

        let (golden, _) =
            ntt_with_impl(&input, &params, false, NttImplementation::Radix2, None).unwrap();
        assert_eq!(protected, golden);
        assert!(mitigation_metrics.checks_performed > 0);
        assert!(mitigation_metrics.stage_checks_performed > 0);
        assert_eq!(mitigation_metrics.check_failures, 0);
    }

    #[test]
    fn stage_checksum_sum_index_mode_records_no_failures_without_fault() {
        let params = RingParams::new(16, 24).expect("valid params");
        let input: Vec<u64> = (0..params.n)
            .map(|i| ((i * 17 + 11) as u64) % params.modulus)
            .collect();
        let mitigation = MitigationOptions {
            kind: MitigationKind::StageChecksum,
            action: MitigationAction::DetectOnly,
            max_retries: 1,
            checksum_mode: crate::mitigation::ChecksumMode::SumIndex,
        };
        let mut mitigation_metrics = MitigationMetrics::default();
        let mut system_metrics = NttSystemMetrics::default();

        let (protected, _) = ntt_with_impl_and_mitigation(
            &input,
            &params,
            false,
            NttImplementation::Radix2,
            Some(&mut system_metrics),
            &mitigation,
            &mut mitigation_metrics,
            None,
        )
        .unwrap();

        let (golden, _) =
            ntt_with_impl(&input, &params, false, NttImplementation::Radix2, None).unwrap();
        assert_eq!(protected, golden);
        assert!(mitigation_metrics.stage_checks_performed > 0);
        assert_eq!(mitigation_metrics.check_failures, 0);
        assert_eq!(mitigation_metrics.stage_checksum_s1_failures, 0);
        assert_eq!(mitigation_metrics.stage_checksum_s2_failures, 0);
    }
}

/*
ARITHMETIC FAULT MODEL TODO

Wire `FaultSpec.site` inside each backend butterfly loop.

Semantics:
- Input: existing pre-stage/pre-operation bit flip.
- MulOutput: flip bit after `v = w * b mod q`.
- AddOutput: flip bit after `y0 = a + v mod q`.
- SubOutput: flip bit after `y1 = a - v mod q`.
- ButterflyOutput: flip bit in selected butterfly output before mitigation check.
- RegisterWrite: flip bit after writing selected output slot to memory.

Mitigation expectations:
- butterfly-check should detect arithmetic/output faults.
- stage-checksum should detect post-stage output/register-write faults.
- neither should necessarily detect input faults.
*/
