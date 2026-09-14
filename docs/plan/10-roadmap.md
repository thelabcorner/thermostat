# Roadmap and Release Gates

## Phase 0 - Research and specification

**Goal:** define behavior before source code can accidentally become the specification.

Tasks:

- [x] Capture current furnace model from installed label.
- [x] Establish conventional `R/W/Y/G/C` conceptual model.
- [x] Document Add-A-Wire-style four-to-five-wire topology.
- [x] Research GMSS96 furnace fan ownership and cycle behavior.
- [x] Research existing thermostat state-machine concepts.
- [x] Write initial architecture/invariants/state-machine plan.
- [ ] Identify outdoor condenser exact model.
- [ ] Identify installed furnace control board revision and heat-delay setting.
- [ ] Identify furnace transformer VA rating.
- [ ] Select local control temperature sensor.

Gate P0:

- Documentation review shows no unresolved contradiction in control responsibilities.

## Phase 1 - Pure Rust domain model and safety core

**Goal:** a deterministic thermostat that cannot touch hardware.

Tasks:

- [ ] Create Cargo workspace.
- [ ] Implement units/domain types.
- [ ] Implement validated equipment profile.
- [ ] Implement setpoint/deadband demand calculation.
- [ ] Implement controller state + events.
- [ ] Implement monotonic timer semantics.
- [ ] Implement equipment action resolver.
- [ ] Implement output invariant checker.
- [ ] Implement transition event records.
- [ ] Add exhaustive table-driven tests.
- [ ] Add `proptest` properties.

Gate P1:

- Every declared invariant has executable tests.
- No I/O dependency exists in `thermostat-core`.

## Phase 2 - Formal/reference model + fuzz/mutation campaign

**Goal:** attack the controller before adding complexity.

Tasks:

- [ ] Create compact TLA+/PlusCal or equivalent transition model.
- [ ] Verify mutual exclusion and lockout/changeover reachability properties.
- [ ] Differential/reference-model tests where practical.
- [ ] `cargo-fuzz` event-sequence harness.
- [ ] configuration fuzz harness.
- [ ] `cargo-mutants` campaign.
- [ ] fix every surviving safety-significant mutation.

Gate P2:

- No known invariant violation.
- Safety mutation suite is effective.

## Phase 3 - Deterministic HVAC simulator

**Goal:** observe complete virtual days/seasons.

Tasks:

- [ ] fake monotonic clock,
- [ ] virtual relay actuator,
- [ ] first-order building thermal model,
- [ ] outdoor temperature profiles,
- [ ] sensor noise/offset/failure injection,
- [ ] multi-day scenario runner,
- [ ] restart injection,
- [ ] state trace export/replay.

Gate P3:

- Heat/cool/AUTO behavior stable across extreme simulated conditions.
- No chattering or illegal transitions in stress corpus.

## Phase 4 - Storage and local sensor

**Goal:** standalone thermostat daemon with no remote dependencies.

Tasks:

- [ ] SQLite schema/migrations,
- [ ] event/observation persistence,
- [ ] local temperature sensor adapter,
- [ ] systemd service/watchdog integration,
- [ ] daemon restart/recovery tests,
- [ ] trace replay tooling.

Gate P4:

- Operates indefinitely in simulation/local-sensor mode with UI/network absent.

## Phase 5 - ESP32 sensor network

**Goal:** distributed sensing without making networking safety-critical.

Tasks:

- [ ] versioned sensor packet schema,
- [ ] ESP32 + DS18B20 firmware,
- [ ] local MQTT ingest,
- [ ] identity/freshness/sequence validation,
- [ ] robust fusion algorithm,
- [ ] calibration tooling,
- [ ] sensor fault-injection suite.

Gate P5:

- Loss/corruption of any remote sensor set cannot directly produce illegal HVAC outputs.

## Phase 6 - API and local web UI

**Goal:** rich visibility and normal thermostat interaction.

Tasks:

- [ ] Unix-socket daemon protocol,
- [ ] unprivileged API gateway,
- [ ] React/TypeScript UI,
- [ ] live demand/action/block-reason display,
- [ ] sensor health views,
- [ ] runtime/history charts,
- [ ] schedules/presets,
- [ ] installer configuration workflow.

Gate P6:

- Web/API process has no GPIO permission and no raw-relay command surface.

## Phase 7 - Physical GPIO and relay bench validation

**Goal:** prove electrical control without HVAC equipment.

Prerequisites:

- exact Pi model,
- exact relay board,
- exact converter,
- actuator-enable design.

Tasks:

- [ ] Linux GPIO character-device backend,
- [ ] output polarity tests,
- [ ] boot-state tests,
- [ ] crash-state tests,
- [ ] LEDs/logic-analyzer run,
- [ ] relay continuity run,
- [ ] 24 VAC dummy-load run,
- [ ] Add-A-Wire simultaneous-Y/G bench run.

Gate P7:

- Hardware defaults safe through boot/crash/power loss.
- Same controller traces pass simulator and physical actuator contract tests.

## Phase 8 - HVAC commissioning

**Goal:** supervised operation on actual equipment.

Prerequisites:

- condenser documented,
- transformer VA budget accepted,
- fuse/protection plan documented,
- furnace wiring photographed and mapped,
- rollback to original thermostat possible.

Sequence:

- [ ] fan-only call,
- [ ] heat call + furnace-owned blower sequence observation,
- [ ] heat stop + board post-run observation,
- [ ] cooling call under appropriate conditions,
- [ ] cooling stop + board post-run observation,
- [ ] compressor lockout test without rapid cycling equipment,
- [ ] restart behavior test,
- [ ] 24-hour supervised observation.

Gate P8:

- Production thermostat role accepted only after measured behavior matches profile.

## Phase 9 - Weather and energy intelligence

**Goal:** improve efficiency without changing actuation authority.

Tasks:

- [ ] weather-provider abstraction/cache,
- [ ] outdoor-temperature history,
- [ ] runtime vs outdoor temperature models,
- [ ] preheat/precool scheduler,
- [ ] comfort-bound optimizer,
- [ ] energy-price input if useful.

Gate P9:

- Disconnecting optimizer/weather returns system to ordinary deterministic thermostat behavior.

## Phase 10 - Learned thermal model / MPC experiments

**Goal:** data-driven predictive control bounded by the same safety core.

Tasks:

- [ ] system identification,
- [ ] first-order/RC house model fitting,
- [ ] per-room response characterization,
- [ ] offline MPC simulation against historical weather,
- [ ] compare energy/comfort Pareto frontier against baseline,
- [ ] shadow-mode predictions,
- [ ] limited bounded setpoint actuation only after evidence.

No optimizer receives direct `W/Y/G` authority.

