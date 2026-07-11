mod ckks;
mod fault;
mod metrics;
mod mitigation;
mod modarith;
mod ntt;
mod params;
mod validation;
mod visualize;

use crate::ckks::{demo_slots, CkksToyContext, CkksTraceOptions};
use crate::fault::{FaultOperand, FaultSite, FaultSpec};
use crate::mitigation::{ChecksumMode, MitigationAction, MitigationKind, MitigationOptions};
use crate::ntt::NttImplementation;
use crate::params::RingParams;
use crate::visualize::{
    print_ckks_trace, print_complex_slots, print_decoded_comparison, print_decoded_metrics,
    print_mitigation_metrics, print_ring,
};
use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(name = "ckks_ntt")]
#[command(about = "NTT/iNTT, fault injection, and toy CKKS decoded-domain experiments")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Run {
        #[arg(long, default_value_t = 16)]
        n: usize,

        #[arg(long, default_value_t = 24)]
        bits: u32,
    },

    CkksDemo {
        #[arg(long, default_value_t = 16)]
        n: usize,

        #[arg(long, default_value_t = 50)]
        bits: u32,

        #[arg(long, default_value_t = 20)]
        scale_bits: u32,

        #[arg(long, value_enum, default_value_t = NttImplementation::Radix2)]
        ntt_impl: NttImplementation,

        #[arg(long, value_enum, default_value_t = MitigationKind::None)]
        mitigation: MitigationKind,

        #[arg(long, value_enum, default_value_t = MitigationAction::DetectOnly)]
        mitigation_action: MitigationAction,

        #[arg(long, default_value_t = 1)]
        max_retries: usize,

        #[arg(long, value_enum, default_value_t = ChecksumMode::Sum)]
        checksum_mode: ChecksumMode,

        #[arg(long, value_enum, default_value_t = CkksFaultOp::Ntt)]
        fault_op: CkksFaultOp,

        #[arg(long, default_value_t = false)]
        fault: bool,

        #[arg(long, value_enum, default_value_t = FaultOperand::A)]
        fault_operand: FaultOperand,

        #[arg(long, default_value_t = 0)]
        fault_stage: usize,

        #[arg(long, default_value_t = 0)]
        fault_slot: usize,

        #[arg(long, default_value_t = 0)]
        fault_bit: u32,

        #[arg(long, default_value_t = false)]
        fault_adjacent: bool,

        #[arg(long, default_value = "input")]
        fault_site: FaultSite,

        #[arg(long, default_value_t = false)]
        fault2: bool,

        #[arg(long, default_value_t = 0)]
        fault2_stage: usize,

        #[arg(long, default_value_t = 0)]
        fault2_slot: usize,

        #[arg(long, default_value_t = 0)]
        fault2_bit: u32,

        #[arg(long, default_value = "input")]
        fault2_site: FaultSite,

        #[arg(long, default_value_t = false)]
        fault2_adjacent: bool,

        #[arg(long, default_value_t = 8)]
        print_slots: usize,

        /// Print every trace section: encode, NTT, multiplication, iNTT, and decode.
        #[arg(long, default_value_t = false)]
        trace_all: bool,

        /// Print encoded coefficient vectors and negacyclic twist vectors.
        #[arg(long, default_value_t = false)]
        trace_encode: bool,

        /// Print forward NTT vectors and per-stage butterfly traces.
        #[arg(long, default_value_t = false)]
        trace_ntt: bool,

        /// Print pointwise multiplication vectors in the NTT domain.
        #[arg(long, default_value_t = false)]
        trace_mul: bool,

        /// Print inverse NTT per-stage butterfly traces and final coefficient vectors.
        #[arg(long, default_value_t = false)]
        trace_intt: bool,

        /// Print decoded correct/faulty slot vectors in the trace section.
        #[arg(long, default_value_t = false)]
        trace_decode: bool,

        /// Treat the demo as a validation run and fail if the expected no-fault/fault behavior is not observed.
        #[arg(long, default_value_t = false)]
        validate: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum CkksFaultOp {
    Ntt,
    Intt,
    Mul,
}

fn main() -> Result<(), String> {
    let cli = Cli::parse();

    match cli.command {
        Command::Run { n, bits } => {
            let params = RingParams::new(n, bits)?;
            print_ring(&params);
            println!(
                "Basic run command is available. Use `ckks-demo` for decoded-domain fault metrics."
            );
        }

        Command::CkksDemo {
            n,
            bits,
            scale_bits,
            ntt_impl,
            mitigation,
            mitigation_action,
            max_retries,
            checksum_mode,
            fault_op,
            fault,
            fault_operand,
            fault_stage,
            fault_slot,
            fault_bit,
            print_slots,
            trace_all,
            trace_encode,
            trace_ntt,
            trace_mul,
            trace_intt,
            trace_decode,
            validate,
            fault_adjacent,
            fault_site,
            fault2,
            fault2_stage,
            fault2_slot,
            fault2_bit,
            fault2_site,
            fault2_adjacent,
        } => {
            let params = RingParams::new(n, bits)?;
            let mitigation_options = MitigationOptions {
                kind: mitigation,
                action: mitigation_action,
                max_retries,
                checksum_mode,
            };
            let ctx = CkksToyContext::new_with_impl(params.clone(), scale_bits, ntt_impl)
                .with_mitigation(mitigation_options.clone());
            let (a_slots, b_slots) = demo_slots(n);

            print_ring(&params);
            println!("================ Toy CKKS Parameters ================");
            println!("Slots:       {}", ctx.slot_count());
            println!("Scale bits:  {}", scale_bits);
            println!("Scale:       {:.8e}", ctx.scale);
            println!("NTT impl:    {}", ntt_impl);
            println!(
                "Mitigation:  {:?} / {:?}, checksum={:?}",
                mitigation, mitigation_action, checksum_mode
            );
            println!("Fault on:    {:?}", fault_op);
            println!("Fault?       {}", fault);
            if fault {
                println!(
                    "\x1b[31mFault coordinates: operand={:?}, stage={}, slot={}, bit={}\x1b[0m",
                    fault_operand, fault_stage, fault_slot, fault_bit
                );
                println!("\x1b[31mFault site: {:?}\x1b[0m", fault_site);
                println!("Fault adjacent: {}", fault_adjacent);
                println!("Fault2 enabled: {}", fault2);
                if fault2 {
                    println!(
                        "Fault2 coordinates: stage={}, slot={}, bit={}",
                        fault2_stage, fault2_slot, fault2_bit
                    );
                    println!("Fault2 site: {:?}", fault2_site);
                    println!("Fault2 adjacent: {}", fault2_adjacent);
                }
            }
            println!("Trace encode: {}", trace_all || trace_encode);
            println!("Trace NTT:    {}", trace_all || trace_ntt);
            println!("Trace mul:    {}", trace_all || trace_mul);
            println!("Trace iNTT:   {}", trace_all || trace_intt);
            println!("Trace decode: {}", trace_all || trace_decode);
            println!("Validate:     {}", validate);
            println!("====================================================");

            let mut spec = FaultSpec::new(fault_operand, fault_stage, fault_slot, fault_bit);
            spec.site = fault_site;
            spec.adjacent = fault_adjacent;
            spec.second_enabled = fault2;
            spec.second_stage = fault2_stage;
            spec.second_slot = fault2_slot;
            spec.second_bit = fault2_bit;
            spec.second_site = fault2_site;
            spec.second_adjacent = fault2_adjacent;
            let fault_name = match fault_op {
                CkksFaultOp::Ntt => "ntt",
                CkksFaultOp::Intt => "intt",
                CkksFaultOp::Mul => "mul",
            };

            let trace_options = if trace_all {
                CkksTraceOptions::all()
            } else {
                CkksTraceOptions {
                    encode: trace_encode,
                    ntt: trace_ntt,
                    mul: trace_mul,
                    intt: trace_intt,
                    decode: trace_decode,
                }
            };

            let result = if trace_encode
                || trace_ntt
                || trace_mul
                || trace_intt
                || trace_decode
                || trace_all
            {
                ctx.multiply_with_optional_fault_traced(
                    &a_slots,
                    &b_slots,
                    if fault { Some(fault_name) } else { None },
                    if fault { Some(&spec) } else { None },
                    trace_options,
                )?
            } else {
                ctx.multiply_with_optional_fault(
                    &a_slots,
                    &b_slots,
                    if fault { Some(fault_name) } else { None },
                    if fault { Some(&spec) } else { None },
                )?
            };

            print_complex_slots("Input slots a", &result.input_a, print_slots);
            print_complex_slots("Input slots b", &result.input_b, print_slots);
            print_decoded_comparison(&result.decoded_correct, &result.decoded_faulty, print_slots);
            print_decoded_metrics(&result.metrics);
            print_mitigation_metrics(&result.mitigation_metrics);
            print_system_metrics(&result.system_metrics);
            print_ckks_trace(&result.trace, print_slots);

            if validate {
                let report = validate_ckks_demo_behavior(fault, &result.metrics)?;
                print_ckks_validation_report(&report);
            }
        }
    }

    Ok(())
}

fn print_system_metrics(metrics: &crate::ntt::NttSystemMetrics) {
    println!("================ Systems Metrics ====================");
    match metrics.ntt_impl {
        Some(implementation) => println!("NTT implementation: {}", implementation),
        None => println!("NTT implementation: unknown"),
    }
    println!("Elapsed NTT time:     {} ns", metrics.elapsed_ns);
    println!("Input bytes:          {}", metrics.input_bytes);
    println!("Output bytes:         {}", metrics.output_bytes);
    println!("Scratch bytes:        {}", metrics.scratch_bytes);
    println!("Twiddle table bytes:  {}", metrics.twiddle_table_bytes);
    println!("Modular adds:         {}", metrics.num_mod_adds);
    println!("Modular subs:         {}", metrics.num_mod_subs);
    println!("Modular muls:         {}", metrics.num_mod_muls);
    println!("Twiddle loads:        {}", metrics.num_twiddle_loads);
    println!("Stage barriers:       {}", metrics.num_stage_barriers);
    println!("Memory reads:         {}", metrics.num_memory_reads);
    println!("Memory writes:        {}", metrics.num_memory_writes);
    println!("Passes:               {}", metrics.num_passes);
    println!("Buffer swaps:         {}", metrics.num_buffer_swaps);
    println!("Blocks processed:     {}", metrics.num_blocks);
    println!("Block bytes:          {}", metrics.block_bytes);
    println!("====================================================");
}

#[derive(Debug, Clone)]
struct CkksValidationReport {
    execution_valid: bool,
    fault_enabled: bool,
    golden_match: bool,
    fault_observed: Option<bool>,
}

fn validate_ckks_demo_behavior(
    fault: bool,
    metrics: &crate::metrics::DecodedMetrics,
) -> Result<CkksValidationReport, String> {
    let golden_match = metrics.max_abs_error == 0.0 && metrics.rms_error == 0.0;
    let fault_observed = if fault { Some(!golden_match) } else { None };

    let report = CkksValidationReport {
        execution_valid: true,
        fault_enabled: fault,
        golden_match,
        fault_observed,
    };

    if fault {
        if fault_observed != Some(true) {
            return Err(
                "validation failed: fault was enabled but no decoded-domain effect was observed"
                    .to_string(),
            );
        }
    } else if !golden_match {
        return Err(
            "validation failed: no fault was enabled but decoded output differs from golden output"
                .to_string(),
        );
    }

    Ok(report)
}

fn pass_fail(x: bool) -> &'static str {
    if x {
        "PASS"
    } else {
        "FAIL"
    }
}

fn print_ckks_validation_report(report: &CkksValidationReport) {
    println!("================ CKKS Demo Validation ================");
    println!("Execution valid: {}", pass_fail(report.execution_valid));
    println!("Fault enabled:   {}", report.fault_enabled);
    println!("Golden match:    {}", pass_fail(report.golden_match));
    match report.fault_observed {
        Some(x) => println!("Fault observed:  {}", pass_fail(x)),
        None => println!("Fault observed:  N/A"),
    }
    println!("======================================================");
}
