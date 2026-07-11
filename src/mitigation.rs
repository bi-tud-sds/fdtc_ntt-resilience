use clap::ValueEnum;

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum MitigationKind {
    None,
    ButterflyCheck,
    StageChecksum,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum MitigationAction {
    DetectOnly,
    Recompute,
    Abort,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum ChecksumMode {
    /// S1 = sum_i a_i mod q
    Sum,
    /// S1 = sum_i a_i mod q and S2 = sum_i i*a_i mod q
    SumIndex,
}

#[derive(Clone, Debug)]
pub struct MitigationOptions {
    pub kind: MitigationKind,
    pub action: MitigationAction,
    pub max_retries: usize,
    pub checksum_mode: ChecksumMode,
}

impl Default for MitigationOptions {
    fn default() -> Self {
        Self {
            kind: MitigationKind::None,
            action: MitigationAction::DetectOnly,
            max_retries: 1,
            checksum_mode: ChecksumMode::Sum,
        }
    }
}

impl MitigationOptions {
    pub fn disabled() -> Self {
        Self::default()
    }

    pub fn enabled(&self) -> bool {
        self.kind != MitigationKind::None
    }
}

#[derive(Debug, Clone, Default)]
pub struct MitigationMetrics {
    pub mitigation_enabled: bool,
    pub mitigation_kind: String,
    pub mitigation_action: String,
    pub checksum_mode: String,
    pub checks_performed: u64,
    pub check_failures: u64,
    pub recomputations: u64,
    pub fault_detected: bool,
    pub fault_corrected: bool,
    pub mitigation_elapsed_ns: u128,
    pub stage_checks_performed: u64,
    pub stage_check_failures: u64,
    pub stage_checksum_s1_failures: u64,
    pub stage_checksum_s2_failures: u64,
}

#[allow(dead_code)]
pub fn record_butterfly_check_result(
    metrics: &mut MitigationMetrics,
    expected_y0: u64,
    expected_y1: u64,
    actual_y0: u64,
    actual_y1: u64,
) {
    metrics.checks_performed += 1;

    if expected_y0 != actual_y0 || expected_y1 != actual_y1 {
        metrics.check_failures += 1;
        metrics.fault_detected = true;
    }
}

impl MitigationMetrics {
    pub fn configure(&mut self, options: &MitigationOptions) {
        self.mitigation_enabled = options.enabled();
        self.mitigation_kind = format!("{:?}", options.kind);
        self.mitigation_action = format!("{:?}", options.action);
        self.checksum_mode = format!("{:?}", options.checksum_mode);
    }
}
