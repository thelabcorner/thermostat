# Persistence, API, and Observability

## Design goals

- Explain every HVAC transition.
- Make real traces replayable through the deterministic core.
- Keep database/UI failure outside the safety path.
- Separate installer configuration from day-to-day comfort controls.

## SQLite role

SQLite is appropriate for the local controller because the event rate is tiny, durability is useful, and the data benefits from relational queries.

Planned tables/concepts:

```text
schema_version
equipment_profiles
installation_config
comfort_config
sensors
sensor_calibrations
sensor_observations
controller_events
equipment_transitions
faults
runtime_rollups
weather_observations       # later
optimizer_models           # later
```

Use WAL mode if measurements show it improves concurrent UI reads without undesirable SD-card write amplification. This is a measured implementation decision, not an automatic choice.

## Event sourcing boundary

The project does not need a fully event-sourced architecture, but the controller should emit semantically rich events.

Example transition record:

```json
{
  "event": "equipment_transition",
  "from": "IDLE",
  "to": "COOLING",
  "reason": "cool_demand_lockout_expired",
  "control_temperature_c": 24.1,
  "cool_target_c": 23.0,
  "outputs_before": {"w": false, "y": false, "g": false},
  "outputs_after": {"w": false, "y": true, "g": true},
  "monotonic_sequence": 8194
}
```

Wall timestamps are attached for human history, but temporal correctness uses monotonic state.

## Persistence failure policy

If SQLite becomes unavailable while the controller is healthy:

- control continues,
- error is surfaced,
- in-memory bounded diagnostic events may be retained temporarily,
- no safety timer is shortened,
- actuator state does not depend on a successful log write.

Configuration loading is different: if required equipment configuration cannot be read/validated at startup, the actuator remains unarmed.

## Configuration versioning

Every accepted installation/equipment configuration receives:

- schema version,
- content hash or revision ID,
- validation result,
- activation timestamp,
- previous revision reference.

Changing installer-level equipment mappings should force safe de-energization and explicit actuator re-arm.

## Local IPC/API

The daemon interface should be narrow and typed.

Candidate implementation:

- Unix domain socket on Linux,
- versioned request/response protocol,
- local gateway exposes HTTP/WebSocket to UI,
- gateway process runs without GPIO access.

The exact serialization can be JSON initially for inspectability. A binary schema is unnecessary at thermostat data rates unless later measurements justify it.

## Read API concepts

- current mode/fan mode,
- heat/cool targets,
- control temperature,
- all sensor observations/health,
- demand vs active action,
- current block reason and remaining duration,
- W/Y/G logical output state,
- equipment runtime today,
- active faults,
- controller/firmware versions,
- recent transitions.

## Write API concepts

- set mode,
- set fan mode,
- set comfort targets,
- activate preset,
- acknowledge non-critical fault,
- installer-only validated configuration transaction.

No raw relay API.

## Observability requirements

The UI should be able to show states such as:

```text
Cooling requested
Waiting 02:13 for compressor protection
```

rather than simply showing "Idle."

Likewise:

```text
Heating
Source temperature: 69.8 F
Target: 72.0 F
Control source: Office + Local
```

## Runtime metrics

Track at minimum:

- heat runtime/day,
- cool runtime/day,
- fan-only runtime/day,
- cycle count,
- average cycle length,
- short-cycle prevented count,
- lockout wait duration,
- temperature change per runtime interval,
- sensor uptime/fault counts.

These metrics later feed equipment-health and optimizer work.

## Replay

Recorded external events should be convertible into a deterministic replay fixture:

```text
initial config
+ sensor observations
+ user intents
+ monotonic deltas
=> expected controller transitions/output vectors
```

Any field failure seen in real operation should become a regression fixture.

