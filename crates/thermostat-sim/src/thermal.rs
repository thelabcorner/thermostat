use thermostat_core::EquipmentAction;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ThermalModelConfig {
    /// First-order envelope time constant. Larger values mean slower passive
    /// convergence toward outdoor temperature.
    pub envelope_time_constant_s: f64,
    /// Net indoor-temperature rate contribution while heating is active.
    pub heating_rate_c_per_s: f64,
    /// Magnitude of net indoor-temperature rate contribution while cooling.
    pub cooling_rate_c_per_s: f64,
    /// Constant internal gain represented as an equivalent temperature rate.
    pub internal_gain_c_per_s: f64,
}

impl Default for ThermalModelConfig {
    fn default() -> Self {
        Self {
            envelope_time_constant_s: 21_600.0,
            heating_rate_c_per_s: 0.0025,
            cooling_rate_c_per_s: 0.0020,
            internal_gain_c_per_s: 0.00005,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThermalModelError {
    NonFinite,
    InvalidTimeConstant,
    InvalidHeatingRate,
    InvalidCoolingRate,
}

impl ThermalModelConfig {
    /// Validates the deterministic first-order thermal model.
    ///
    /// # Errors
    ///
    /// Rejects non-finite values, a non-positive envelope time constant, or
    /// negative heating/cooling magnitudes.
    pub fn validate(self) -> Result<Self, ThermalModelError> {
        let values = [
            self.envelope_time_constant_s,
            self.heating_rate_c_per_s,
            self.cooling_rate_c_per_s,
            self.internal_gain_c_per_s,
        ];
        if values.iter().any(|value| !value.is_finite()) {
            return Err(ThermalModelError::NonFinite);
        }
        if self.envelope_time_constant_s <= 0.0 {
            return Err(ThermalModelError::InvalidTimeConstant);
        }
        if self.heating_rate_c_per_s < 0.0 {
            return Err(ThermalModelError::InvalidHeatingRate);
        }
        if self.cooling_rate_c_per_s < 0.0 {
            return Err(ThermalModelError::InvalidCoolingRate);
        }
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ThermalModel {
    indoor_c: f64,
    config: ThermalModelConfig,
}

impl ThermalModel {
    /// Creates a thermal model after validating its parameters and initial
    /// temperature.
    ///
    /// # Errors
    ///
    /// Returns a [`ThermalModelError`] when configuration or initial
    /// temperature is invalid.
    pub fn new(indoor_c: f64, config: ThermalModelConfig) -> Result<Self, ThermalModelError> {
        let config = config.validate()?;
        if !indoor_c.is_finite() {
            return Err(ThermalModelError::NonFinite);
        }
        Ok(Self { indoor_c, config })
    }

    #[must_use]
    pub const fn indoor_c(self) -> f64 {
        self.indoor_c
    }

    /// Advances the first-order building model exactly for a constant outdoor
    /// temperature and equipment action over the requested interval.
    ///
    /// # Errors
    ///
    /// Returns [`ThermalModelError::NonFinite`] for non-finite outdoor
    /// temperature or duration-derived arithmetic.
    pub fn advance(
        &mut self,
        action: EquipmentAction,
        outdoor_c: f64,
        duration_ms: u64,
    ) -> Result<f64, ThermalModelError> {
        if !outdoor_c.is_finite() {
            return Err(ThermalModelError::NonFinite);
        }

        #[allow(clippy::cast_precision_loss)]
        let elapsed_s = duration_ms as f64 / 1000.0;
        let hvac_rate = match action {
            EquipmentAction::Heat => self.config.heating_rate_c_per_s,
            EquipmentAction::Cool => -self.config.cooling_rate_c_per_s,
            EquipmentAction::Idle | EquipmentAction::FanOnly => 0.0,
        };
        let total_rate = hvac_rate + self.config.internal_gain_c_per_s;
        let tau = self.config.envelope_time_constant_s;
        let equilibrium_c = outdoor_c + total_rate * tau;
        let decay = (-elapsed_s / tau).exp();
        let next = equilibrium_c + (self.indoor_c - equilibrium_c) * decay;
        if !next.is_finite() {
            return Err(ThermalModelError::NonFinite);
        }
        self.indoor_c = next;
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passive_house_moves_toward_outdoor_temperature() {
        let config = ThermalModelConfig {
            internal_gain_c_per_s: 0.0,
            ..ThermalModelConfig::default()
        };
        let mut model = ThermalModel::new(20.0, config).unwrap_or_else(|error| {
            panic!("valid test model expected, got {error:?}");
        });
        let next = model
            .advance(EquipmentAction::Idle, 30.0, 3_600_000)
            .unwrap_or_else(|error| panic!("advance failed: {error:?}"));
        assert!(next > 20.0 && next < 30.0);
    }

    #[test]
    fn heating_and_cooling_move_in_opposite_directions() {
        let config = ThermalModelConfig::default();
        let mut heat = ThermalModel::new(21.0, config)
            .unwrap_or_else(|error| panic!("model failed: {error:?}"));
        let mut cool = heat;
        let heated = heat
            .advance(EquipmentAction::Heat, 21.0, 60_000)
            .unwrap_or_else(|error| panic!("advance failed: {error:?}"));
        let cooled = cool
            .advance(EquipmentAction::Cool, 21.0, 60_000)
            .unwrap_or_else(|error| panic!("advance failed: {error:?}"));
        assert!(heated > 21.0);
        assert!(cooled < 21.0);
        assert!(cooled < heated);
    }

    #[test]
    fn configuration_validation_rejects_each_invalid_parameter_class() {
        let non_finite = ThermalModelConfig {
            internal_gain_c_per_s: f64::NAN,
            ..ThermalModelConfig::default()
        };
        assert_eq!(non_finite.validate(), Err(ThermalModelError::NonFinite));

        let invalid_tau = ThermalModelConfig {
            envelope_time_constant_s: 0.0,
            ..ThermalModelConfig::default()
        };
        assert_eq!(
            invalid_tau.validate(),
            Err(ThermalModelError::InvalidTimeConstant)
        );

        let invalid_heat = ThermalModelConfig {
            heating_rate_c_per_s: -0.001,
            ..ThermalModelConfig::default()
        };
        assert_eq!(
            invalid_heat.validate(),
            Err(ThermalModelError::InvalidHeatingRate)
        );

        let invalid_cool = ThermalModelConfig {
            cooling_rate_c_per_s: -0.001,
            ..ThermalModelConfig::default()
        };
        assert_eq!(
            invalid_cool.validate(),
            Err(ThermalModelError::InvalidCoolingRate)
        );

        let zero_rates = ThermalModelConfig {
            heating_rate_c_per_s: 0.0,
            cooling_rate_c_per_s: 0.0,
            ..ThermalModelConfig::default()
        };
        assert_eq!(zero_rates.validate(), Ok(zero_rates));
    }

    #[test]
    fn model_rejects_non_finite_initial_and_outdoor_temperatures() {
        let cfg = ThermalModelConfig::default();
        assert_eq!(
            ThermalModel::new(f64::NAN, cfg),
            Err(ThermalModelError::NonFinite)
        );

        let mut model =
            ThermalModel::new(21.0, cfg).unwrap_or_else(|error| panic!("model failed: {error:?}"));
        assert_eq!(
            model.advance(EquipmentAction::Idle, f64::INFINITY, 1_000),
            Err(ThermalModelError::NonFinite)
        );
    }

    #[test]
    fn fan_only_has_no_direct_hvac_thermal_term() {
        let cfg = ThermalModelConfig {
            internal_gain_c_per_s: 0.0,
            ..ThermalModelConfig::default()
        };
        let mut idle =
            ThermalModel::new(21.0, cfg).unwrap_or_else(|error| panic!("model failed: {error:?}"));
        let mut fan = idle;
        let idle_next = idle
            .advance(EquipmentAction::Idle, 10.0, 60_000)
            .unwrap_or_else(|error| panic!("advance failed: {error:?}"));
        let fan_next = fan
            .advance(EquipmentAction::FanOnly, 10.0, 60_000)
            .unwrap_or_else(|error| panic!("advance failed: {error:?}"));
        assert!((idle_next - fan_next).abs() < f64::EPSILON);
    }

    #[test]
    fn positive_internal_gain_warms_relative_to_zero_gain() {
        let zero_gain_cfg = ThermalModelConfig {
            internal_gain_c_per_s: 0.0,
            ..ThermalModelConfig::default()
        };
        let gain_cfg = ThermalModelConfig {
            internal_gain_c_per_s: 0.0001,
            ..zero_gain_cfg
        };
        let mut zero_gain = ThermalModel::new(21.0, zero_gain_cfg)
            .unwrap_or_else(|error| panic!("model failed: {error:?}"));
        let mut with_gain = ThermalModel::new(21.0, gain_cfg)
            .unwrap_or_else(|error| panic!("model failed: {error:?}"));

        let baseline = zero_gain
            .advance(EquipmentAction::Idle, 21.0, 60_000)
            .unwrap_or_else(|error| panic!("advance failed: {error:?}"));
        let gained = with_gain
            .advance(EquipmentAction::Idle, 21.0, 60_000)
            .unwrap_or_else(|error| panic!("advance failed: {error:?}"));
        assert!(gained > baseline);
    }

    #[test]
    fn advance_rejects_non_finite_arithmetic_result() {
        let cfg = ThermalModelConfig {
            envelope_time_constant_s: f64::MAX,
            heating_rate_c_per_s: f64::MAX,
            cooling_rate_c_per_s: 0.0,
            internal_gain_c_per_s: 0.0,
        };
        let mut model =
            ThermalModel::new(21.0, cfg).unwrap_or_else(|error| panic!("model failed: {error:?}"));
        assert_eq!(
            model.advance(EquipmentAction::Heat, 21.0, 1_000),
            Err(ThermalModelError::NonFinite)
        );
    }
}
