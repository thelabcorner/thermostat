#![no_main]

use libfuzzer_sys::fuzz_target;
use thermostat_core::{
    ComfortConfig, ControllerConfig, ControllerState, EquipmentProfile, Event, MilliCelsius, Mode,
    OutputVector, TimingConfig, step, validate_state,
};

fuzz_target!(|data: &[u8]| {
    if data.len() < 20 {
        return;
    }

    let threshold = |offset: usize| {
        let raw = i16::from_le_bytes([data[offset], data[offset + 1]]);
        MilliCelsius::from_milli_celsius(i32::from(raw).clamp(-30_000, 60_000))
            .unwrap_or(MilliCelsius::MIN)
    };
    let duration = |offset: usize| {
        u64::from(u16::from_le_bytes([data[offset], data[offset + 1]])).saturating_mul(1_000)
    };
    let flags = data[18];
    let profile = EquipmentProfile {
        idle: OutputVector {
            w: flags & 0x01 != 0,
            y: flags & 0x02 != 0,
            g: flags & 0x04 != 0,
        },
        heat: OutputVector {
            w: flags & 0x08 != 0,
            y: flags & 0x10 != 0,
            g: data[19] & 0x01 != 0,
        },
        cool: OutputVector {
            w: data[19] & 0x02 != 0,
            y: data[19] & 0x04 != 0,
            g: data[19] & 0x08 != 0,
        },
        fan_only: OutputVector {
            w: data[19] & 0x10 != 0,
            y: data[19] & 0x20 != 0,
            g: data[19] & 0x40 != 0,
        },
        allow_continuous_fan_with_heat: data[19] & 0x80 != 0,
    };
    let cfg = ControllerConfig {
        comfort: ComfortConfig {
            heat_on: threshold(0),
            heat_off: threshold(2),
            cool_off: threshold(4),
            cool_on: threshold(6),
        },
        timing: TimingConfig {
            cooling_startup_lockout_ms: duration(8),
            cooling_min_off_ms: duration(10),
            cooling_min_run_ms: duration(12),
            heating_startup_lockout_ms: duration(14),
            heating_min_off_ms: duration(16),
            heating_min_run_ms: duration(8),
            changeover_ms: duration(10),
        },
        equipment: profile,
    };

    let valid = cfg.validate().is_ok();
    let state = ControllerState::new(0);
    let state = step(
        state,
        Event::SetControlTemperature {
            value: threshold(0),
            valid_for_ms: 600_000,
        },
        0,
        cfg,
    )
    .state;
    let state = step(state, Event::SetActuatorArmed(true), 0, cfg).state;
    let transition = step(state, Event::SetMode(Mode::Auto), 1, cfg);

    if valid {
        assert!(validate_state(&transition.state).is_ok());
    } else {
        assert_eq!(transition.next_outputs, OutputVector::OFF);
    }
});
