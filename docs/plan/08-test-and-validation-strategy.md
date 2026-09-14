# Test and Validation Strategy

## Philosophy

The hard thermostat bugs are temporal and combinatorial: restarts, mode changes, stale sensors, lockout boundaries, and conflicting events. We therefore test properties and sequences, not only individual functions.

## Test pyramid

### Layer 1: pure unit tests

Test:

- temperature/deadband calculations,
- configuration validation,
- action->output mapping,
- sensor freshness,
- remaining-time calculations,
- event application.

### Layer 2: table-driven transition tests

Build explicit matrices for:

- each user mode,
- each current equipment action,
- demand none/heat/cool,
- lockout expired/not expired,
- minimum-run expired/not expired,
- fan auto/on,
- sensor valid/invalid,
- user OFF during active cycles.

Each row asserts:

- next phase,
- block reason,
- semantic action,
- W/Y/G vector,
- next deadline.

### Layer 3: property-based tests

Use `proptest` to generate long random sequences of:

- temperatures,
- monotonic time deltas,
- mode changes,
- fan changes,
- sensor disconnects,
- config-valid inputs,
- daemon restart boundaries.

Properties include:

```text
never(W && Y)
startup => outputs_all_off
Y_start => prior_proven_off_duration >= cooling_min_off
heat_to_cool => observed_idle_interval
critical_fault => outputs_all_off
OFF => W=0 && Y=0
wall_clock_change => no effect on lockout deadline
```

### Layer 4: state-machine/model testing

Maintain a compact formal/reference model under `models/formal/`.

Candidate approaches:

- TLA+/PlusCal for temporal invariants and transition reachability,
- a small executable reference model for differential testing,
- both if the state space remains manageable.

The implementation is tested against model-generated valid traces where practical.

### Layer 5: fuzzing

Use `cargo-fuzz`/libFuzzer on:

- event decoder,
- configuration decoder/validator,
- state-machine event sequences,
- sensor packet parser,
- persistence migration inputs.

Fuzzer oracles include panics, invariant violations, impossible output vectors, and unbounded behavior.

### Layer 6: mutation testing

Use `cargo-mutants` against the safety core.

Mutations that **must** be killed include:

- removing cooling lockout check,
- changing `>=` to `>` at boundary semantics where relevant,
- inverting W/Y mapping,
- removing mutual-exclusion assertion,
- bypassing changeover wait,
- treating stale sensor as healthy,
- using wall time for a monotonic duration.

If a safety mutation survives, add a test before continuing.

### Layer 7: deterministic thermal simulation

Build a simple first-order multi-zone simulator before pursuing sophistication.

Base model:

```text
C * dT/dt = K_out * (T_out - T_in) + Q_hvac + Q_internal
```

Extend with room lag/noise only as needed.

Simulation goals:

- verify hysteresis behaves sensibly,
- quantify cycle frequency under varying deadbands,
- test extreme outdoor conditions,
- verify no heat/cool chatter in AUTO,
- test remote sensor faults,
- generate days/weeks of virtual operation quickly.

### Layer 8: restart/fault injection

At random points:

- kill controller,
- reconstruct state,
- corrupt optional telemetry write,
- drop MQTT,
- freeze a sensor,
- create outlier sensor,
- change wall time,
- deny GPIO write,
- remove actuator enable.

Expected result is deterministic and safe.

### Layer 9: hardware-in-loop

Stages:

1. mock GPIO backend,
2. physical GPIO + LEDs/logic analyzer,
3. relay board + continuity measurement,
4. isolated 24 VAC dummy circuit,
5. multiplexer bench,
6. HVAC board supervised commissioning.

## Boundary tests that receive explicit cases

- exactly at hysteresis threshold,
- one nanosecond/millisecond before timer expiration,
- exactly at expiration,
- immediately after expiration,
- mode OFF and ON at same logical timestamp/order variants,
- restart one instant after Y turns off,
- sensor expires exactly as lockout expires,
- opposite demand appears during minimum run,
- fan ON requested during cooling,
- fan ON while heat is equipment-owned,
- database failure during transition,
- actuator write fails after desired state changed but before confirmation.

## Actuator transactional behavior

Applying a vector should be treated as one logical effect.

If hardware writes cannot be atomic, order transitions to avoid dangerous intermediate vectors and test them explicitly.

For example, heat -> cool must never sequence through `W=1,Y=1`. Safe general strategy:

```text
de-energize outgoing calls
confirm/write success
wait required idle interval
energize incoming legal vector
```

## Performance testing

Thermostat throughput is tiny; performance work focuses on bounded latency and reliability rather than raw speed.

Measure:

- event-to-decision latency,
- actuator-command latency,
- startup-to-safe-ready latency,
- memory stability over long runs,
- SQLite write amplification,
- sensor burst backpressure behavior.

The safety core should remain fast enough that exhaustive/property tests can execute millions of transitions cheaply.

## Coverage gates

Coverage percentage alone is not a safety metric. Still, require high branch coverage in the safety core and combine it with mutation score.

Initial release gate proposal:

- 100% of declared invariants represented by automated tests,
- no known surviving safety-significant mutations,
- zero fuzz-discovered invariant violations over the campaign corpus,
- all modeled legal phases reachable by test,
- all illegal relay combinations unreachable.

### Current executable-verification snapshot — 2026-09-14

The implemented model/core/simulator surface currently has 63 Rust test functions. The final mutation campaigns caught every viable generated mutant: controller 103 caught / 6 unviable, model 44 / 4, and simulator 57 / 7. Post-change sanitizer-backed fuzz reruns completed 537,481 event-sequence inputs plus 5,469,320 configuration inputs with zero invariant failure or crash.

`cargo llvm-cov --workspace --exclude thermostat-bench` reports 97.24% line coverage overall. The controller is 98.39% line-covered with every function executed; the invariant checker is 100% line-covered. Remaining uncovered controller lines are defensive/unreachable paths under already-validated configuration or internal state ordering rather than untested public safety branches. Coverage remains supporting evidence rather than the release criterion by itself.

The compact TLA+ model has also been exhaustively checked by TLC over its deliberately bounded timer domain: 141 states generated, 43 distinct reachable states, state-graph depth 8, and no invariant error. This checks the abstraction; it does not replace implementation tests or prove the entire Rust program.

The release-mode microbenchmark campaign remains comfortably outside any control-latency concern. Final five-run median-of-medians are 37.93 ns/event (idle), 39.29 ns/event (active heat), and 40.42 ns/event (mixed trace). Variation within the benchmark samples is several percent, so only changes that clear that noise floor should be interpreted as performance movement.
