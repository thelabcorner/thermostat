use proptest::prelude::*;
use thermostat_core::{
    ComfortConfig, ControllerConfig, ControllerState, Demand, EquipmentAction, EquipmentProfile,
    Event, FanMode, MilliCelsius, Mode, OutputVector, TimingConfig, step,
};

#[derive(Clone, Copy, Debug)]
enum Command {
    Mode(Mode),
    Fan(FanMode),
    Temperature(MilliCelsius),
    Tick,
}

#[derive(Clone, Copy, Debug)]
struct ReferenceModel {
    mode: Mode,
    fan_mode: FanMode,
    temperature: MilliCelsius,
    demand: Demand,
    action: EquipmentAction,
    break_before_make: bool,
}

impl ReferenceModel {
    const fn new(temperature: MilliCelsius) -> Self {
        Self {
            mode: Mode::Off,
            fan_mode: FanMode::Auto,
            temperature,
            demand: Demand::None,
            action: EquipmentAction::Idle,
            break_before_make: false,
        }
    }

    fn apply(&mut self, command: Command, comfort: ComfortConfig) {
        match command {
            Command::Mode(mode) => self.mode = mode,
            Command::Fan(fan_mode) => self.fan_mode = fan_mode,
            Command::Temperature(temperature) => self.temperature = temperature,
            Command::Tick => {}
        }

        if self.mode == Mode::Off {
            self.demand = Demand::None;
            self.action = idle_action(self.fan_mode);
            self.break_before_make = false;
            return;
        }

        self.demand = reference_demand(self.mode, self.action, self.temperature, comfort);

        let prior = self.action;
        if !mode_allows(self.mode, prior)
            || demand_is_opposite(prior, self.demand)
            || (matches!(prior, EquipmentAction::Heat | EquipmentAction::Cool)
                && self.demand == Demand::None)
        {
            self.action = idle_action(self.fan_mode);
        }

        if demand_is_opposite(prior, self.demand) && prior != self.action {
            self.break_before_make = true;
            return;
        }

        if self.break_before_make {
            self.break_before_make = false;
        }

        if matches!(
            self.action,
            EquipmentAction::Idle | EquipmentAction::FanOnly
        ) {
            self.action = match self.demand {
                Demand::None => idle_action(self.fan_mode),
                Demand::Heat => EquipmentAction::Heat,
                Demand::Cool => EquipmentAction::Cool,
            };
        }
    }

    fn outputs(self) -> OutputVector {
        match self.action {
            EquipmentAction::Idle => OutputVector::OFF,
            EquipmentAction::Heat => OutputVector {
                w: true,
                y: false,
                g: self.fan_mode == FanMode::On,
            },
            EquipmentAction::Cool => OutputVector {
                w: false,
                y: true,
                g: true,
            },
            EquipmentAction::FanOnly => OutputVector {
                w: false,
                y: false,
                g: true,
            },
        }
    }
}

fn reference_demand(
    mode: Mode,
    action: EquipmentAction,
    temperature: MilliCelsius,
    comfort: ComfortConfig,
) -> Demand {
    match mode {
        Mode::Off => Demand::None,
        Mode::Heat => {
            if action == EquipmentAction::Heat {
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
            if action == EquipmentAction::Cool {
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
        Mode::Auto => match action {
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

fn mode_allows(mode: Mode, action: EquipmentAction) -> bool {
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

const fn idle_action(fan_mode: FanMode) -> EquipmentAction {
    match fan_mode {
        FanMode::Auto => EquipmentAction::Idle,
        FanMode::On => EquipmentAction::FanOnly,
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
            cooling_startup_lockout_ms: 0,
            cooling_min_off_ms: 0,
            cooling_min_run_ms: 0,
            heating_startup_lockout_ms: 0,
            heating_min_off_ms: 0,
            heating_min_run_ms: 0,
            changeover_ms: 0,
        },
        equipment: EquipmentProfile::goodman_gmss960804cnaa(),
    }
}

const fn mc(value: i32) -> MilliCelsius {
    match MilliCelsius::from_milli_celsius(value) {
        Some(value) => value,
        None => panic!("reference temperature out of range"),
    }
}

fn command_strategy() -> impl Strategy<Value = Command> {
    prop_oneof![
        Just(Command::Mode(Mode::Off)),
        Just(Command::Mode(Mode::Heat)),
        Just(Command::Mode(Mode::Cool)),
        Just(Command::Mode(Mode::Auto)),
        Just(Command::Fan(FanMode::Auto)),
        Just(Command::Fan(FanMode::On)),
        (-20_000_i32..50_000).prop_map(|raw| Command::Temperature(mc(raw))),
        Just(Command::Tick),
    ]
}

proptest! {
    // The reference-model property test is intentionally bounded because the
    // dedicated libFuzzer campaign explores much longer traces continuously.
    // Keeping the deterministic CI lane small makes mutation testing and local
    // feedback dramatically faster without duplicating fuzz work.
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn implementation_matches_independent_zero_timer_reference_model(
        commands in prop::collection::vec(command_strategy(), 1..512)
    ) {
        let cfg = config();
        let initial_temperature = mc(21_500);
        let mut state = ControllerState::new(0);
        state = step(
            state,
            Event::SetControlTemperature {
                value: initial_temperature,
                valid_for_ms: u64::MAX / 2,
            },
            0,
            cfg,
        ).state;
        state = step(state, Event::SetActuatorArmed(true), 0, cfg).state;
        let mut reference = ReferenceModel::new(initial_temperature);
        let mut now = 0_u64;

        for command in commands {
            now = now.saturating_add(1);
            let event = match command {
                Command::Mode(mode) => Event::SetMode(mode),
                Command::Fan(fan_mode) => Event::SetFanMode(fan_mode),
                Command::Temperature(value) => Event::SetControlTemperature {
                    value,
                    valid_for_ms: u64::MAX / 2,
                },
                Command::Tick => Event::Tick,
            };

            reference.apply(command, cfg.comfort);
            let transition = step(state, event, now, cfg);
            prop_assert_eq!(transition.next_action, reference.action);
            prop_assert_eq!(transition.state.demand, reference.demand);
            prop_assert_eq!(transition.next_outputs, reference.outputs());
            state = transition.state;
        }
    }
}
