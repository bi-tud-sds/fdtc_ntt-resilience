use clap::ValueEnum;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaultSite {
    Input,
    MulOutput,
    AddOutput,
    SubOutput,
    ButterflyOutput,
    RegisterWrite,
}

impl std::str::FromStr for FaultSite {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "input" => Ok(Self::Input),
            "mul-output" => Ok(Self::MulOutput),
            "add-output" => Ok(Self::AddOutput),
            "sub-output" => Ok(Self::SubOutput),
            "butterfly-output" => Ok(Self::ButterflyOutput),
            "register-write" => Ok(Self::RegisterWrite),
            _ => Err(format!("unsupported fault site: {s}")),
        }
    }
}

impl std::fmt::Display for FaultSite {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Input => "input",
            Self::MulOutput => "mul-output",
            Self::AddOutput => "add-output",
            Self::SubOutput => "sub-output",
            Self::ButterflyOutput => "butterfly-output",
            Self::RegisterWrite => "register-write",
        };
        write!(f, "{s}")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum FaultOperand {
    A,
    B,
}

#[derive(Debug, Clone)]
pub struct FaultSpec {
    pub operand: FaultOperand,
    pub stage: usize,
    pub slot: usize,
    pub bit: u32,
    pub site: FaultSite,
    pub adjacent: bool,
    pub second_enabled: bool,
    pub second_stage: usize,
    pub second_slot: usize,
    pub second_bit: u32,
    pub second_site: FaultSite,
    pub second_adjacent: bool,
}

impl FaultSpec {
    #[allow(dead_code)]
    pub fn is_input_site(&self) -> bool {
        matches!(self.site, FaultSite::Input)
    }

    #[allow(dead_code)]
    pub fn is_arithmetic_site(&self) -> bool {
        !self.is_input_site()
    }

    pub fn new(operand: FaultOperand, stage: usize, slot: usize, bit: u32) -> Self {
        Self {
            operand,
            stage,
            slot,
            bit,
            site: FaultSite::Input,
            adjacent: false,
            second_enabled: false,
            second_stage: stage,
            second_slot: slot,
            second_bit: bit,
            second_site: FaultSite::Input,
            second_adjacent: false,
        }
    }
}

pub fn inject_bit_fault(
    v: &mut [u64],
    slot: usize,
    bit: u32,
    modulus_bits: u32,
    q: u64,
) -> Result<(), String> {
    if slot >= v.len() {
        return Err(format!(
            "fault slot {} out of range for vector length {}",
            slot,
            v.len()
        ));
    }
    if bit >= modulus_bits {
        return Err(format!(
            "fault bit {} out of range for modulus bit width {}",
            bit, modulus_bits
        ));
    }
    v[slot] ^= 1u64 << bit;
    v[slot] %= q;
    Ok(())
}
