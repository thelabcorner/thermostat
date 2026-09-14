//! Pure domain model for the thermostat controller.
//!
//! This crate intentionally contains no I/O, wall clock, GPIO, networking, or
//! persistence dependencies. Types here are shared by the deterministic
//! controller and future simulator/daemon crates.

mod config;
mod temperature;
mod types;

pub use config::{ComfortConfig, ConfigError, ControllerConfig, EquipmentProfile, TimingConfig};
pub use temperature::{MilliCelsius, TemperatureError};
pub use types::{
    BlockReason, Demand, EquipmentAction, EquipmentFamily, FanMode, Mode, OutputVector,
};
