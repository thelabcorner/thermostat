# Architecture

## Architectural objective

Keep the safety-critical control surface small enough to reason about exhaustively while placing networking, UI, telemetry, and experimental optimization outside it.

## Proposed process architecture

```text
                     untrusted / optional inputs
             +---------------------------------------+
             | Web UI | schedules | weather | MQTT  |
             +-------------------+-------------------+
                                 |
                          high-level intents
                                 |
                                 v
                       +-------------------+
                       | thermostat API/UI |
                       | gateway process   |
                       +---------+---------+
                                 |
                            local IPC only
                                 |
                                 v
                 +----------------------------------+
                 | thermostatd                      |
                 |                                  |
 local sensor -->| sensor validation/fusion         |
 remote ingest ->| demand controller                |
                 | equipment safety gate            |
                 | equipment sequencer              |
                 | invariant checker                |
                 | event publisher                  |
                 +----------------+-----------------+
                                  |
                           actuator HAL trait
                                  |
                     +------------+------------+
                     |                         |
                     v                         v
              simulator backend          Linux GPIO backend
                                                |
                                                v
                                        isolated relays
```

## Core packages

### `thermostat-model`

Owns domain types with minimal dependencies:

- temperatures and units,
- mode/fan mode,
- equipment profile,
- timing configuration,
- sensor identities/health,
- validated output vector,
- fault/reason enums.

No I/O.

### `thermostat-core`

Pure deterministic transition engine.

Conceptual API:

```rust
next_state = reduce(previous_state, event, monotonic_now, validated_config)
```

The reducer does not read a clock itself, touch the filesystem, open sockets, or write GPIO.

Outputs include:

- new controller state,
- semantic transition/effects,
- desired equipment action,
- next interesting timer deadline.

### `thermostat-hal`

Defines narrow interfaces:

- `Actuator`
- `TemperatureSource`
- optional `MonotonicClock` only at daemon boundary

The actuator accepts a **validated output vector**, not individual arbitrary pin writes from callers.

### `thermostat-sim`

Contains:

- fake monotonic clock,
- virtual actuator,
- thermal-zone model,
- synthetic weather/load traces,
- sensor noise/fault models,
- scenario runner,
- trace replay.

### `thermostat-storage`

SQLite implementation for:

- validated configuration snapshots,
- sensor metadata/calibration,
- event log,
- temperature observations,
- cycle/runtime summaries,
- optimizer model data later.

Storage failure does not directly force an HVAC transition. Safety state resides in memory and safe restart rules do not depend on a successful last database write.

### `thermostat-gpio`

Linux-only backend using the modern GPIO character-device/libgpiod ecosystem.

Responsibilities:

- resolve configured GPIO line identifiers,
- claim lines exclusively,
- set known inactive levels before arming,
- support active-high/active-low coil inputs,
- atomically apply an output vector where the interface allows,
- read back logical requested state,
- fail closed on line/permission errors.

### `thermostatd`

Supervisory daemon:

- receives sensor observations and user intents,
- feeds the deterministic core,
- schedules monotonic deadlines,
- applies validated actuator effects,
- persists events asynchronously/best-effort,
- exposes narrow local IPC,
- participates in OS watchdog/health management.

## UI / API separation

The UI process has no GPIO group/device permissions.

Allowed intent examples:

```text
SetMode(HEAT)
SetFanMode(AUTO)
SetHeatTarget(20.5 C)
SetCoolTarget(23.0 C)
SelectComfortSensors([office, bedroom])
ApplyPreset(SLEEP)
```

Forbidden API concepts:

```text
SetGPIO(17, true)
SetRelay(Y, true)
ForceCompressorOn()
DisableCompressorLockout()
```

Installer-level configuration changes may modify equipment constraints, but only through validation and explicit re-arm/restart procedures.

## Synchronous vs asynchronous design

The control algorithm should remain synchronous and deterministic even if ingress is asynchronous.

A practical daemon loop can use an async runtime for IPC/MQTT/timers, but all events serialize through one controller task. We should avoid concurrent mutation of controller state.

```text
network tasks ----+
sensor tasks -----+--> bounded event queue --> single controller owner
timer task -------+
IPC intents ------+
```

Benefits:

- no lock ordering in the safety state machine,
- deterministic event ordering,
- easy record/replay,
- smaller concurrency test surface,
- straightforward invariants.

## Backpressure policy

Not every measurement is equal.

- Safety/user-mode events: never silently dropped.
- Timer deadlines: never silently dropped.
- Sensor telemetry: newest value may supersede older queued values from the same sensor.
- UI observation events: may be sampled/coalesced.

The event queue must be bounded so a broken sensor cannot allocate memory indefinitely.

## Time architecture

Two time domains are explicitly separate:

### Monotonic time

Used for:

- minimum on/off durations,
- compressor lockouts,
- changeover delay,
- sensor age within a running process,
- internal scheduling.

### Wall time

Used for:

- schedules,
- human timestamps,
- weather correlation,
- history display.

Changing wall time never alters an active equipment lockout.

## Configuration layers

1. **Compiled safe defaults**: all equipment disabled until configured.
2. **Equipment profile**: supported modes, action->output mapping, timing constraints.
3. **Installation mapping**: GPIO lines, relay polarity, sensor IDs.
4. **Comfort config**: setpoints, deadbands, schedules.
5. **Optimization config**: optional and lowest authority.

The controller can validate layers 1-3 before actuator arming.

## Dependency discipline

The safety core should remain dependency-light. Avoid coupling it to:

- web frameworks,
- MQTT libraries,
- SQLite,
- GPIO libraries,
- serialization formats,
- async runtime internals.

This keeps fuzzing/model tests fast and makes the core portable.

