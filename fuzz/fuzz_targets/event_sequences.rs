#![no_main]

use libfuzzer_sys::fuzz_target;
use thermostat_core::{
    ComfortConfig, ControllerConfig, ControllerState, EquipmentProfile, Event, FanMode,
    MilliCelsius, Mode, TimingConfig, step, validate_state,
};

fuzz_target!(|data: &[u8]| {
    let cfg = config();
    let mut state = ControllerState::new(0);
    let mut now = 0_u64;

    for chunk in data.chunks(4) {
        if chunk.len() < 4 {
            break;
        }
        let delta = u64::from(chunk[2]).saturating_mul(1_000);
        now = if chunk[3] & 0x80 == 0 {
            now.saturating_add(delta)
        } else {
            now.saturating_sub(delta)
        };

        let raw_temperature = i32::from(i16::from_le_bytes([chunk[1], chunk[2]]));
        let raw_temperature = raw_temperature.clamp(-30_000, 60_000);
        let temperature =
            MilliCelsius::from_milli_celsius(raw_temperature).unwrap_or(MilliCelsius::MIN);
        let event = match chunk[0] % 13 {
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
            11 => Event::ClearCriticalFault,
            _ => Event::SetControlTemperature {
                value: temperature,
                valid_for_ms: u64::from(chunk[3]).saturating_mul(10_000),
            },
        };

        let transition = step(state, event, now, cfg);
        assert!(validate_state(&transition.state).is_ok());
        assert!(!(transition.previous_outputs.w && transition.next_outputs.y));
        assert!(!(transition.previous_outputs.y && transition.next_outputs.w));
        state = transition.state;
    }
});

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

const fn mc(value: i32) -> MilliCelsius {
    match MilliCelsius::from_milli_celsius(value) {
        Some(value) => value,
        None => panic!("fixed fuzz configuration temperature is valid"),
    }
}
