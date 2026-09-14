//! Deterministic thermostat safety/control state machine.
//!
//! The controller accepts explicit monotonic timestamps and validated domain
//! events. It performs no I/O. That makes every transition deterministic,
//! replayable, fuzzable, and suitable for simulation before GPIO exists.

mod controller;
mod invariants;
mod state;

pub use controller::{Event, Transition, TransitionRecord, step};
pub use invariants::{InvariantViolation, validate_output_vector, validate_state};
pub use state::{ControlTemperature, ControllerState, CriticalFault, Phase};
pub use thermostat_model::{
    BlockReason, ComfortConfig, ConfigError, ControllerConfig, Demand, EquipmentAction,
    EquipmentFamily, EquipmentProfile, FanMode, MilliCelsius, Mode, OutputVector, TimingConfig,
};
