//! Deterministic virtual-time HVAC simulation for the thermostat safety core.
//!
//! This crate contains no GPIO, networking, wall clock, or randomness. A
//! scenario is therefore exactly replayable from its initial state and outdoor
//! temperature sequence.

mod actuator;
mod clock;
mod scenario;
mod thermal;

pub use actuator::{ActuatorError, ActuatorTransition, VirtualActuator};
pub use clock::{ClockError, VirtualClock};
pub use scenario::{Scenario, ScenarioError, TraceSample};
pub use thermal::{ThermalModel, ThermalModelConfig, ThermalModelError};
