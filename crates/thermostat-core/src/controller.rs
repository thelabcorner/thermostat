use thermostat_model::{
    BlockReason, ControllerConfig, Demand, EquipmentAction, EquipmentFamily, FanMode, Mode,
    OutputVector,
};

use crate::{
    ControlTemperature, ControllerState, CriticalFault, MilliCelsius, Phase, validate_output_vector,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Event {
    Tick,
    SetMode(Mode),
    SetFanMode(FanMode),
    SetControlTemperature {
        value: MilliCelsius,
        valid_for_ms: u64,
    },
    ClearControlTemperature,
    SetActuatorArmed(bool),
    RaiseCriticalFault,
    ClearCriticalFault,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Transition {
    pub previous_action: EquipmentAction,
    pub next_action: EquipmentAction,
    pub previous_outputs: OutputVector,
    pub next_outputs: OutputVector,
    pub block_reason: BlockReason,
    pub next_deadline_ms: Option<u64>,
    pub state: ControllerState,
}

/// Compact semantic record suitable for persistence/replay without exposing a
/// raw relay-write API.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransitionRecord {
    pub at_ms: u64,
    pub event: Event,
    pub previous_action: EquipmentAction,
    pub next_action: EquipmentAction,
    pub previous_outputs: OutputVector,
    pub next_outputs: OutputVector,
    pub next_phase: Phase,
    pub next_demand: Demand,
    pub block_reason: BlockReason,
    pub next_deadline_ms: Option<u64>,
}

impl Transition {
    /// Converts a completed transition into the semantic event record that a
    /// future persistence layer can append asynchronously.
    #[must_use]
    pub const fn record(&self, event: Event, at_ms: u64) -> TransitionRecord {
        TransitionRecord {
            at_ms,
            event,
            previous_action: self.previous_action,
            next_action: self.next_action,
            previous_outputs: self.previous_outputs,
            next_outputs: self.next_outputs,
            next_phase: self.state.phase,
            next_demand: self.state.demand,
            block_reason: self.block_reason,
            next_deadline_ms: self.next_deadline_ms,
        }
    }

    #[must_use]
    pub const fn changed_outputs(&self) -> bool {
        self.previous_outputs.w != self.next_outputs.w
            || self.previous_outputs.y != self.next_outputs.y
            || self.previous_outputs.g != self.next_outputs.g
    }
}

#[must_use]
pub fn step(
    mut state: ControllerState,
    event: Event,
    now_ms: u64,
    config: ControllerConfig,
) -> Transition {
    let previous_action = state.action;
    let previous_outputs = state.outputs;

    if now_ms < state.last_evaluated_at_ms {
        let safe_stop_ms = state.last_evaluated_at_ms;
        state.critical_fault = Some(CriticalFault::NonMonotonicTime);
        force_safe(&mut state, BlockReason::NonMonotonicTime, safe_stop_ms);
        return finish(state, previous_action, previous_outputs, None, now_ms);
    }

    state.last_evaluated_at_ms = now_ms;
    apply_event(&mut state, event, now_ms);

    if config.validate().is_err() {
        return safe_transition(
            state,
            BlockReason::ConfigInvalid,
            None,
            previous_action,
            previous_outputs,
            now_ms,
        );
    }

    if state.critical_fault.is_some() {
        return safe_transition(
            state,
            BlockReason::CriticalFault,
            None,
            previous_action,
            previous_outputs,
            now_ms,
        );
    }

    if !state.actuator_armed {
        return safe_transition(
            state,
            BlockReason::ActuatorNotArmed,
            Some(Phase::BootSafe),
            previous_action,
            previous_outputs,
            now_ms,
        );
    }

    evaluate_armed_state(state, previous_action, previous_outputs, now_ms, config)
}

fn evaluate_armed_state(
    mut state: ControllerState,
    previous_action: EquipmentAction,
    previous_outputs: OutputVector,
    now_ms: u64,
    config: ControllerConfig,
) -> Transition {
    // Mode OFF is intentionally usable without a temperature sensor. Fan ON
    // remains independent, matching conventional thermostat behavior.
    if state.mode == Mode::Off {
        stop_active_conditioning(&mut state, now_ms);
        state.demand = Demand::None;
        let action = match state.fan_mode {
            FanMode::Auto => EquipmentAction::Idle,
            FanMode::On => EquipmentAction::FanOnly,
        };
        set_action(&mut state, action, now_ms);
        return finalize_outputs(
            state,
            previous_action,
            previous_outputs,
            None,
            now_ms,
            config,
        );
    }

    let Some(control_temperature) = state.control_temperature else {
        stop_active_conditioning(&mut state, now_ms);
        return safe_transition(
            state,
            BlockReason::SensorInvalid,
            None,
            previous_action,
            previous_outputs,
            now_ms,
        );
    };
    if now_ms > control_temperature.valid_until_ms {
        stop_active_conditioning(&mut state, now_ms);
        return safe_transition(
            state,
            BlockReason::SensorInvalid,
            None,
            previous_action,
            previous_outputs,
            now_ms,
        );
    }

    let demand = calculate_demand(&state, control_temperature.value, config);
    state.demand = demand;

    let sensor_deadline = sensor_expiry_deadline(&state);
    let mut deadline = min_deadline(
        apply_active_cycle_policy(&mut state, demand, now_ms, config),
        sensor_deadline,
    );

    if let Some((block_reason, family_deadline)) =
        family_reversal_break(&mut state, previous_action, demand, now_ms, config)
    {
        state.block_reason = block_reason;
        deadline = min_deadline(deadline, Some(family_deadline));
        return finalize_outputs(
            state,
            previous_action,
            previous_outputs,
            deadline,
            now_ms,
            config,
        );
    }

    // If the active-cycle policy left conditioning active, no replacement
    // action may be selected on this evaluation. This is true both while
    // demand remains in-family and while a minimum-run gate is holding the
    // cycle, so conditioning state itself is the complete predicate.
    if matches!(state.action, EquipmentAction::Heat | EquipmentAction::Cool) {
        return finalize_outputs(
            state,
            previous_action,
            previous_outputs,
            deadline,
            now_ms,
            config,
        );
    }

    // The active-family case returned above, so the remaining exhaustive
    // EquipmentAction variants are Idle/FanOnly and are eligible for start
    // selection.
    let (next_action, block_reason, start_deadline) =
        choose_idle_action(&state, demand, now_ms, config);
    state.block_reason = block_reason;
    deadline = min_deadline(deadline, start_deadline);
    set_action(&mut state, next_action, now_ms);

    finalize_outputs(
        state,
        previous_action,
        previous_outputs,
        deadline,
        now_ms,
        config,
    )
}

fn apply_active_cycle_policy(
    state: &mut ControllerState,
    demand: Demand,
    now_ms: u64,
    config: ControllerConfig,
) -> Option<u64> {
    // Explicit mode changes that no longer permit the currently active family
    // stop it immediately. Minimum run time is a comfort/anti-short-cycle
    // policy, not authority to ignore a direct mode change.
    if !mode_allows_action(state.mode, state.action) {
        stop_active_conditioning(state, now_ms);
        return None;
    }

    match state.action {
        EquipmentAction::Heat => apply_running_family_policy(
            state,
            demand,
            Demand::Heat,
            Demand::Cool,
            config.timing.heating_min_run_ms,
            BlockReason::MinHeatRun,
            now_ms,
        ),
        EquipmentAction::Cool => apply_running_family_policy(
            state,
            demand,
            Demand::Cool,
            Demand::Heat,
            config.timing.cooling_min_run_ms,
            BlockReason::MinCoolRun,
            now_ms,
        ),
        EquipmentAction::Idle | EquipmentAction::FanOnly => None,
    }
}

fn apply_running_family_policy(
    state: &mut ControllerState,
    demand: Demand,
    same_family: Demand,
    opposite_family: Demand,
    minimum_run_ms: u64,
    minimum_run_reason: BlockReason,
    now_ms: u64,
) -> Option<u64> {
    if demand == same_family {
        return None;
    }

    // An actual opposite-family demand is stronger than the comfort-oriented
    // minimum-run preference. Stop first; changeover gates control the restart.
    if demand == opposite_family {
        stop_active_conditioning(state, now_ms);
        return None;
    }

    if let Some(deadline) =
        minimum_run_deadline(state.current_action_started_at_ms, minimum_run_ms, now_ms)
    {
        state.block_reason = minimum_run_reason;
        return Some(deadline);
    }

    stop_active_conditioning(state, now_ms);
    None
}

fn safe_transition(
    mut state: ControllerState,
    reason: BlockReason,
    phase_override: Option<Phase>,
    previous_action: EquipmentAction,
    previous_outputs: OutputVector,
    now_ms: u64,
) -> Transition {
    force_safe(&mut state, reason, now_ms);
    if let Some(phase) = phase_override {
        state.phase = phase;
    }
    finish(state, previous_action, previous_outputs, None, now_ms)
}

fn apply_event(state: &mut ControllerState, event: Event, now_ms: u64) {
    match event {
        Event::Tick => {}
        Event::SetMode(mode) => state.mode = mode,
        Event::SetFanMode(mode) => state.fan_mode = mode,
        Event::SetControlTemperature {
            value,
            valid_for_ms,
        } => {
            state.control_temperature = Some(ControlTemperature {
                value,
                valid_until_ms: now_ms.saturating_add(valid_for_ms),
            });
        }
        Event::ClearControlTemperature => state.control_temperature = None,
        Event::SetActuatorArmed(armed) => state.actuator_armed = armed,
        Event::RaiseCriticalFault => state.critical_fault = Some(CriticalFault::External),
        Event::ClearCriticalFault => state.critical_fault = None,
    }
}

fn calculate_demand(
    state: &ControllerState,
    temperature: MilliCelsius,
    config: ControllerConfig,
) -> Demand {
    let comfort = config.comfort;

    match state.mode {
        Mode::Off => Demand::None,
        Mode::Heat => {
            if state.action == EquipmentAction::Heat {
                if temperature >= comfort.heat_off {
                    Demand::None
                } else {
                    Demand::Heat
                }
            } else if temperature <= comfort.heat_on {
                Demand::Heat
            } else {
                Demand::None
            }
        }
        Mode::Cool => {
            if state.action == EquipmentAction::Cool {
                if temperature <= comfort.cool_off {
                    Demand::None
                } else {
                    Demand::Cool
                }
            } else if temperature >= comfort.cool_on {
                Demand::Cool
            } else {
                Demand::None
            }
        }
        Mode::Auto => match state.action {
            EquipmentAction::Heat => {
                if temperature >= comfort.cool_on {
                    Demand::Cool
                } else if temperature >= comfort.heat_off {
                    Demand::None
                } else {
                    Demand::Heat
                }
            }
            EquipmentAction::Cool => {
                if temperature <= comfort.heat_on {
                    Demand::Heat
                } else if temperature <= comfort.cool_off {
                    Demand::None
                } else {
                    Demand::Cool
                }
            }
            EquipmentAction::Idle | EquipmentAction::FanOnly => {
                if temperature <= comfort.heat_on {
                    Demand::Heat
                } else if temperature >= comfort.cool_on {
                    Demand::Cool
                } else {
                    Demand::None
                }
            }
        },
    }
}

fn choose_idle_action(
    state: &ControllerState,
    demand: Demand,
    now_ms: u64,
    config: ControllerConfig,
) -> (EquipmentAction, BlockReason, Option<u64>) {
    match demand {
        Demand::None => (fan_idle_action(state.fan_mode), BlockReason::None, None),
        Demand::Heat => {
            let mut deadline = None;
            let mut reason = BlockReason::None;

            if let Some(until) = startup_deadline(
                state.booted_at_ms,
                config.timing.heating_startup_lockout_ms,
                now_ms,
            ) {
                accumulate_block(
                    &mut deadline,
                    &mut reason,
                    until,
                    BlockReason::StartupLockout,
                );
            }

            if let Some(until) = stopped_deadline(
                state.last_heat_stopped_at_ms,
                config.timing.heating_min_off_ms,
                now_ms,
            ) {
                accumulate_block(&mut deadline, &mut reason, until, BlockReason::MinHeatOff);
            }

            if let Some(until) = opposite_changeover_deadline(
                state,
                EquipmentFamily::Heat,
                config.timing.changeover_ms,
                now_ms,
            ) {
                accumulate_block(
                    &mut deadline,
                    &mut reason,
                    until,
                    BlockReason::ChangeoverDelay,
                );
            }

            if deadline.is_some() {
                (fan_idle_action(state.fan_mode), reason, deadline)
            } else {
                (EquipmentAction::Heat, BlockReason::None, None)
            }
        }
        Demand::Cool => {
            let mut deadline = None;
            let mut reason = BlockReason::None;

            if let Some(until) = startup_deadline(
                state.booted_at_ms,
                config.timing.cooling_startup_lockout_ms,
                now_ms,
            ) {
                accumulate_block(
                    &mut deadline,
                    &mut reason,
                    until,
                    BlockReason::StartupLockout,
                );
            }

            if let Some(until) = stopped_deadline(
                state.last_cool_stopped_at_ms,
                config.timing.cooling_min_off_ms,
                now_ms,
            ) {
                accumulate_block(&mut deadline, &mut reason, until, BlockReason::MinCoolOff);
            }

            if let Some(until) = opposite_changeover_deadline(
                state,
                EquipmentFamily::Cool,
                config.timing.changeover_ms,
                now_ms,
            ) {
                accumulate_block(
                    &mut deadline,
                    &mut reason,
                    until,
                    BlockReason::ChangeoverDelay,
                );
            }

            if deadline.is_some() {
                (fan_idle_action(state.fan_mode), reason, deadline)
            } else {
                (EquipmentAction::Cool, BlockReason::None, None)
            }
        }
    }
}

fn mode_allows_action(mode: Mode, action: EquipmentAction) -> bool {
    match action {
        EquipmentAction::Idle | EquipmentAction::FanOnly => true,
        EquipmentAction::Heat => matches!(mode, Mode::Heat | Mode::Auto),
        EquipmentAction::Cool => matches!(mode, Mode::Cool | Mode::Auto),
    }
}

fn demand_is_opposite(action: EquipmentAction, demand: Demand) -> bool {
    matches!(
        (action, demand),
        (EquipmentAction::Heat, Demand::Cool) | (EquipmentAction::Cool, Demand::Heat)
    )
}

fn family_reversal_break(
    state: &mut ControllerState,
    previous_action: EquipmentAction,
    demand: Demand,
    now_ms: u64,
    config: ControllerConfig,
) -> Option<(BlockReason, u64)> {
    if !demand_is_opposite(previous_action, demand) {
        return None;
    }

    // `apply_active_cycle_policy` normally performs this stop first. Keep the
    // break operation defensive here as well: if a future refactor changes
    // that ordering, an opposite-family demand still cannot leave the old
    // conditioning family energized while the reversal is being scheduled.
    if matches!(state.action, EquipmentAction::Heat | EquipmentAction::Cool) {
        stop_active_conditioning(state, now_ms);
    }

    // Family reversals are always break-before-make. Even with a configured
    // zero-duration changeover, emit an observable idle/fan-only transition
    // and schedule a reevaluation one monotonic millisecond later rather than
    // transforming a heat vector directly into a cool vector in one commit.
    let (_, block_reason, start_deadline) = choose_idle_action(state, demand, now_ms, config);
    Some(match start_deadline {
        Some(deadline) => (block_reason, deadline),
        None => (BlockReason::ChangeoverDelay, now_ms.saturating_add(1)),
    })
}

fn fan_idle_action(fan_mode: FanMode) -> EquipmentAction {
    match fan_mode {
        FanMode::Auto => EquipmentAction::Idle,
        FanMode::On => EquipmentAction::FanOnly,
    }
}

fn set_action(state: &mut ControllerState, action: EquipmentAction, now_ms: u64) {
    if state.action == action {
        state.phase = phase_for(action);
        return;
    }

    if matches!(state.action, EquipmentAction::Heat | EquipmentAction::Cool) {
        stop_active_conditioning(state, now_ms);
    }

    state.action = action;
    state.phase = phase_for(action);
    state.current_action_started_at_ms = match action {
        EquipmentAction::Heat | EquipmentAction::Cool => Some(now_ms),
        EquipmentAction::Idle | EquipmentAction::FanOnly => None,
    };
}

fn stop_active_conditioning(state: &mut ControllerState, now_ms: u64) {
    match state.action {
        EquipmentAction::Heat => {
            state.last_heat_stopped_at_ms = Some(now_ms);
            state.last_family_stopped = Some((EquipmentFamily::Heat, now_ms));
        }
        EquipmentAction::Cool => {
            state.last_cool_stopped_at_ms = Some(now_ms);
            state.last_family_stopped = Some((EquipmentFamily::Cool, now_ms));
        }
        EquipmentAction::Idle | EquipmentAction::FanOnly => return,
    }

    state.action = fan_idle_action(state.fan_mode);
    state.phase = phase_for(state.action);
    state.current_action_started_at_ms = None;
    state.block_reason = BlockReason::None;
}

fn phase_for(action: EquipmentAction) -> Phase {
    match action {
        EquipmentAction::Idle => Phase::Idle,
        EquipmentAction::Heat => Phase::Heating,
        EquipmentAction::Cool => Phase::Cooling,
        EquipmentAction::FanOnly => Phase::FanOnly,
    }
}

fn force_safe(state: &mut ControllerState, reason: BlockReason, safe_stop_ms: u64) {
    stop_active_conditioning(state, safe_stop_ms);
    state.phase = Phase::FaultSafe;
    state.demand = Demand::None;
    state.action = EquipmentAction::Idle;
    state.outputs = OutputVector::OFF;
    state.block_reason = reason;
    state.current_action_started_at_ms = None;
}

fn minimum_run_deadline(started_at: Option<u64>, minimum_ms: u64, now_ms: u64) -> Option<u64> {
    let started_at = started_at?;
    let deadline = started_at.saturating_add(minimum_ms);
    (now_ms < deadline).then_some(deadline)
}

fn startup_deadline(booted_at: u64, duration_ms: u64, now_ms: u64) -> Option<u64> {
    let deadline = booted_at.saturating_add(duration_ms);
    (now_ms < deadline).then_some(deadline)
}

fn stopped_deadline(stopped_at: Option<u64>, minimum_ms: u64, now_ms: u64) -> Option<u64> {
    let stopped_at = stopped_at?;
    let deadline = stopped_at.saturating_add(minimum_ms);
    (now_ms < deadline).then_some(deadline)
}

fn sensor_expiry_deadline(state: &ControllerState) -> Option<u64> {
    let valid_until = state.control_temperature?.valid_until_ms;
    // A sample is defined as valid through `valid_until_ms` inclusively. The
    // first instant at which its validity can change is therefore +1 ms. A
    // saturated `u64::MAX` validity has no representable expiry deadline and
    // must not schedule an already-due timer that could spin the event loop.
    valid_until.checked_add(1)
}

fn opposite_changeover_deadline(
    state: &ControllerState,
    requested: EquipmentFamily,
    changeover_ms: u64,
    now_ms: u64,
) -> Option<u64> {
    let (last_family, stopped_at) = state.last_family_stopped?;
    if last_family == requested {
        return None;
    }
    let deadline = stopped_at.saturating_add(changeover_ms);
    (now_ms < deadline).then_some(deadline)
}

fn apply_fan_override(
    action: EquipmentAction,
    fan_mode: FanMode,
    mut outputs: OutputVector,
    config: ControllerConfig,
) -> OutputVector {
    if fan_mode != FanMode::On {
        return outputs;
    }

    match action {
        EquipmentAction::Idle | EquipmentAction::FanOnly | EquipmentAction::Cool => {
            outputs.g = true;
        }
        EquipmentAction::Heat if config.equipment.allow_continuous_fan_with_heat => {
            outputs.g = true;
        }
        EquipmentAction::Heat => {}
    }
    outputs
}

fn finalize_outputs(
    mut state: ControllerState,
    previous_action: EquipmentAction,
    previous_outputs: OutputVector,
    next_deadline_ms: Option<u64>,
    now_ms: u64,
    config: ControllerConfig,
) -> Transition {
    let base = config.equipment.outputs_for(state.action);
    let outputs = apply_fan_override(state.action, state.fan_mode, base, config);

    if validate_output_vector(outputs).is_err() {
        state.critical_fault = Some(CriticalFault::OutputInvariantViolation);
        force_safe(&mut state, BlockReason::CriticalFault, now_ms);
        return finish(state, previous_action, previous_outputs, None, now_ms);
    }

    state.outputs = outputs;
    finish(
        state,
        previous_action,
        previous_outputs,
        next_deadline_ms,
        now_ms,
    )
}

fn finish(
    state: ControllerState,
    previous_action: EquipmentAction,
    previous_outputs: OutputVector,
    next_deadline_ms: Option<u64>,
    _now_ms: u64,
) -> Transition {
    Transition {
        previous_action,
        next_action: state.action,
        previous_outputs,
        next_outputs: state.outputs,
        block_reason: state.block_reason,
        next_deadline_ms,
        state,
    }
}

fn min_deadline(a: Option<u64>, b: Option<u64>) -> Option<u64> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

fn accumulate_block(
    deadline: &mut Option<u64>,
    reason: &mut BlockReason,
    candidate_deadline: u64,
    candidate_reason: BlockReason,
) {
    if deadline.is_none_or(|current| candidate_deadline >= current) {
        *deadline = Some(candidate_deadline);
        *reason = candidate_reason;
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use thermostat_model::{ComfortConfig, EquipmentProfile, TimingConfig};

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

    fn arm_with_temp(mut state: ControllerState, now: u64, temp: MilliCelsius) -> ControllerState {
        state = step(
            state,
            Event::SetControlTemperature {
                value: temp,
                valid_for_ms: 3_600_000,
            },
            now,
            config(),
        )
        .state;
        step(state, Event::SetActuatorArmed(true), now, config()).state
    }

    #[test]
    fn boot_is_deenergized() {
        let state = ControllerState::new(0);
        assert_eq!(state.outputs, OutputVector::OFF);
        assert_eq!(state.phase, Phase::BootSafe);
    }

    #[test]
    fn goodman_heat_call_is_w_without_g_in_fan_auto() {
        let state = arm_with_temp(ControllerState::new(0), 0, mc(19_000));
        let transition = step(state, Event::SetMode(Mode::Heat), 1, config());
        assert_eq!(transition.next_action, EquipmentAction::Heat);
        assert_eq!(
            transition.next_outputs,
            OutputVector {
                w: true,
                y: false,
                g: false
            }
        );
    }

    #[test]
    fn cooling_waits_for_startup_lockout() {
        let state = arm_with_temp(ControllerState::new(0), 0, mc(24_000));
        let transition = step(state, Event::SetMode(Mode::Cool), 1, config());
        assert_eq!(transition.next_action, EquipmentAction::Idle);
        assert_eq!(transition.block_reason, BlockReason::StartupLockout);
        assert_eq!(transition.next_deadline_ms, Some(300_000));

        let transition = step(transition.state, Event::Tick, 300_000, config());
        assert_eq!(transition.next_action, EquipmentAction::Cool);
        assert_eq!(
            transition.next_outputs,
            OutputVector {
                w: false,
                y: true,
                g: true
            }
        );
    }

    #[test]
    fn user_off_stops_cooling_even_inside_minimum_run() {
        let state = arm_with_temp(ControllerState::new(0), 0, mc(24_000));
        let state = step(state, Event::SetMode(Mode::Cool), 300_000, config()).state;
        assert_eq!(state.action, EquipmentAction::Cool);

        let transition = step(state, Event::SetMode(Mode::Off), 300_001, config());
        assert_eq!(transition.next_action, EquipmentAction::Idle);
        assert_eq!(transition.next_outputs, OutputVector::OFF);
        assert_eq!(transition.state.last_cool_stopped_at_ms, Some(300_001));
    }

    #[test]
    fn setpoint_satisfaction_respects_cooling_minimum_run() {
        let state = arm_with_temp(ControllerState::new(0), 0, mc(24_000));
        let state = step(state, Event::SetMode(Mode::Cool), 300_000, config()).state;

        let transition = step(
            state,
            Event::SetControlTemperature {
                value: mc(22_000),
                valid_for_ms: 1_000_000,
            },
            300_001,
            config(),
        );
        assert_eq!(transition.next_action, EquipmentAction::Cool);
        assert_eq!(transition.block_reason, BlockReason::MinCoolRun);
        assert_eq!(transition.next_deadline_ms, Some(480_000));
    }

    #[test]
    fn restart_reapplies_full_compressor_lockout() {
        let restarted = arm_with_temp(ControllerState::new(1_000_000), 1_000_000, mc(24_000));
        let transition = step(restarted, Event::SetMode(Mode::Cool), 1_000_001, config());
        assert_eq!(transition.block_reason, BlockReason::StartupLockout);
        assert_eq!(transition.next_deadline_ms, Some(1_300_000));
        assert!(!transition.next_outputs.y);
    }

    #[test]
    fn heat_to_cool_passes_through_changeover_wait() {
        let mut cfg = config();
        cfg.timing.heating_min_run_ms = 0;
        let state = arm_with_temp(ControllerState::new(0), 0, mc(19_000));
        let state = step(state, Event::SetMode(Mode::Heat), 1, cfg).state;
        assert_eq!(state.action, EquipmentAction::Heat);

        let state = step(
            state,
            Event::SetControlTemperature {
                value: mc(24_000),
                valid_for_ms: 1_000_000,
            },
            10_000,
            cfg,
        )
        .state;
        let transition = step(state, Event::SetMode(Mode::Cool), 10_001, cfg);
        assert_ne!(transition.next_action, EquipmentAction::Cool);
        assert_eq!(transition.block_reason, BlockReason::ChangeoverDelay);
        assert_eq!(transition.next_outputs, OutputVector::OFF);
    }

    #[test]
    fn sensor_expiry_ends_active_conditioning() {
        let mut state = ControllerState::new(0);
        state = step(
            state,
            Event::SetControlTemperature {
                value: mc(19_000),
                valid_for_ms: 10_000,
            },
            0,
            config(),
        )
        .state;
        state = step(state, Event::SetActuatorArmed(true), 0, config()).state;
        state = step(state, Event::SetMode(Mode::Heat), 1, config()).state;
        assert_eq!(state.action, EquipmentAction::Heat);

        let transition = step(state, Event::Tick, 10_001, config());
        assert_eq!(transition.next_outputs, OutputVector::OFF);
        assert_eq!(transition.block_reason, BlockReason::SensorInvalid);
    }

    #[test]
    fn off_plus_fan_on_produces_only_g() {
        let state = ControllerState::new(0);
        let state = step(state, Event::SetActuatorArmed(true), 0, config()).state;
        let transition = step(state, Event::SetFanMode(FanMode::On), 1, config());
        assert_eq!(transition.next_action, EquipmentAction::FanOnly);
        assert_eq!(
            transition.next_outputs,
            OutputVector {
                w: false,
                y: false,
                g: true
            }
        );
    }

    #[test]
    fn backwards_monotonic_time_fails_safe() {
        let state = arm_with_temp(ControllerState::new(100), 100, mc(19_000));
        let state = step(state, Event::SetMode(Mode::Heat), 101, config()).state;
        assert!(state.outputs.w);

        let transition = step(state, Event::Tick, 99, config());
        assert_eq!(transition.next_outputs, OutputVector::OFF);
        assert_eq!(transition.block_reason, BlockReason::NonMonotonicTime);
        assert_eq!(
            transition.state.critical_fault,
            Some(CriticalFault::NonMonotonicTime)
        );
    }

    proptest! {
        #[test]
        fn arbitrary_event_sequences_never_energize_w_and_y_together(
            events in prop::collection::vec((0u8..7, -30_000i32..60_000, 0u64..120_000), 1..500)
        ) {
            let cfg = config();
            let mut state = ControllerState::new(0);
            let mut now = 0u64;

            for (kind, temp_raw, delta) in events {
                now = now.saturating_add(delta);
                let event = match kind {
                    0 => Event::Tick,
                    1 => Event::SetMode(Mode::Off),
                    2 => Event::SetMode(Mode::Heat),
                    3 => Event::SetMode(Mode::Cool),
                    4 => Event::SetMode(Mode::Auto),
                    5 => Event::SetActuatorArmed(true),
                    _ => Event::SetControlTemperature {
                        value: MilliCelsius::from_milli_celsius(temp_raw)
                            .unwrap_or(MilliCelsius::MIN),
                        valid_for_ms: 300_000,
                    },
                };

                state = step(state, event, now, cfg).state;
                prop_assert!(!(state.outputs.w && state.outputs.y));
            }
        }
    }
}
