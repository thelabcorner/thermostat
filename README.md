# Thermostat

Local-first, safety-oriented DIY residential HVAC controller for a conventional 24 VAC forced-air system.

This repository is being built around a real installation: a Goodman GMSS96-family single-stage natural-gas furnace, an external single-stage cooling system, four existing thermostat conductors, an Add-A-Wire-style Y/G multiplexer to recover a C conductor, a Raspberry Pi-class controller, isolated dry-contact relays, and distributed ESP32 + DS18B20 room sensors.

The project goal is not merely to recreate a commercial smart thermostat. The long-term target is a small local building-control platform with deterministic equipment protection, robust multi-room sensing, full observability, weather-aware optimization, and eventually a bounded learned thermal model. The critical equipment-control path remains deterministic even when higher-level automation is unavailable or wrong.

> **Core rule:** optimization may request comfort. Only the deterministic safety controller may actuate HVAC equipment.

## Status

**Phase 0: architecture and formal control planning.**

No GPIO or HVAC hardware is driven yet. The current repository intentionally begins with the control contract, equipment profile, failure model, state machine, and verification plan before actuator code is allowed to exist.

The detailed execution plan lives in [`docs/plan/`](docs/plan/README.md).

## Known installation

### Existing thermostat wiring

The existing wall run contains four conductors:

| Logical function | Conventional terminal | Existing installation |
| --- | --- | --- |
| 24 VAC source | `R` | red |
| Heat call | `W` | white |
| Cooling call | `Y` | yellow |
| Fan call | `G` | black conductor used as G |
| Common | `C` | not presently available at thermostat |

The intended retrofit uses an Add-A-Wire-style Y/G multiplexer. The current G conductor is repurposed as `C`; logical `Y` and `G` share the existing yellow conductor through the multiplexer/diode network. The software continues to model independent `Y` and `G` outputs. Multiplexing is a hardware transport detail.

### Furnace

The installed furnace label identifies:

- **Manufacturer:** Goodman
- **Model:** `GMSS960804CNAA`
- **Family:** GMSS96
- **Fuel:** natural gas
- **Architecture:** single-stage furnace with multi-speed blower
- **EnergyGuide AFUE shown on installed unit:** 96.1%

The GMSS96 control board is important to the thermostat design. For this furnace family the furnace itself owns the heat-cycle blower sequencing. The thermostat should call for heat with `W`; it should not blindly force `G` during a normal heat call. The furnace also implements its own blower post-run behavior.

For cooling, the GMSS96 documentation describes `Y + G` as the normal thermostat call and an internal cooling blower off-delay after the call ends. These equipment-owned delays must be respected rather than duplicated by arbitrary software timers.

The outdoor condenser model is not yet documented. Cooling-protection values therefore remain conservative until the actual condenser/compressor documentation is captured.

## System architecture

The system is deliberately layered so that networking, UI, weather services, databases, and optimization cannot bypass equipment safety.

```text
                         +--------------------------+
 ESP32 room sensors ---> | sensor ingest + health   |
 local thermostat sensor | validation + fusion      |
                         +------------+-------------+
                                      |
                                      v
                         +--------------------------+
                         | comfort controller       |
 schedules ------------> | setpoint / deadband      |
 optimizer ------------->| demand calculation      |
                         +------------+-------------+
                                      |
                                      v
                         +--------------------------+
                         | SAFETY GATE              |
                         | lockouts / min times     |
                         | changeover / startup     |
                         | sensor/fault policy      |
                         +------------+-------------+
                                      |
                                      v
                         +--------------------------+
                         | equipment sequencer      |
                         | equipment profile        |
                         +------------+-------------+
                                      |
                                      v
                         +--------------------------+
                         | output invariant checker |
                         | W/Y/G relay vector       |
                         +------------+-------------+
                                      |
                                      v
                         +--------------------------+
                         | actuator HAL             |
                         | simulator / Linux GPIO   |
                         +------------+-------------+
                                      |
                              isolated dry contacts
                                      |
                                      v
                                R -> W / Y / G
```

### Intended software split

The initial implementation plan uses:

- **Rust** for the deterministic controller, safety gate, state machine, simulator, GPIO HAL, persistence boundary, and daemon.
- **React + TypeScript** for the local UI once the control core is proven.
- **SQLite** for durable configuration, events, runtime history, and replayable telemetry.
- **ESP32 firmware** for distributed temperature nodes.
- **Local-only IPC/API** between the control daemon and higher-level UI/services.

The web/API process will never be given a primitive such as `setRelay(Y, true)`. It may submit intents such as `setMode(COOL)` or `setSetpoint(72°F)`. Only the controller may translate an intent into equipment outputs.

## Initial logical outputs

For the currently known Goodman gas-furnace installation, the default equipment profile is expected to resolve high-level actions as follows:

| Logical action | `W` | `Y` | `G` | Notes |
| --- | ---: | ---: | ---: | --- |
| Idle | 0 | 0 | 0 | All thermostat calls open |
| Heat | 1 | 0 | 0 | Furnace board owns heat blower sequence |
| Cool | 0 | 1 | 1 | Cooling + blower call |
| Fan only | 0 | 0 | 1 | Independent circulation |

These mappings are configuration, not hard-coded controller assumptions.

## Non-negotiable safety invariants

The implementation will treat these as executable properties, not prose-only guidelines:

1. `W` and `Y` must never be energized simultaneously for this single-stage conventional profile.
2. All HVAC calls are OFF at process startup and remain OFF until configuration and actuator polarity are validated.
3. Cooling cannot restart until the compressor minimum-off/startup lockout has elapsed.
4. A transition from heating to cooling, or cooling to heating, must pass through an idle/changeover interval.
5. Safety shutdown and explicit user OFF override comfort-driven minimum-runtime preferences.
6. No remote sensor, weather service, MQTT broker, UI, database write, or optimizer is allowed to directly actuate a relay.
7. Loss of remote sensors must not disable control while a trustworthy local sensor remains available.
8. Loss of all trustworthy control-temperature sources moves the controller to a defined fail-safe state.
9. Control timers use a monotonic time source. Wall-clock/NTP/DST changes must not shorten equipment lockouts.
10. On uncertain restart history, cooling receives the full conservative startup lockout rather than assuming the compressor has been off long enough.
11. Invalid configuration must fail closed: no equipment calls.
12. Relay polarity and GPIO mapping are validated before the actuator enable path is armed.

## Why the furnace fan is not manually delayed in software

An early design concept proposed starting `W`, waiting for the heat exchanger, then starting `G`, and keeping `G` running after `W` ends. That is not the correct default for this furnace.

The GMSS96 integrated control already performs the combustion sequence, blower-on sequencing, limit monitoring, and heat blower off-delay. The thermostat's normal heat responsibility is therefore the `W` call. Duplicating furnace-owned blower timing at the thermostat can fight the board's intended behavior.

Likewise, the furnace family documentation describes an internal cooling blower off-delay. We will characterize the actual installed control board before commissioning and encode only the responsibilities that truly belong to the thermostat.

## Power architecture

The intended controller is powered from the HVAC 24 VAC `R/C` supply after the common conductor is recovered.

Important electrical assumptions:

- `R` and `C` are the two sides of a nominal 24 VAC transformer secondary, not DC positive/negative.
- 24 VAC is an RMS value. After full-wave rectification and reservoir filtering, the unloaded DC bus can approach the low-to-mid 30 V range before transformer regulation and line tolerance are considered.
- Any AC/DC module therefore must be explicitly rated for the actual 24 VAC input environment and enough output power for the Pi, relay coils, and margin.
- The furnace transformer's VA rating must be measured/read from the installed transformer before the Pi power budget is accepted.
- HVAC relay contacts should be isolated dry contacts. The Pi-side electronics do not need to share logic ground with the 24 VAC control circuit.

The project will not assume a specific converter or relay board until exact hardware is selected and documented.

## Temperature sensing

The target sensing architecture is multi-source:

- one local temperature source at/near the thermostat controller,
- multiple ESP32 room nodes,
- DS18B20 sensors initially,
- per-sensor calibration, freshness, identity, and health tracking,
- robust fusion rather than a naive arithmetic mean,
- user-selectable room weighting / active comfort zones later.

Remote sensing improves control but is not allowed to become the sole path required for basic safe thermostat operation.

## Control philosophy

### V1: deterministic thermostat

The first production-capable controller will use conventional hysteresis/deadband behavior with explicit equipment timing constraints. A compressor is not a PWM actuator; PID/PWM is intentionally not the first control strategy.

The system separates:

- user mode (`OFF`, `HEAT`, `COOL`, `AUTO`),
- fan mode (`AUTO`, `ON`, later `CIRCULATE`),
- temperature demand (`NONE`, `HEAT`, `COOL`),
- equipment phase,
- safety block reason,
- physical output vector.

### V2: robust distributed sensing and schedules

Add room-level sensor fusion, occupancy/room selection, schedules, remote UI, event history, and energy/runtime statistics without changing the safety model.

### V3: bounded optimization

After sufficient measured data exists, add weather-aware preconditioning and a learned first-order building thermal model. Optimizers can propose future setpoints but remain downstream of hard equipment constraints.

## Verification strategy

No hardware backend is accepted until the exact same controller has passed:

- table-driven state-transition tests,
- property-based invariant testing,
- fuzz testing of events/configuration/restarts,
- mutation testing of safety checks,
- deterministic virtual-clock testing,
- multi-day/multi-season thermal simulation,
- fault injection for stale/bad/missing sensors,
- restart tests at arbitrary points in a heat/cool cycle,
- API authorization and invalid-command tests,
- relay-HAL simulation tests,
- bench testing with non-HVAC loads before 24 VAC equipment is connected.

The planned tooling includes Rust `proptest`, `cargo-fuzz`, `cargo-mutants`, and `cargo-llvm-cov`, plus a deterministic HVAC simulator. Formal/model-based verification of the compact safety state machine is also planned.

## Repository layout

The repository begins documentation-first. The intended implementation layout is:

```text
thermostat/
├── README.md
├── docs/
│   └── plan/
├── crates/
│   ├── thermostat-core/       # pure deterministic reducer/state machine
│   ├── thermostat-model/      # shared domain types/config validation
│   ├── thermostat-sim/        # thermal + equipment simulation
│   ├── thermostat-storage/    # SQLite event/history boundary
│   ├── thermostat-hal/        # actuator/sensor interfaces
│   ├── thermostat-gpio/       # Linux GPIO implementation
│   └── thermostatd/           # supervisor/control daemon
├── apps/
│   └── web/                   # local React/TypeScript UI
├── firmware/
│   └── esp32-sensor/          # remote temperature nodes
├── models/
│   └── formal/                # state-machine model/specification
└── tools/
    └── bench/                 # replay, fault injection, simulation tools
```

The exact crate split may be simplified during implementation if measured complexity does not justify a boundary. Safety boundaries are preserved even if package boundaries change.

## Plan index

Start here: [`docs/plan/README.md`](docs/plan/README.md)

- [Project charter](docs/plan/00-project-charter.md)
- [Requirements and invariants](docs/plan/01-requirements-and-invariants.md)
- [Architecture](docs/plan/02-architecture.md)
- [Control state machine](docs/plan/03-control-state-machine.md)
- [Installed equipment profile](docs/plan/04-equipment-profile-goodman-gmss960804cnaa.md)
- [Hardware interface and power](docs/plan/05-hardware-interface-and-power.md)
- [Sensor network](docs/plan/06-sensor-network.md)
- [Persistence, API, and observability](docs/plan/07-persistence-api-observability.md)
- [Testing and validation](docs/plan/08-test-and-validation-strategy.md)
- [Faults, recovery, and security](docs/plan/09-faults-recovery-security.md)
- [Roadmap and gates](docs/plan/10-roadmap.md)
- [Research notes and references](docs/plan/11-research-notes.md)

## Current unknowns to resolve before hardware commissioning

- Outdoor condenser/compressor model and manufacturer-required cycle constraints.
- Exact installed GMSS96 board revision and blower delay jumper/settings.
- Furnace 24 VAC transformer VA rating and measured loaded/unloaded secondary voltage.
- Exact Add-A-Wire/multiplexer make/model and its simultaneous `Y + G` behavior.
- Exact Raspberry Pi model.
- Exact 24 VAC to 5 VDC converter.
- Exact relay module, input polarity, contact ratings, isolation topology, and boot behavior.
- Local thermostat temperature sensor choice.
- GPIO numbering and physical connector design.

These are explicit commissioning gates, not details to guess in software.

## Research baseline

The architecture is informed by mature thermostat implementations and equipment documentation rather than invented in isolation. Primary references are maintained in [`docs/plan/11-research-notes.md`](docs/plan/11-research-notes.md), including:

- Goodman GMSS96 documentation,
- Venstar ACC0410 Add-A-Wire documentation,
- ESPHome thermostat controller behavior,
- Linux GPIO character-device/libgpiod interfaces,
- commercial thermostat compressor-protection conventions.

## License

Not selected yet. The project remains private/experimental until the implementation and hardware safety boundaries are mature enough to publish responsibly.

