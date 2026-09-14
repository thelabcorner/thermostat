# Control State Machine

## Design choice: orthogonal state + explicit equipment phase

A single mega-enum containing every combination of mode, fan mode, timer, demand, sensor health, and equipment status becomes difficult to reason about. The controller will instead keep orthogonal state fields while using an explicit equipment phase for temporal sequencing.

## State dimensions

### User mode

```text
OFF
HEAT
COOL
AUTO
```

### Fan mode

```text
AUTO
ON
```

Later:

```text
CIRCULATE
```

### Demand

```text
NONE
HEAT
COOL
```

Demand is computed from validated control temperature + setpoints + deadband + user mode. It is not synonymous with active equipment.

### Equipment phase

Initial conceptual phases:

```text
BOOT_SAFE
IDLE
HEAT_PENDING
HEATING
COOL_PENDING
COOLING
FAN_ONLY
CHANGEOVER_WAIT
FAULT_SAFE
```

We may collapse `HEAT_PENDING`/`COOL_PENDING` into `IDLE + block_reason` in code if that reduces duplication. The observable model must still communicate pending demand.

### Block reason

```text
NONE
STARTUP_LOCKOUT
MIN_HEAT_OFF
MIN_HEAT_RUN
MIN_COOL_OFF
MIN_COOL_RUN
CHANGEOVER_DELAY
SENSOR_INVALID
ACTUATOR_NOT_ARMED
CONFIG_INVALID
FAULT
```

## Relay-resolution model

The core first chooses a semantic equipment action:

```text
IDLE
HEAT
COOL
FAN_ONLY
```

The equipment profile then resolves the action into output contacts.

For the current Goodman profile:

```text
IDLE     => W=0 Y=0 G=0
HEAT     => W=1 Y=0 G=0
COOL     => W=0 Y=1 G=1
FAN_ONLY => W=0 Y=0 G=1
```

Fan `ON` modifies legal idle/heat behavior through profile-aware resolution rather than allowing the UI to independently assert `G` behind the controller's back.

## Hysteresis model

Prefer explicit low/high thresholds.

Example with separate AUTO targets:

```text
heat_target = 20.0 C
cool_target = 23.0 C

heating starts when T <= heat_on_threshold
heating stops  when T >= heat_off_threshold

cooling starts when T >= cool_on_threshold
cooling stops  when T <= cool_off_threshold
```

The exact threshold parameterization will be configuration-driven. The key property is path dependence: once equipment starts, the stop threshold differs from the start threshold so noise does not chatter the relay.

AUTO configuration must enforce sufficient separation between heat and cool regions after deadbands are applied.

## Transition evaluation order

Each event reevaluates the controller using an explicit priority order.

### 1. Validate actuator/config state

If actuator is unarmed, mapping invalid, or a critical actuator fault exists:

```text
desired_action = IDLE
phase = FAULT_SAFE or BOOT_SAFE
```

### 2. Apply explicit OFF

If mode is `OFF`, heating/cooling calls end immediately.

Fan behavior in `OFF + Fan ON` is a product decision. Initial plan: OFF means HVAC conditioning off but fan mode may still be separately allowed only if explicitly configured. The UI must make this unambiguous.

### 3. Validate temperature authority

If no trustworthy control temperature exists, enter `FAULT_SAFE` and de-energize heat/cool. A valid local sensor prevents remote-node loss from causing this state.

### 4. Compute temperature demand

Calculate `NONE`, `HEAT`, or `COOL` from current mode and hysteresis memory.

### 5. Preserve/terminate current equipment according to safety rules

If current action is heating/cooling:

- explicit OFF/fault can terminate immediately,
- normal comfort stop may be held until configured minimum runtime,
- opposite demand never transitions directly to opposite equipment.

### 6. Apply changeover/lockout rules

Before starting requested equipment, verify:

- required minimum off time,
- compressor startup lockout,
- heat/cool changeover delay,
- any equipment-profile start constraint.

If blocked, remain idle/pending and expose the reason + remaining duration.

### 7. Resolve fan behavior

Apply `FanMode` only through equipment-profile rules.

### 8. Resolve and validate physical outputs

Convert action -> output vector, then assert all output invariants before returning an actuator effect.

## Cooling lockout semantics

Default initial value: **300 seconds** until the actual condenser documentation is captured.

Important cases:

### Normal cycle end

Record monotonic time at `Y: 1 -> 0`. A subsequent cooling start is prohibited until minimum off duration has elapsed.

### Process restart

Monotonic process history is lost. The controller cannot prove how long the compressor has been off, so it applies a full startup cooling lockout from controller initialization.

### Power interruption

Same conservative rule. We do not attempt to infer safe off-duration from a possibly incorrect wall clock.

### User repeatedly toggles COOL

Turning COOL off ends `Y` immediately. Turning it back on does not bypass the lockout.

## Minimum run semantics

Minimum run times are intended to reduce short cycling, not override explicit safety/user shutdown.

Normal setpoint satisfaction can be delayed until minimum runtime expires.

Overrides that end the call immediately:

- user mode OFF,
- critical actuator/config fault,
- explicit emergency shutdown,
- future hardware interlock trip.

Sensor-loss policy is initially treated as a critical fault if no trusted fallback exists.

## Heat/cool changeover

An opposite demand while equipment is active behaves conceptually as:

```text
HEATING
  -> IDLE
  -> CHANGEOVER_WAIT
  -> COOL_PENDING
  -> COOLING (only after cooling lockout also passes)
```

and symmetrically for cooling -> heating.

The effective wait is the maximum of all applicable constraints, not separate timers that race unpredictably.

## Next-deadline computation

The reducer should expose the earliest future monotonic deadline at which a blocked decision could change without a new external event.

Examples:

- compressor lockout expiration,
- minimum run expiration,
- changeover expiration,
- sensor freshness expiration.

This permits an event-driven daemon rather than unnecessary high-frequency polling while retaining a periodic watchdog/health tick.

## Pseudocode

```text
reduce(state, event, now, config):
    state = apply_event(state, event)

    if !config.valid or !state.actuator_ready:
        return safe_idle(CONFIG_OR_ACTUATOR)

    if state.mode == OFF:
        return resolve_off_and_fan_policy(state)

    control_temp = sensor_fusion(state.sensors, now)
    if control_temp is None:
        return safe_idle(SENSOR_INVALID)

    demand = calculate_hysteretic_demand(state, control_temp, config)

    desired = evaluate_current_cycle_and_demand(state, demand, now, config)
    desired = apply_changeover_and_min_off_gates(desired, state, now, config)
    desired = apply_fan_policy(desired, state.fan_mode, config.equipment)

    outputs = config.equipment.resolve(desired)
    assert_invariants(outputs, state, now, config)

    return transition(state, desired, outputs)
```

## State persistence boundary

Persist enough information for observability, but never trust persisted timestamps to shorten a safety lockout after restart.

Safe pattern:

- database says compressor last stopped 20 minutes ago,
- daemon just restarted,
- still enforce startup lockout unless a separate reliable always-on timing source is later introduced.

This is intentionally conservative.

