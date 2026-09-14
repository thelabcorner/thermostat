//! Low-overhead, dependency-free microbenchmark harness for the pure controller.
//!
//! The harness deliberately lives outside `thermostat-core` so benchmark
//! instrumentation cannot leak into the safety-critical library. Each workload
//! is warmed up, then measured in independent batches. The report includes
//! median, mean, standard deviation, coefficient of variation, p95, and p99.

use std::hint::black_box;
use std::time::Instant;

use thermostat_core::{
    ComfortConfig, ControllerConfig, ControllerState, EquipmentProfile, Event, FanMode,
    MilliCelsius, Mode, TimingConfig, step,
};

const WARMUP_ITERATIONS: usize = 200_000;
const SAMPLE_ITERATIONS: usize = 200_000;
const SAMPLES: usize = 40;

fn main() {
    println!("thermostat controller benchmark");
    println!("warmup={WARMUP_ITERATIONS} sample_iterations={SAMPLE_ITERATIONS} samples={SAMPLES}");

    run_benchmark("idle_tick", idle_tick);
    run_benchmark("active_heat_tick", active_heat_tick);
    run_benchmark("mixed_trace", mixed_trace);
}

#[allow(clippy::cast_precision_loss)]
fn run_benchmark(name: &str, workload: fn(usize) -> u64) {
    black_box(workload(WARMUP_ITERATIONS));

    let mut samples = Vec::with_capacity(SAMPLES);
    let mut checksum = 0_u64;
    for _ in 0..SAMPLES {
        let started = Instant::now();
        checksum ^= black_box(workload(SAMPLE_ITERATIONS));
        let elapsed = started.elapsed();
        samples.push(elapsed.as_secs_f64() * 1_000_000_000.0 / SAMPLE_ITERATIONS as f64);
    }

    samples.sort_by(f64::total_cmp);
    let mean = samples.iter().sum::<f64>() / samples.len() as f64;
    let variance = samples
        .iter()
        .map(|sample| {
            let delta = sample - mean;
            delta * delta
        })
        .sum::<f64>()
        / samples.len() as f64;
    let stddev = variance.sqrt();
    let cv = if mean == 0.0 {
        0.0
    } else {
        stddev / mean * 100.0
    };
    let median = percentile(&samples, 0.50);
    let p95 = percentile(&samples, 0.95);
    let p99 = percentile(&samples, 0.99);
    let throughput = 1_000_000_000.0 / median;

    println!(
        "{name}: median={median:.2} ns/op mean={mean:.2} ns/op stddev={stddev:.2} cv={cv:.2}% p95={p95:.2} p99={p99:.2} throughput={throughput:.0} transitions/s checksum={checksum}"
    );
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]
fn percentile(sorted: &[f64], quantile: f64) -> f64 {
    let max_index = sorted.len().saturating_sub(1);
    let index = (quantile * max_index as f64).round() as usize;
    sorted[index.min(max_index)]
}

fn idle_tick(iterations: usize) -> u64 {
    let cfg = config();
    let mut state = armed_state(21_000, Mode::Off, 0, cfg);
    let mut checksum = 0_u64;

    for index in 0..iterations {
        let now = index as u64 + 1;
        let transition = step(state, Event::Tick, now, cfg);
        checksum = checksum.wrapping_add(u64::from(transition.next_outputs.g));
        state = black_box(transition.state);
    }
    checksum
}

fn active_heat_tick(iterations: usize) -> u64 {
    let cfg = config();
    let mut state = armed_state(19_000, Mode::Heat, 0, cfg);
    let mut checksum = 0_u64;

    for index in 0..iterations {
        let now = index as u64 + 2;
        let transition = step(state, Event::Tick, now, cfg);
        checksum = checksum.wrapping_add(u64::from(transition.next_outputs.w));
        state = black_box(transition.state);
    }
    checksum
}

fn mixed_trace(iterations: usize) -> u64 {
    let cfg = config();
    let mut state = armed_state(21_000, Mode::Auto, 0, cfg);
    let mut now = 1_u64;
    let mut checksum = 0_u64;

    for index in 0..iterations {
        now = now.saturating_add(1_000);
        let event = match index & 63 {
            0 => Event::SetControlTemperature {
                value: mc(19_000),
                valid_for_ms: 600_000,
            },
            16 => Event::SetFanMode(FanMode::On),
            24 | 56 => Event::SetControlTemperature {
                value: mc(21_500),
                valid_for_ms: 600_000,
            },
            32 => Event::SetFanMode(FanMode::Auto),
            40 => Event::SetControlTemperature {
                value: mc(24_000),
                valid_for_ms: 600_000,
            },
            _ => Event::Tick,
        };
        let transition = step(state, event, now, cfg);
        checksum = checksum
            .wrapping_add(u64::from(transition.next_outputs.w))
            .wrapping_add(u64::from(transition.next_outputs.y) << 1)
            .wrapping_add(u64::from(transition.next_outputs.g) << 2);
        state = black_box(transition.state);
    }
    checksum
}

fn armed_state(
    temperature_milli_c: i32,
    mode: Mode,
    now_ms: u64,
    cfg: ControllerConfig,
) -> ControllerState {
    let state = ControllerState::new(now_ms);
    let state = step(
        state,
        Event::SetControlTemperature {
            value: mc(temperature_milli_c),
            valid_for_ms: u64::MAX / 2,
        },
        now_ms,
        cfg,
    )
    .state;
    let state = step(state, Event::SetActuatorArmed(true), now_ms, cfg).state;
    step(state, Event::SetMode(mode), now_ms, cfg).state
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

const fn mc(value: i32) -> MilliCelsius {
    match MilliCelsius::from_milli_celsius(value) {
        Some(value) => value,
        None => panic!("benchmark temperature must be in range"),
    }
}
