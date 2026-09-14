use crate::{EquipmentAction, MilliCelsius, OutputVector};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ComfortConfig {
    /// Start heating at or below this temperature.
    pub heat_on: MilliCelsius,
    /// Stop heating at or above this temperature.
    pub heat_off: MilliCelsius,
    /// Stop cooling at or below this temperature.
    pub cool_off: MilliCelsius,
    /// Start cooling at or above this temperature.
    pub cool_on: MilliCelsius,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TimingConfig {
    pub cooling_startup_lockout_ms: u64,
    pub cooling_min_off_ms: u64,
    pub cooling_min_run_ms: u64,
    pub heating_startup_lockout_ms: u64,
    pub heating_min_off_ms: u64,
    pub heating_min_run_ms: u64,
    pub changeover_ms: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EquipmentProfile {
    pub idle: OutputVector,
    pub heat: OutputVector,
    pub cool: OutputVector,
    pub fan_only: OutputVector,
    /// Whether a user `FanMode::On` may add a G call while heating.
    pub allow_continuous_fan_with_heat: bool,
}

impl EquipmentProfile {
    /// Draft profile for the installed Goodman GMSS960804CNAA conventional
    /// gas-furnace + split-A/C installation.
    #[must_use]
    pub const fn goodman_gmss960804cnaa() -> Self {
        Self {
            idle: OutputVector::OFF,
            heat: OutputVector {
                w: true,
                y: false,
                g: false,
            },
            cool: OutputVector {
                w: false,
                y: true,
                g: true,
            },
            fan_only: OutputVector {
                w: false,
                y: false,
                g: true,
            },
            allow_continuous_fan_with_heat: true,
        }
    }

    #[must_use]
    pub const fn outputs_for(self, action: EquipmentAction) -> OutputVector {
        match action {
            EquipmentAction::Idle => self.idle,
            EquipmentAction::Heat => self.heat,
            EquipmentAction::Cool => self.cool,
            EquipmentAction::FanOnly => self.fan_only,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ControllerConfig {
    pub comfort: ComfortConfig,
    pub timing: TimingConfig,
    pub equipment: EquipmentProfile,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigError {
    HeatHysteresisInvalid,
    CoolHysteresisInvalid,
    AutoBandOverlap,
    IdleMustBeOff,
    HeatMappingInvalid,
    CoolMappingInvalid,
    FanMappingInvalid,
    HeatCoolConflict,
}

impl ControllerConfig {
    /// Validates relationships that must hold before a controller can be armed.
    ///
    /// # Errors
    ///
    /// Returns a [`ConfigError`] when hysteresis bands overlap or when an
    /// equipment action maps to an unsafe/invalid thermostat contact vector.
    pub fn validate(self) -> Result<Self, ConfigError> {
        if self.comfort.heat_on >= self.comfort.heat_off {
            return Err(ConfigError::HeatHysteresisInvalid);
        }
        if self.comfort.cool_off >= self.comfort.cool_on {
            return Err(ConfigError::CoolHysteresisInvalid);
        }
        if self.comfort.heat_off >= self.comfort.cool_off {
            return Err(ConfigError::AutoBandOverlap);
        }

        let profile = self.equipment;
        if !profile.idle.is_all_off() {
            return Err(ConfigError::IdleMustBeOff);
        }
        if !profile.heat.w || profile.heat.y {
            return Err(ConfigError::HeatMappingInvalid);
        }
        if !profile.cool.y || profile.cool.w {
            return Err(ConfigError::CoolMappingInvalid);
        }
        if !profile.fan_only.g || profile.fan_only.w || profile.fan_only.y {
            return Err(ConfigError::FanMappingInvalid);
        }

        for output in [profile.idle, profile.heat, profile.cool, profile.fan_only] {
            if output.has_heat_cool_conflict() {
                return Err(ConfigError::HeatCoolConflict);
            }
        }

        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn mc(value: i32) -> MilliCelsius {
        match MilliCelsius::from_milli_celsius(value) {
            Some(value) => value,
            None => panic!("test temperature must be in range"),
        }
    }

    fn config() -> ControllerConfig {
        ControllerConfig {
            comfort: ComfortConfig {
                heat_on: mc(20_000),
                heat_off: mc(20_500),
                cool_off: mc(22_500),
                cool_on: mc(23_000),
            },
            timing: TimingConfig {
                cooling_startup_lockout_ms: 300_000,
                cooling_min_off_ms: 300_000,
                cooling_min_run_ms: 180_000,
                heating_startup_lockout_ms: 0,
                heating_min_off_ms: 60_000,
                heating_min_run_ms: 60_000,
                changeover_ms: 300_000,
            },
            equipment: EquipmentProfile::goodman_gmss960804cnaa(),
        }
    }

    #[test]
    fn current_profile_validates() {
        assert_eq!(config().validate(), Ok(config()));
    }

    #[test]
    fn overlapping_auto_band_is_rejected() {
        let mut invalid = config();
        invalid.comfort.cool_off = invalid.comfort.heat_off;
        assert_eq!(invalid.validate(), Err(ConfigError::AutoBandOverlap));
    }
}
