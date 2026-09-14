use thermostat_core::{
    ControllerConfig, ControllerState, Event, FanMode, MilliCelsius, Mode, OutputVector,
    TransitionRecord, step, validate_state,
};

use crate::{
    ActuatorError, ClockError, ThermalModel, ThermalModelConfig, ThermalModelError,
    VirtualActuator, VirtualClock,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TraceSample {
    pub at_ms: u64,
    pub indoor_c: f64,
    pub outdoor_c: f64,
    pub outputs: OutputVector,
    pub transition: TransitionRecord,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScenarioError {
    Clock(ClockError),
    Thermal(ThermalModelError),
    TemperatureConversion,
    Actuator(ActuatorError),
    ControllerInvariant,
}

pub struct Scenario {
    controller_config: ControllerConfig,
    state: ControllerState,
    clock: VirtualClock,
    thermal: ThermalModel,
    actuator: VirtualActuator,
    sensor_ttl_ms: u64,
    mode: Mode,
    fan_mode: FanMode,
    trace: Vec<TraceSample>,
}

impl Scenario {
    /// Creates a deterministic virtual thermostat scenario.
    ///
    /// # Errors
    ///
    /// Returns an error when the thermal model is invalid.
    pub fn new(
        initial_indoor_c: f64,
        controller_config: ControllerConfig,
        thermal_config: ThermalModelConfig,
        sensor_ttl_ms: u64,
    ) -> Result<Self, ScenarioError> {
        let thermal =
            ThermalModel::new(initial_indoor_c, thermal_config).map_err(ScenarioError::Thermal)?;
        Ok(Self {
            controller_config,
            state: ControllerState::new(0),
            clock: VirtualClock::new(0),
            thermal,
            actuator: VirtualActuator::default(),
            sensor_ttl_ms,
            mode: Mode::Off,
            fan_mode: FanMode::Auto,
            trace: Vec::new(),
        })
    }

    #[must_use]
    pub const fn state(&self) -> &ControllerState {
        &self.state
    }

    #[must_use]
    pub const fn actuator(&self) -> &VirtualActuator {
        &self.actuator
    }

    #[must_use]
    pub const fn clock(&self) -> VirtualClock {
        self.clock
    }

    #[must_use]
    pub const fn thermal(&self) -> ThermalModel {
        self.thermal
    }

    #[must_use]
    pub fn trace(&self) -> &[TraceSample] {
        &self.trace
    }

    /// Arms the virtual actuator after providing an initial local temperature,
    /// then applies the requested fan and HVAC modes through ordinary events.
    ///
    /// # Errors
    ///
    /// Returns an error if conversion, invariant checking, or actuator
    /// application fails.
    pub fn arm(&mut self, mode: Mode, fan_mode: FanMode) -> Result<(), ScenarioError> {
        self.mode = mode;
        self.fan_mode = fan_mode;
        self.feed_temperature()?;
        self.apply_event(Event::SetActuatorArmed(true))?;
        self.apply_event(Event::SetFanMode(fan_mode))?;
        self.apply_event(Event::SetMode(mode))?;
        Ok(())
    }

    /// Advances plant physics and then emits a fresh local sensor observation.
    ///
    /// # Errors
    ///
    /// Propagates virtual clock, thermal-model, temperature-conversion,
    /// controller-invariant, or actuator errors.
    pub fn step(&mut self, outdoor_c: f64, delta_ms: u64) -> Result<(), ScenarioError> {
        self.advance_plant(outdoor_c, delta_ms)?;
        self.feed_temperature_with_outdoor(outdoor_c)
    }

    /// Advances plant physics without refreshing the local temperature sensor,
    /// then sends only a timer tick. This is used to test stale-sensor policy.
    ///
    /// # Errors
    ///
    /// Propagates virtual clock, thermal-model, invariant, or actuator errors.
    pub fn step_without_sensor(
        &mut self,
        outdoor_c: f64,
        delta_ms: u64,
    ) -> Result<(), ScenarioError> {
        self.advance_plant(outdoor_c, delta_ms)?;
        self.apply_event_with_outdoor(Event::Tick, outdoor_c)
    }

    /// Simulates a daemon restart. Physical outputs are first driven to safe
    /// OFF, monotonic boot history is discarded, and normal arming is repeated.
    /// Cooling therefore receives the full startup lockout again.
    ///
    /// # Errors
    ///
    /// Returns an error if any safe-off or re-arm operation fails.
    pub fn restart(&mut self) -> Result<(), ScenarioError> {
        let now = self.clock.now_ms();
        self.actuator
            .apply(now, OutputVector::OFF)
            .map_err(ScenarioError::Actuator)?;
        self.state = ControllerState::new(now);
        self.arm(self.mode, self.fan_mode)
    }

    fn advance_plant(&mut self, outdoor_c: f64, delta_ms: u64) -> Result<(), ScenarioError> {
        self.thermal
            .advance(self.state.action, outdoor_c, delta_ms)
            .map_err(ScenarioError::Thermal)?;
        self.clock
            .advance_ms(delta_ms)
            .map_err(ScenarioError::Clock)?;
        Ok(())
    }

    fn feed_temperature(&mut self) -> Result<(), ScenarioError> {
        self.feed_temperature_with_outdoor(f64::NAN)
    }

    fn feed_temperature_with_outdoor(&mut self, outdoor_c: f64) -> Result<(), ScenarioError> {
        let temperature = MilliCelsius::from_celsius(self.thermal.indoor_c())
            .map_err(|_| ScenarioError::TemperatureConversion)?;
        self.apply_event_with_outdoor(
            Event::SetControlTemperature {
                value: temperature,
                valid_for_ms: self.sensor_ttl_ms,
            },
            outdoor_c,
        )
    }

    fn apply_event(&mut self, event: Event) -> Result<(), ScenarioError> {
        self.apply_event_with_outdoor(event, f64::NAN)
    }

    fn apply_event_with_outdoor(
        &mut self,
        event: Event,
        outdoor_c: f64,
    ) -> Result<(), ScenarioError> {
        let now = self.clock.now_ms();
        let transition = step(self.state.clone(), event, now, self.controller_config);
        if validate_state(&transition.state).is_err() {
            return Err(ScenarioError::ControllerInvariant);
        }
        self.actuator
            .apply(now, transition.next_outputs)
            .map_err(ScenarioError::Actuator)?;
        let record = transition.record(event, now);
        self.state = transition.state;
        if outdoor_c.is_finite() {
            self.trace.push(TraceSample {
                at_ms: now,
                indoor_c: self.thermal.indoor_c(),
                outdoor_c,
                outputs: self.state.outputs,
                transition: record,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use thermostat_core::{BlockReason, ComfortConfig, EquipmentProfile, TimingConfig};

    use super::*;

    const fn mc(value: i32) -> MilliCelsius {
        match MilliCelsius::from_milli_celsius(value) {
            Some(value) => value,
            None => panic!("test temperature must be valid"),
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

    fn scenario(initial_c: f64) -> Scenario {
        Scenario::new(initial_c, config(), ThermalModelConfig::default(), 120_000)
            .unwrap_or_else(|error| panic!("scenario creation failed: {error:?}"))
    }

    #[test]
    fn multi_day_auto_simulation_never_creates_illegal_outputs() {
        let mut scenario = scenario(21.5);
        assert!((scenario.thermal().indoor_c() - 21.5).abs() < f64::EPSILON);
        scenario
            .arm(Mode::Auto, FanMode::Auto)
            .unwrap_or_else(|error| panic!("arm failed: {error:?}"));

        // 72 virtual hours at one-minute resolution. Outdoor temperature
        // follows a deterministic daily square-wave stress profile.
        for minute in 0..(72 * 60) {
            let outdoor = if (minute / 360) % 2 == 0 { 5.0 } else { 34.0 };
            scenario
                .step(outdoor, 60_000)
                .unwrap_or_else(|error| panic!("step {minute} failed: {error:?}"));
            assert_eq!(validate_state(scenario.state()), Ok(()));
        }

        assert!(!scenario.trace().is_empty());
        assert!(scenario.actuator().transitions().len() > 4);
    }

    #[test]
    fn stale_sensor_forces_active_heat_safe_off() {
        let mut scenario = scenario(19.0);
        scenario
            .arm(Mode::Heat, FanMode::Auto)
            .unwrap_or_else(|error| panic!("arm failed: {error:?}"));
        assert!(scenario.state().outputs.w);

        scenario
            .step_without_sensor(0.0, 120_001)
            .unwrap_or_else(|error| panic!("step failed: {error:?}"));
        assert_eq!(scenario.state().outputs, OutputVector::OFF);
        assert_eq!(scenario.state().block_reason, BlockReason::SensorInvalid);
    }

    #[test]
    fn restart_during_cooling_reapplies_startup_lockout() {
        let mut scenario = scenario(24.0);
        scenario
            .arm(Mode::Cool, FanMode::Auto)
            .unwrap_or_else(|error| panic!("arm failed: {error:?}"));
        scenario
            .step(35.0, 300_000)
            .unwrap_or_else(|error| panic!("step failed: {error:?}"));
        assert!(scenario.state().outputs.y);

        let restart_at = scenario.clock().now_ms();
        scenario
            .restart()
            .unwrap_or_else(|error| panic!("restart failed: {error:?}"));
        assert!(!scenario.state().outputs.y);
        assert_eq!(scenario.state().block_reason, BlockReason::StartupLockout);

        scenario
            .step(35.0, 299_999)
            .unwrap_or_else(|error| panic!("step failed: {error:?}"));
        assert!(!scenario.state().outputs.y);
        assert_eq!(scenario.clock().now_ms(), restart_at + 299_999);
        scenario
            .step(35.0, 1)
            .unwrap_or_else(|error| panic!("step failed: {error:?}"));
        assert!(scenario.state().outputs.y);
    }
}
