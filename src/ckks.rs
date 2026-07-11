#![allow(clippy::too_many_arguments)]
#![allow(clippy::needless_range_loop)]
#![allow(clippy::if_same_then_else)]

use num_complex::Complex64;
use std::f64::consts::PI;

use crate::fault::{inject_bit_fault, FaultSpec};
use crate::metrics::{decoded_metrics, DecodedMetrics};
use crate::mitigation::{MitigationMetrics, MitigationOptions};
use crate::modarith::{centered, from_centered, mul_mod};
use crate::ntt::{mul_ntt, NttImplementation, NttSystemMetrics, StageTrace};
use crate::params::RingParams;

#[derive(Debug, Clone)]
pub struct CkksToyContext {
    pub params: RingParams,
    pub scale: f64,
    pub ntt_impl: NttImplementation,
    pub mitigation: MitigationOptions,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct CkksTraceOptions {
    pub encode: bool,
    pub ntt: bool,
    pub mul: bool,
    pub intt: bool,
    pub decode: bool,
}

impl CkksTraceOptions {
    pub fn all() -> Self {
        Self {
            encode: true,
            ntt: true,
            mul: true,
            intt: true,
            decode: true,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct CkksExecutionTrace {
    pub encoded_a: Option<Vec<u64>>,
    pub encoded_b: Option<Vec<u64>>,
    pub twisted_a: Option<Vec<u64>>,
    pub twisted_b: Option<Vec<u64>>,
    pub correct_ntt_a: Option<Vec<u64>>,
    pub correct_ntt_b: Option<Vec<u64>>,
    pub faulty_ntt_a: Option<Vec<u64>>,
    pub faulty_ntt_b: Option<Vec<u64>>,
    pub correct_ntt_stages: Vec<StageTrace>,
    pub faulty_ntt_stages: Vec<StageTrace>,
    pub correct_mul_ntt: Option<Vec<u64>>,
    pub faulty_mul_ntt: Option<Vec<u64>>,
    pub correct_intt_stages: Vec<StageTrace>,
    pub faulty_intt_stages: Vec<StageTrace>,
    pub correct_coeffs: Option<Vec<u64>>,
    pub faulty_coeffs: Option<Vec<u64>>,
    pub decoded_correct: Option<Vec<Complex64>>,
    pub decoded_faulty: Option<Vec<Complex64>>,
}

#[derive(Debug, Clone)]
pub struct CkksDemoResult {
    pub input_a: Vec<Complex64>,
    pub input_b: Vec<Complex64>,
    pub decoded_correct: Vec<Complex64>,
    pub decoded_faulty: Vec<Complex64>,
    pub metrics: DecodedMetrics,
    pub system_metrics: NttSystemMetrics,
    pub mitigation_metrics: MitigationMetrics,
    pub trace: CkksExecutionTrace,
}

impl CkksToyContext {
    pub fn new_with_impl(params: RingParams, scale_bits: u32, ntt_impl: NttImplementation) -> Self {
        Self {
            params,
            scale: 2f64.powi(scale_bits as i32),
            ntt_impl,
            mitigation: MitigationOptions::disabled(),
        }
    }

    pub fn with_mitigation(mut self, mitigation: MitigationOptions) -> Self {
        self.mitigation = mitigation;
        self
    }

    pub fn slot_count(&self) -> usize {
        self.params.n / 2
    }

    /// Toy CKKS-like encoder.
    ///
    /// This uses a conjugate-symmetric inverse DFT so decoded values can be observed
    /// in a CKKS-like complex slot domain. It is intentionally not a production CKKS
    /// canonical embedding implementation.
    pub fn encode(&self, slots: &[Complex64]) -> Result<Vec<u64>, String> {
        let n = self.params.n;
        let half = n / 2;
        if slots.len() != half {
            return Err(format!("expected {} slots, got {}", half, slots.len()));
        }

        let mut spectrum = vec![Complex64::new(0.0, 0.0); n];
        for k in 0..half {
            spectrum[k] = slots[k];
            spectrum[n - 1 - k] = slots[k].conj();
        }

        let mut coeffs = vec![0u64; n];
        for j in 0..n {
            let mut sum = Complex64::new(0.0, 0.0);
            for k in 0..n {
                let angle = -2.0 * PI * (j as f64) * (k as f64) / (n as f64);
                let w = Complex64::from_polar(1.0, angle);
                sum += spectrum[k] * w;
            }
            let real_coeff = sum.re / n as f64;
            let scaled = (real_coeff * self.scale).round() as i128;
            coeffs[j] = from_centered(scaled, self.params.modulus);
        }
        Ok(coeffs)
    }

    pub fn decode_with_scale(
        &self,
        coeffs_mod_q: &[u64],
        scale: f64,
    ) -> Result<Vec<Complex64>, String> {
        let n = self.params.n;
        let half = n / 2;
        if coeffs_mod_q.len() != n {
            return Err(format!(
                "expected {} coefficients, got {}",
                n,
                coeffs_mod_q.len()
            ));
        }

        let coeffs: Vec<f64> = coeffs_mod_q
            .iter()
            .map(|&x| centered(x, self.params.modulus) as f64 / scale)
            .collect();

        let mut slots = vec![Complex64::new(0.0, 0.0); half];
        for k in 0..half {
            let mut sum = Complex64::new(0.0, 0.0);
            for j in 0..n {
                let angle = 2.0 * PI * (j as f64) * (k as f64) / (n as f64);
                let w = Complex64::from_polar(1.0, angle);
                sum += w * coeffs[j];
            }
            slots[k] = sum;
        }
        Ok(slots)
    }

    /// Decode a single toy CKKS plaintext at the context scale.
    ///
    /// The multiplication demo decodes at `scale^2`; this helper is retained for
    /// standalone encode/decode experiments and library-style use.
    #[allow(dead_code)]
    pub fn decode(&self, coeffs_mod_q: &[u64]) -> Result<Vec<Complex64>, String> {
        self.decode_with_scale(coeffs_mod_q, self.scale)
    }

    pub fn multiply_with_optional_fault(
        &self,
        a_slots: &[Complex64],
        b_slots: &[Complex64],
        fault_op: Option<&str>,
        fault: Option<&FaultSpec>,
    ) -> Result<CkksDemoResult, String> {
        self.multiply_with_optional_fault_traced(
            a_slots,
            b_slots,
            fault_op,
            fault,
            CkksTraceOptions::default(),
        )
    }

    /// Encode two slot vectors, perform polynomial multiplication using the negacyclic
    /// NTT pipeline, decode the correct and faulty coefficient outputs, and compute
    /// decoded-domain metrics. Optional trace flags preserve intermediate values for
    /// validation and visualization.
    pub fn multiply_with_optional_fault_traced(
        &self,
        a_slots: &[Complex64],
        b_slots: &[Complex64],
        fault_op: Option<&str>,
        fault: Option<&FaultSpec>,
        trace_options: CkksTraceOptions,
    ) -> Result<CkksDemoResult, String> {
        let a_poly = self.encode(a_slots)?;
        let b_poly = self.encode(b_slots)?;

        let mut system_metrics = NttSystemMetrics::default();
        let mut mitigation_metrics = MitigationMetrics::default();
        mitigation_metrics.configure(&self.mitigation);
        let correct_run = self.negacyclic_mul_pipeline(
            &a_poly,
            &b_poly,
            None,
            None,
            trace_options,
            &mut system_metrics,
            &mut mitigation_metrics,
        )?;
        let faulty_run = self.negacyclic_mul_pipeline(
            &a_poly,
            &b_poly,
            fault_op,
            fault,
            trace_options,
            &mut system_metrics,
            &mut mitigation_metrics,
        )?;

        // Multiplication yields scale^2.
        let decoded_correct =
            self.decode_with_scale(&correct_run.coeffs, self.scale * self.scale)?;
        let decoded_faulty = self.decode_with_scale(&faulty_run.coeffs, self.scale * self.scale)?;
        let metrics = decoded_metrics(&decoded_correct, &decoded_faulty);

        let mut trace = CkksExecutionTrace::default();
        if trace_options.encode {
            trace.encoded_a = Some(a_poly.clone());
            trace.encoded_b = Some(b_poly.clone());
            trace.twisted_a = correct_run.twisted_a.clone();
            trace.twisted_b = correct_run.twisted_b.clone();
        }
        let fault_enabled = fault_op.is_some() && fault.is_some();

        if trace_options.ntt {
            trace.correct_ntt_a = correct_run.ntt_a.clone();
            trace.correct_ntt_b = correct_run.ntt_b.clone();
            if fault_enabled {
                trace.faulty_ntt_a = faulty_run.ntt_a.clone();
                trace.faulty_ntt_b = faulty_run.ntt_b.clone();
            }
            trace.correct_ntt_stages = correct_run.ntt_stages.clone();
            if fault_enabled {
                trace.faulty_ntt_stages = faulty_run.ntt_stages.clone();
            }
        }
        if trace_options.mul {
            trace.correct_mul_ntt = correct_run.mul_ntt.clone();
            if fault_enabled {
                trace.faulty_mul_ntt = faulty_run.mul_ntt.clone();
            }
        }
        if trace_options.intt {
            trace.correct_intt_stages = correct_run.intt_stages.clone();
            if fault_enabled {
                trace.faulty_intt_stages = faulty_run.intt_stages.clone();
            }
            trace.correct_coeffs = Some(correct_run.coeffs.clone());
            if fault_enabled {
                trace.faulty_coeffs = Some(faulty_run.coeffs.clone());
            }
        }
        if trace_options.decode {
            trace.decoded_correct = Some(decoded_correct.clone());
            if fault_enabled {
                trace.decoded_faulty = Some(decoded_faulty.clone());
            }
        }

        Ok(CkksDemoResult {
            input_a: a_slots.to_vec(),
            input_b: b_slots.to_vec(),
            decoded_correct,
            decoded_faulty,
            metrics,
            system_metrics,
            mitigation_metrics,
            trace,
        })
    }

    fn negacyclic_mul_pipeline(
        &self,
        a: &[u64],
        b: &[u64],
        fault_op: Option<&str>,
        fault: Option<&FaultSpec>,
        trace_options: CkksTraceOptions,
        system_metrics: &mut NttSystemMetrics,
        mitigation_metrics: &mut MitigationMetrics,
    ) -> Result<PipelineRun, String> {
        let n = self.params.n;
        let q = self.params.modulus;
        let psi = self.params.primitive_2n_root;
        let psi_inv = crate::modarith::inv_mod(psi, q);

        let mut ta = vec![0u64; n];
        let mut tb = vec![0u64; n];

        for i in 0..n {
            ta[i] = mul_mod(a[i], crate::modarith::pow_mod(psi, i as u64, q), q);
            tb[i] = mul_mod(b[i], crate::modarith::pow_mod(psi, i as u64, q), q);
        }

        let ntt_trace_enabled = trace_options.ntt;
        let intt_trace_enabled = trace_options.intt;

        let fault_spec = fault
            .ok_or_else(|| "internal fault state missing".to_string())
            .ok();

        let (mut a_hat, ntt_stages) = if fault_op == Some("ntt")
            && fault_spec.is_some()
            && fault_spec.unwrap().operand == crate::fault::FaultOperand::A
        {
            crate::ntt::ntt_with_impl_and_mitigation(
                &ta,
                &self.params,
                ntt_trace_enabled,
                self.ntt_impl,
                Some(&mut *system_metrics),
                &self.mitigation,
                mitigation_metrics,
                fault_spec,
            )?
        } else {
            crate::ntt::ntt_with_impl_and_mitigation(
                &ta,
                &self.params,
                ntt_trace_enabled,
                self.ntt_impl,
                Some(&mut *system_metrics),
                &self.mitigation,
                mitigation_metrics,
                fault_spec,
            )?
        };

        let (mut b_hat, b_stages) = if fault_op == Some("ntt")
            && fault_spec.is_some()
            && fault_spec.unwrap().operand == crate::fault::FaultOperand::B
        {
            crate::ntt::ntt_with_impl_and_mitigation(
                &tb,
                &self.params,
                ntt_trace_enabled,
                self.ntt_impl,
                Some(&mut *system_metrics),
                &self.mitigation,
                mitigation_metrics,
                fault_spec,
            )?
        } else {
            crate::ntt::ntt_with_impl_and_mitigation(
                &tb,
                &self.params,
                ntt_trace_enabled,
                self.ntt_impl,
                Some(&mut *system_metrics),
                &self.mitigation,
                mitigation_metrics,
                fault_spec,
            )?
        };

        // For CKKS pipeline experiments, a `mul` fault means a fault in one of the
        // pointwise multiplication operands in the NTT domain. This is intentionally
        // different from an iNTT-stage-0 fault, which targets the product vector
        // `c_hat` immediately before inverse transformation.
        if fault_op == Some("mul") {
            if let Some(spec) = fault {
                match spec.operand {
                    crate::fault::FaultOperand::A => {
                        inject_bit_fault(
                            &mut a_hat,
                            spec.slot,
                            spec.bit,
                            self.params.modulus_bits,
                            q,
                        )?;
                    }
                    crate::fault::FaultOperand::B => {
                        inject_bit_fault(
                            &mut b_hat,
                            spec.slot,
                            spec.bit,
                            self.params.modulus_bits,
                            q,
                        )?;
                    }
                }
            }
        }

        let c_hat = mul_ntt(&a_hat, &b_hat, q);

        let (mut c, intt_stages) = if fault_op == Some("intt") && fault.is_some() {
            crate::ntt::intt_with_impl_and_mitigation(
                &c_hat,
                &self.params,
                intt_trace_enabled,
                self.ntt_impl,
                Some(&mut *system_metrics),
                &self.mitigation,
                mitigation_metrics,
                fault_spec,
            )?
        } else {
            crate::ntt::intt_with_impl_and_mitigation(
                &c_hat,
                &self.params,
                intt_trace_enabled,
                self.ntt_impl,
                Some(&mut *system_metrics),
                &self.mitigation,
                mitigation_metrics,
                fault_spec,
            )?
        };

        for i in 0..n {
            c[i] = mul_mod(c[i], crate::modarith::pow_mod(psi_inv, i as u64, q), q);
        }

        let mut combined_ntt_stages = ntt_stages;
        if trace_options.ntt && !b_stages.is_empty() {
            combined_ntt_stages.extend(b_stages);
        }

        Ok(PipelineRun {
            coeffs: c,
            twisted_a: if trace_options.encode { Some(ta) } else { None },
            twisted_b: if trace_options.encode { Some(tb) } else { None },
            ntt_a: if trace_options.ntt {
                Some(a_hat.to_vec())
            } else {
                None
            },
            ntt_b: if trace_options.ntt {
                Some(b_hat.to_vec())
            } else {
                None
            },
            ntt_stages: combined_ntt_stages,
            mul_ntt: if trace_options.mul { Some(c_hat) } else { None },
            intt_stages,
        })
    }
}

#[derive(Debug, Clone)]
struct PipelineRun {
    coeffs: Vec<u64>,
    twisted_a: Option<Vec<u64>>,
    twisted_b: Option<Vec<u64>>,
    ntt_a: Option<Vec<u64>>,
    ntt_b: Option<Vec<u64>>,
    ntt_stages: Vec<StageTrace>,
    mul_ntt: Option<Vec<u64>>,
    intt_stages: Vec<StageTrace>,
}

pub fn demo_slots(n: usize) -> (Vec<Complex64>, Vec<Complex64>) {
    let half = n / 2;
    let mut a = Vec::with_capacity(half);
    let mut b = Vec::with_capacity(half);

    for i in 0..half {
        let x = i as f64 + 1.0;
        a.push(Complex64::new(x / 10.0, x / 20.0));
        b.push(Complex64::new((x + 1.0) / 12.0, -(x / 30.0)));
    }

    (a, b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fault::{FaultOperand, FaultSpec};

    fn ctx() -> CkksToyContext {
        let params = RingParams::new(16, 24).expect("valid test params");
        CkksToyContext::new_with_impl(params, 10, NttImplementation::Radix2)
    }

    #[test]
    fn ckks_demo_without_fault_has_zero_decoded_error() {
        let ctx = ctx();
        let (a, b) = demo_slots(ctx.params.n);
        let result = ctx
            .multiply_with_optional_fault(&a, &b, None, None)
            .expect("ckks demo should run");
        assert_eq!(result.metrics.max_abs_error, 0.0);
        assert_eq!(result.metrics.mean_abs_error, 0.0);
        assert_eq!(result.metrics.rms_error, 0.0);
        assert_eq!(result.metrics.relative_l2_error, 0.0);
        assert!(result.metrics.snr_db.is_infinite());
    }

    #[test]
    fn ckks_demo_fault_spec_creates_decoded_error() {
        let ctx = ctx();
        let (a, b) = demo_slots(ctx.params.n);
        let fault = FaultSpec::new(FaultOperand::A, 0, 0, 8);
        let result = ctx
            .multiply_with_optional_fault(&a, &b, Some("ntt"), Some(&fault))
            .expect("ckks demo should run");

        eprintln!("DEBUG ckks_demo_fault_spec_creates_decoded_error:");
        eprintln!("  fault={:?}", fault);
        eprintln!("  rms_error={}", result.metrics.rms_error);
        eprintln!("  max_abs_error={}", result.metrics.max_abs_error);
        eprintln!("  mean_abs_error={}", result.metrics.mean_abs_error);
        eprintln!("  relative_l2_error={}", result.metrics.relative_l2_error);
        eprintln!(
            "  mitigation checks={}",
            result.mitigation_metrics.checks_performed
        );
        eprintln!(
            "  mitigation failures={}",
            result.mitigation_metrics.check_failures
        );
        eprintln!(
            "  fault_detected={}",
            result.mitigation_metrics.fault_detected
        );

        assert!(result.metrics.rms_error > 0.0);
        assert!(result.metrics.max_abs_error > 0.0);
    }

    #[test]
    fn ckks_demo_mul_fault_creates_decoded_error() {
        let ctx = ctx();
        let (a, b) = demo_slots(ctx.params.n);
        let fault = FaultSpec::new(FaultOperand::A, 0, 0, 4);
        let result = ctx
            .multiply_with_optional_fault(&a, &b, Some("mul"), Some(&fault))
            .expect("ckks demo should run");
        assert!(result.metrics.rms_error > 0.0);
    }

    #[test]
    fn ckks_demo_trace_all_populates_intermediates() {
        let ctx = ctx();
        let (a, b) = demo_slots(ctx.params.n);
        let fault = FaultSpec::new(FaultOperand::A, 0, 0, 1);
        let result = ctx
            .multiply_with_optional_fault_traced(
                &a,
                &b,
                Some("ntt"),
                Some(&fault),
                CkksTraceOptions::all(),
            )
            .expect("ckks demo should run with traces");
        assert!(result.trace.encoded_a.is_some());
        assert!(result.trace.twisted_a.is_some());
        assert!(result.trace.correct_ntt_a.is_some());
        assert!(result.trace.faulty_ntt_a.is_some());
        assert!(!result.trace.correct_ntt_stages.is_empty());
        assert!(!result.trace.faulty_ntt_stages.is_empty());
        assert!(result.trace.correct_mul_ntt.is_some());
        assert!(result.trace.faulty_mul_ntt.is_some());
        assert!(!result.trace.correct_intt_stages.is_empty());
        assert!(!result.trace.faulty_intt_stages.is_empty());
        assert!(result.trace.decoded_correct.is_some());
        assert!(result.trace.decoded_faulty.is_some());
    }

    #[test]
    fn ckks_demo_mul_and_fault_spec_locations_are_distinct() {
        let ctx = ctx();
        let (a, b) = demo_slots(ctx.params.n);
        let fault = FaultSpec::new(FaultOperand::A, 0, 0, 4);

        let mul_result = ctx
            .multiply_with_optional_fault(&a, &b, Some("mul"), Some(&fault))
            .expect("mul fault should run");
        let intt_result = ctx
            .multiply_with_optional_fault(&a, &b, Some("intt"), Some(&fault))
            .expect("intt fault should run");

        assert_ne!(mul_result.decoded_faulty, intt_result.decoded_faulty);
    }

    #[test]
    fn ckks_demo_butterfly_check_records_checks() {
        let params = RingParams::new(16, 24).expect("valid test params");
        let ctx = CkksToyContext::new_with_impl(params, 10, NttImplementation::Radix2)
            .with_mitigation(crate::mitigation::MitigationOptions {
                kind: crate::mitigation::MitigationKind::ButterflyCheck,
                action: crate::mitigation::MitigationAction::DetectOnly,
                max_retries: 1,
                checksum_mode: crate::mitigation::ChecksumMode::Sum,
            });
        let (a, b) = demo_slots(ctx.params.n);
        let result = ctx
            .multiply_with_optional_fault(&a, &b, None, None)
            .expect("ckks demo should run");
        assert!(result.mitigation_metrics.mitigation_enabled);
        assert!(result.mitigation_metrics.checks_performed > 0);
        assert_eq!(result.mitigation_metrics.check_failures, 0);
    }
}
