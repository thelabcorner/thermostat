use thermostat_model::{
    BlockReason, Demand, EquipmentAction, EquipmentFamily, FanMode, MilliCelsius, Mode,
    OutputVector,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ControlTemperature {
    pub value: MilliCelsius,
    /// Monotonic timestamp through which this fused observation remains valid.
    pub valid_until_ms: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CriticalFault {
    External,
    NonMonotonicTime,
    OutputInvariantViolation,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Phase {
    #[default]
    BootSafe,
    Idle,
    Heating,
    Cooling,
    FanOnly,
    FaultSafe,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ControllerState {
    pub mode: Mode,
    pub fan_mode: FanMode,
    pub phase: Phase,
    pub demand: Demand,
    pub action: EquipmentAction,
    pub outputs: OutputVector,
    pub block_reason: BlockReason,
    pub actuator_armed: bool,
    pub control_temperature: Option<ControlTemperature>,
    pub critical_fault: Option<CriticalFault>,
    pub booted_at_ms: u64,
    pub last_evaluated_at_ms: u64,
    pub current_action_started_at_ms: Option<u64>,
    pub last_heat_stopped_at_ms: Option<u64>,
    pub last_cool_stopped_at_ms: Option<u64>,
    pub last_family_stopped: Option<(EquipmentFamily, u64)>,
}

impl ControllerState {
    #[must_use]
    pub const fn new(booted_at_ms: u64) -> Self {
        Self {
            mode: Mode::Off,
            fan_mode: FanMode::Auto,
            phase: Phase::BootSafe,
            demand: Demand::None,
            action: EquipmentAction::Idle,
            outputs: OutputVector::OFF,
            block_reason: BlockReason::ActuatorNotArmed,
            actuator_armed: false,
            control_temperature: None,
            critical_fault: None,
            booted_at_ms,
            last_evaluated_at_ms: booted_at_ms,
            current_action_started_at_ms: None,
            last_heat_stopped_at_ms: None,
            last_cool_stopped_at_ms: None,
            last_family_stopped: None,
        }
    }
}
