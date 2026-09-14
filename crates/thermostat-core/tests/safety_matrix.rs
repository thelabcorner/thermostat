use proptest::prelude::*;
use thermostat_core::{
    BlockReason, ComfortConfig, ControllerConfig, ControllerState, CriticalFault, Demand,
    EquipmentAction, EquipmentProfile, Event, FanMode, MilliCelsius, Mode, OutputVector, Phase,
    TimingConfig, step, validate_state,
};

const SENSOR_TTL_MS: u64 = 3_600_000;

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

fn zero_cycle_timers() -> ControllerConfig {
    let mut cfg = config();
    cfg.timing.cooling_startup_lockout_ms = 0;
    cfg.timing.cooling_min_off_ms = 0;
    cfg.timing.cooling_min_run_ms = 0;
    cfg.timing.heating_min_off_ms = 0;
    cfg.timing.heating_min_run_ms = 0;
    cfg.timing.changeover_ms = 0;
    cfg
}

fn armed_state(now_ms: u64, temperature: MilliCelsius, cfg: ControllerConfig) -> ControllerState {
    let state = ControllerState::new(now_ms);
    let state = step(
        state,
        Event::SetControlTemperature {
            value: temperature,
            valid_for_ms: SENSOR_TTL_MS,
        },
        now_ms,
        cfg,
    )
    .state;
    step(state, Event::SetActuatorArmed(true), now_ms, cfg).state
}

#[test]
fn hysteresis_thresholds_are_inclusive_at_declared_boundaries() {
    let cfg = zero_cycle_timers();

    let heat_at_boundary = step(
        armed_state(0, cfg.comfort.heat_on, cfg),
        Event::SetMode(Mode::Heat),
        1,
        cfg,
    );
    assert_eq!(heat_at_boundary.next_action, EquipmentAction::Heat);

    let heat_above_boundary = step(
        armed_state(0, mc(20_001), cfg),
        Event::SetMode(Mode::Heat),
        1,
        cfg,
    );
    assert_eq!(heat_above_boundary.next_action, EquipmentAction::Idle);

    let cool_at_boundary = step(
        armed_state(0, cfg.comfort.cool_on, cfg),
        Event::SetMode(Mode::Cool),
        1,
        cfg,
    );
    assert_eq!(cool_at_boundary.next_action, EquipmentAction::Cool);

    let cool_below_boundary = step(
        armed_state(0, mc(22_999), cfg),
        Event::SetMode(Mode::Cool),
        1,
        cfg,
    );
    assert_eq!(cool_below_boundary.next_action, EquipmentAction::Idle);
}

#[test]
fn running_cycle_stops_exactly_at_stop_threshold_when_min_run_is_zero() {
    let cfg = zero_cycle_timers();

    let heat = step(
        armed_state(0, mc(19_000), cfg),
        Event::SetMode(Mode::Heat),
        1,
        cfg,
    )
    .state;
    let heat_stop = step(
        heat,
        Event::SetControlTemperature {
            value: cfg.comfort.heat_off,
            valid_for_ms: SENSOR_TTL_MS,
        },
        2,
        cfg,
    );
    assert_eq!(heat_stop.next_action, EquipmentAction::Idle);
    assert_eq!(heat_stop.block_reason, BlockReason::None);

    let cool = step(
        armed_state(0, mc(24_000), cfg),
        Event::SetMode(Mode::Cool),
        1,
        cfg,
    )
    .state;
    let cool_stop = step(
        cool,
        Event::SetControlTemperature {
            value: cfg.comfort.cool_off,
            valid_for_ms: SENSOR_TTL_MS,
        },
        2,
        cfg,
    );
    assert_eq!(cool_stop.next_action, EquipmentAction::Idle);
    assert_eq!(cool_stop.block_reason, BlockReason::None);
}

#[test]
fn changed_outputs_detects_unchanged_and_single_contact_transitions() {
    let cfg = zero_cycle_timers();
    let state = armed_state(0, mc(24_000), cfg);

    let unchanged = step(state, Event::Tick, 1, cfg);
    assert!(!unchanged.changed_outputs());
    assert_eq!(unchanged.previous_outputs, unchanged.next_outputs);

    let fan_on = step(unchanged.state, Event::SetFanMode(FanMode::On), 2, cfg);
    assert_eq!(fan_on.previous_outputs, OutputVector::OFF);
    assert_eq!(
        fan_on.next_outputs,
        OutputVector {
            w: false,
            y: false,
            g: true,
        }
    );
    assert!(
        fan_on.changed_outputs(),
        "a G-only transition must count as changed"
    );

    let cooling = step(fan_on.state, Event::SetMode(Mode::Cool), 3, cfg);
    assert_eq!(cooling.previous_outputs.g, cooling.next_outputs.g);
    assert!(!cooling.previous_outputs.y && cooling.next_outputs.y);
    assert!(!cooling.previous_outputs.w && !cooling.next_outputs.w);
    assert!(
        cooling.changed_outputs(),
        "a Y-only transition must count as changed"
    );
}

#[test]
fn explicit_family_mode_change_stops_disallowed_heat_inside_minimum_run() {
    let cfg = config();
    let state = step(
        armed_state(0, mc(19_000), cfg),
        Event::SetMode(Mode::Heat),
        1,
        cfg,
    )
    .state;
    assert_eq!(state.action, EquipmentAction::Heat);

    // At 19 C, COOL mode has no cooling demand. The mode change itself still
    // revokes authority for W immediately; the heating minimum-run preference
    // is not allowed to keep an explicitly disallowed family energized.
    let stopped = step(state, Event::SetMode(Mode::Cool), 2, cfg);
    assert_eq!(stopped.next_action, EquipmentAction::Idle);
    assert_eq!(stopped.next_outputs, OutputVector::OFF);
    assert_eq!(stopped.state.last_heat_stopped_at_ms, Some(2));
    assert_ne!(stopped.block_reason, BlockReason::MinHeatRun);
}

#[test]
fn minimum_run_opens_exactly_at_its_deadline() {
    let cfg = config();
    let state = step(
        armed_state(0, mc(19_000), cfg),
        Event::SetMode(Mode::Heat),
        1,
        cfg,
    )
    .state;

    let stopped = step(
        state,
        Event::SetControlTemperature {
            value: cfg.comfort.heat_off,
            valid_for_ms: SENSOR_TTL_MS,
        },
        1 + cfg.timing.heating_min_run_ms,
        cfg,
    );
    assert_eq!(stopped.next_action, EquipmentAction::Idle);
    assert_eq!(stopped.block_reason, BlockReason::None);
}

#[test]
fn saturated_sensor_validity_has_no_representable_expiry_deadline() {
    let cfg = zero_cycle_timers();
    let state = ControllerState::new(10);
    let state = step(
        state,
        Event::SetControlTemperature {
            value: mc(19_000),
            valid_for_ms: u64::MAX,
        },
        10,
        cfg,
    )
    .state;
    let state = step(state, Event::SetActuatorArmed(true), 10, cfg).state;
    let running = step(state, Event::SetMode(Mode::Heat), 10, cfg);
    assert_eq!(running.next_action, EquipmentAction::Heat);
    assert_eq!(
        running
            .state
            .control_temperature
            .map(|sample| sample.valid_until_ms),
        Some(u64::MAX)
    );
    assert_eq!(running.next_deadline_ms, None);
}

#[test]
fn profile_can_forbid_continuous_fan_during_heat() {
    let mut cfg = zero_cycle_timers();
    cfg.equipment.allow_continuous_fan_with_heat = false;
    let state = armed_state(0, mc(19_000), cfg);
    let state = step(state, Event::SetFanMode(FanMode::On), 1, cfg).state;
    assert_eq!(state.action, EquipmentAction::FanOnly);

    let heating = step(state, Event::SetMode(Mode::Heat), 2, cfg);
    assert!(heating.next_outputs.w);
    assert!(!heating.next_outputs.y);
    assert!(!heating.next_outputs.g);
}

#[test]
fn opposite_family_never_switches_make_before_break_even_with_zero_changeover() {
    let cfg = zero_cycle_timers();
    let heat = step(
        armed_state(0, mc(19_000), cfg),
        Event::SetMode(Mode::Auto),
        1,
        cfg,
    )
    .state;
    assert!(heat.outputs.w);

    let break_transition = step(
        heat,
        Event::SetControlTemperature {
            value: mc(24_000),
            valid_for_ms: SENSOR_TTL_MS,
        },
        2,
        cfg,
    );
    assert!(!break_transition.next_outputs.w);
    assert!(!break_transition.next_outputs.y);
    assert_eq!(break_transition.block_reason, BlockReason::ChangeoverDelay);
    assert_eq!(break_transition.next_deadline_ms, Some(3));

    let make_transition = step(break_transition.state, Event::Tick, 3, cfg);
    assert!(!make_transition.previous_outputs.w);
    assert!(make_transition.next_outputs.y);
}

#[test]
fn cooling_startup_lockout_is_closed_one_ms_before_and_open_at_deadline() {
    let cfg = config();
    let state = armed_state(0, mc(24_000), cfg);
    let waiting = step(state, Event::SetMode(Mode::Cool), 299_999, cfg);
    assert_eq!(waiting.next_action, EquipmentAction::Idle);
    assert_eq!(waiting.block_reason, BlockReason::StartupLockout);
    assert_eq!(waiting.next_deadline_ms, Some(300_000));

    let started = step(waiting.state, Event::Tick, 300_000, cfg);
    assert_eq!(started.next_action, EquipmentAction::Cool);
    assert!(started.next_outputs.y);
}

#[test]
fn heating_startup_lockout_is_enforced_when_configured() {
    let mut cfg = zero_cycle_timers();
    cfg.timing.heating_startup_lockout_ms = 10_000;
    let state = armed_state(0, mc(19_000), cfg);

    let waiting = step(state, Event::SetMode(Mode::Heat), 9_999, cfg);
    assert_eq!(waiting.next_action, EquipmentAction::Idle);
    assert_eq!(waiting.block_reason, BlockReason::StartupLockout);
    assert_eq!(waiting.next_deadline_ms, Some(10_000));

    let started = step(waiting.state, Event::Tick, 10_000, cfg);
    assert_eq!(started.next_action, EquipmentAction::Heat);
    assert!(started.next_outputs.w);
}

#[test]
fn cooling_minimum_off_boundary_is_exact() {
    let mut cfg = config();
    cfg.timing.cooling_startup_lockout_ms = 0;
    cfg.timing.cooling_min_run_ms = 0;

    let state = armed_state(0, mc(24_000), cfg);
    let state = step(state, Event::SetMode(Mode::Cool), 1, cfg).state;
    let state = step(state, Event::SetMode(Mode::Off), 2, cfg).state;
    let state = step(
        state,
        Event::SetControlTemperature {
            value: mc(24_000),
            valid_for_ms: SENSOR_TTL_MS,
        },
        3,
        cfg,
    )
    .state;
    let state = step(state, Event::SetMode(Mode::Cool), 3, cfg).state;

    let before = step(state, Event::Tick, 300_001, cfg);
    assert_eq!(before.next_action, EquipmentAction::Idle);
    assert_eq!(before.block_reason, BlockReason::MinCoolOff);
    assert_eq!(before.next_deadline_ms, Some(300_002));

    let at = step(before.state, Event::Tick, 300_002, cfg);
    assert_eq!(at.next_action, EquipmentAction::Cool);
}

#[test]
fn sensor_deadline_is_exposed_and_expiry_has_higher_authority_than_min_run() {
    let cfg = config();
    let state = ControllerState::new(0);
    let state = step(
        state,
        Event::SetControlTemperature {
            value: mc(19_000),
            valid_for_ms: 10_000,
        },
        0,
        cfg,
    )
    .state;
    let state = step(state, Event::SetActuatorArmed(true), 0, cfg).state;
    let running = step(state, Event::SetMode(Mode::Heat), 1, cfg);
    assert_eq!(running.next_action, EquipmentAction::Heat);
    assert_eq!(running.next_deadline_ms, Some(10_001));

    let still_valid = step(running.state, Event::Tick, 10_000, cfg);
    assert_eq!(still_valid.next_action, EquipmentAction::Heat);
    assert_eq!(still_valid.next_deadline_ms, Some(10_001));

    let expired = step(still_valid.state, Event::Tick, 10_001, cfg);
    assert_eq!(expired.next_outputs, OutputVector::OFF);
    assert_eq!(expired.block_reason, BlockReason::SensorInvalid);
}

#[test]
fn invalid_configuration_fails_closed_even_from_running_state() {
    let cfg = zero_cycle_timers();
    let state = step(
        armed_state(0, mc(19_000), cfg),
        Event::SetMode(Mode::Heat),
        1,
        cfg,
    )
    .state;
    assert!(state.outputs.w);

    let mut invalid = cfg;
    invalid.equipment.cool.w = true;
    let transition = step(state, Event::Tick, 2, invalid);
    assert_eq!(transition.next_outputs, OutputVector::OFF);
    assert_eq!(transition.block_reason, BlockReason::ConfigInvalid);
    assert_eq!(transition.state.phase, Phase::FaultSafe);
}

#[test]
fn critical_fault_latches_safe_until_explicitly_cleared() {
    let cfg = zero_cycle_timers();
    let state = step(
        armed_state(0, mc(19_000), cfg),
        Event::SetMode(Mode::Heat),
        1,
        cfg,
    )
    .state;

    let faulted = step(state, Event::RaiseCriticalFault, 2, cfg);
    assert_eq!(faulted.next_outputs, OutputVector::OFF);
    assert_eq!(faulted.state.critical_fault, Some(CriticalFault::External));
    assert_eq!(faulted.block_reason, BlockReason::CriticalFault);

    let remains_faulted = step(faulted.state, Event::Tick, 3, cfg);
    assert_eq!(remains_faulted.next_outputs, OutputVector::OFF);

    let cleared = step(remains_faulted.state, Event::ClearCriticalFault, 4, cfg);
    assert_eq!(cleared.state.critical_fault, None);
    assert_eq!(cleared.next_action, EquipmentAction::Heat);
}

#[test]
fn disarming_actuator_while_running_immediately_deenergizes_every_contact() {
    let cfg = zero_cycle_timers();
    let state = step(
        armed_state(0, mc(19_000), cfg),
        Event::SetMode(Mode::Heat),
        1,
        cfg,
    )
    .state;
    assert!(state.outputs.w);

    let disarmed = step(state, Event::SetActuatorArmed(false), 2, cfg);
    assert_eq!(disarmed.next_outputs, OutputVector::OFF);
    assert_eq!(disarmed.block_reason, BlockReason::ActuatorNotArmed);
    assert_eq!(disarmed.state.phase, Phase::BootSafe);
}

#[test]
fn clearing_control_temperature_while_running_fails_safe_immediately() {
    let cfg = zero_cycle_timers();
    let state = step(
        armed_state(0, mc(24_000), cfg),
        Event::SetMode(Mode::Cool),
        1,
        cfg,
    )
    .state;
    assert!(state.outputs.y);

    let cleared = step(state, Event::ClearControlTemperature, 2, cfg);
    assert_eq!(cleared.next_outputs, OutputVector::OFF);
    assert_eq!(cleared.block_reason, BlockReason::SensorInvalid);
    assert_eq!(cleared.state.phase, Phase::FaultSafe);
}

#[test]
fn goodman_profile_allows_explicit_continuous_fan_during_heat() {
    let cfg = zero_cycle_timers();
    let state = armed_state(0, mc(19_000), cfg);
    let state = step(state, Event::SetFanMode(FanMode::On), 1, cfg).state;
    let heating = step(state, Event::SetMode(Mode::Heat), 2, cfg);

    assert!(heating.next_outputs.w);
    assert!(!heating.next_outputs.y);
    assert!(heating.next_outputs.g);
}

#[test]
fn longest_start_constraint_determines_block_reason_but_sensor_can_wake_earlier() {
    let mut cfg = config();
    cfg.timing.cooling_startup_lockout_ms = 100_000;
    cfg.timing.cooling_min_off_ms = 200_000;
    cfg.timing.changeover_ms = 300_000;
    cfg.timing.heating_min_run_ms = 0;

    let state = ControllerState::new(0);
    let state = step(
        state,
        Event::SetControlTemperature {
            value: mc(19_000),
            valid_for_ms: 150_000,
        },
        0,
        cfg,
    )
    .state;
    let state = step(state, Event::SetActuatorArmed(true), 0, cfg).state;
    let heat = step(state, Event::SetMode(Mode::Heat), 1, cfg).state;
    let idle = step(
        heat,
        Event::SetControlTemperature {
            value: mc(24_000),
            valid_for_ms: 149_999,
        },
        1_000,
        cfg,
    )
    .state;

    let cool_wait = step(idle, Event::SetMode(Mode::Cool), 1_001, cfg);
    assert_eq!(cool_wait.block_reason, BlockReason::ChangeoverDelay);
    // Changeover would end at 301_000, but the sensor must be revalidated at
    // 151_000 first, so the event loop receives the earlier wake-up.
    assert_eq!(cool_wait.next_deadline_ms, Some(151_000));
}

#[test]
fn transition_record_contains_replay_relevant_semantics() {
    let cfg = zero_cycle_timers();
    let state = armed_state(0, mc(19_000), cfg);
    let transition = step(state, Event::SetMode(Mode::Heat), 1, cfg);
    let record = transition.record(Event::SetMode(Mode::Heat), 1);

    assert_eq!(record.at_ms, 1);
    assert_eq!(record.event, Event::SetMode(Mode::Heat));
    assert_eq!(record.previous_action, EquipmentAction::Idle);
    assert_eq!(record.next_action, EquipmentAction::Heat);
    assert_eq!(record.next_phase, Phase::Heating);
    assert_eq!(record.next_demand, Demand::Heat);
    assert!(transition.changed_outputs());
}

#[test]
fn mode_fan_temperature_matrix_always_yields_a_valid_state() {
    let cfg = zero_cycle_timers();
    let modes = [Mode::Off, Mode::Heat, Mode::Cool, Mode::Auto];
    let fan_modes = [FanMode::Auto, FanMode::On];
    let temperatures = [mc(19_000), mc(20_000), mc(21_500), mc(23_000), mc(24_000)];

    for mode in modes {
        for fan_mode in fan_modes {
            for temperature in temperatures {
                let state = armed_state(0, temperature, cfg);
                let state = step(state, Event::SetFanMode(fan_mode), 1, cfg).state;
                let state = step(state, Event::SetMode(mode), 2, cfg).state;
                assert_eq!(
                    validate_state(&state),
                    Ok(()),
                    "{mode:?} {fan_mode:?} {temperature}"
                );
                if mode == Mode::Off {
                    assert!(!state.outputs.w && !state.outputs.y);
                }
            }
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn randomized_sequences_preserve_hard_state_invariants(
        events in prop::collection::vec((0u8..12, -50_000i32..80_000, 0u64..30_000), 1..1_000)
    ) {
        let cfg = config();
        let mut state = ControllerState::new(0);
        let mut now = 0_u64;

        for (kind, temp_raw, delta) in events {
            now = now.saturating_add(delta);
            let event = match kind {
                0 => Event::Tick,
                1 => Event::SetMode(Mode::Off),
                2 => Event::SetMode(Mode::Heat),
                3 => Event::SetMode(Mode::Cool),
                4 => Event::SetMode(Mode::Auto),
                5 => Event::SetFanMode(FanMode::Auto),
                6 => Event::SetFanMode(FanMode::On),
                7 => Event::SetActuatorArmed(true),
                8 => Event::SetActuatorArmed(false),
                9 => Event::ClearControlTemperature,
                10 => Event::RaiseCriticalFault,
                _ => Event::SetControlTemperature {
                    value: MilliCelsius::from_milli_celsius(temp_raw).unwrap_or(MilliCelsius::MIN),
                    valid_for_ms: delta.saturating_add(1),
                },
            };

            let transition = step(state, event, now, cfg);
            prop_assert_eq!(validate_state(&transition.state), Ok(()));
            prop_assert!(!(transition.previous_outputs.w && transition.next_outputs.y));
            prop_assert!(!(transition.previous_outputs.y && transition.next_outputs.w));
            state = transition.state;
        }
    }

    #[test]
    fn every_cooling_restart_respects_known_minimum_off_time(
        temperatures in prop::collection::vec(18_000i32..26_000, 1..500),
        deltas in prop::collection::vec(1_u64..120_000, 1..500),
    ) {
        let cfg = config();
        let mut state = ControllerState::new(0);
        let mut now = 0_u64;
        let mut last_y_off: Option<u64> = None;

        for (index, temp_raw) in temperatures.into_iter().enumerate() {
            now = now.saturating_add(deltas[index % deltas.len()]);
            let temp = MilliCelsius::from_milli_celsius(temp_raw).unwrap_or(mc(21_000));
            let event = if index == 0 {
                Event::SetActuatorArmed(true)
            } else if index == 1 {
                Event::SetMode(Mode::Auto)
            } else {
                Event::SetControlTemperature {
                    value: temp,
                    valid_for_ms: 600_000,
                }
            };

            let transition = step(state, event, now, cfg);
            if transition.previous_outputs.y && !transition.next_outputs.y {
                last_y_off = Some(now);
            }
            if !transition.previous_outputs.y && transition.next_outputs.y {
                prop_assert!(
                    now >= transition.state.booted_at_ms.saturating_add(cfg.timing.cooling_startup_lockout_ms)
                );
                if let Some(stopped_at) = last_y_off {
                    prop_assert!(now >= stopped_at.saturating_add(cfg.timing.cooling_min_off_ms));
                }
            }
            state = transition.state;
        }
    }
}
