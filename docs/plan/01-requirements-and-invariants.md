# Requirements and Invariants

## Terminology

- **Intent:** high-level user/automation request such as mode or setpoint.
- **Demand:** control-layer conclusion that heating/cooling is required from temperature state.
- **Action:** semantic equipment operation such as `HEAT`, `COOL`, `FAN_ONLY`, or `IDLE`.
- **Output vector:** physical thermostat contacts (`W`, `Y`, `G`) to energize.
- **Block reason:** safety/timing reason a demand cannot yet become an action.
- **Control temperature:** validated/fused value used for thermostat decisions.
- **Monotonic time:** continuously increasing process/OS time unaffected by civil clock changes.

## Functional requirements

### FR-001 Modes

Support:

- `OFF`
- `HEAT`
- `COOL`
- `AUTO`

### FR-002 Fan modes

Initial support:

- `AUTO`
- `ON`

Later support:

- `CIRCULATE`, defined as a minimum fan-runtime policy per window rather than continuous operation.

### FR-003 Deadband / hysteresis

Use configurable heating and cooling thresholds with an explicit neutral region. AUTO mode uses distinct heat and cool targets or an equivalent minimum separation invariant.

### FR-004 Minimum equipment times

Support independent configured values for:

- cooling minimum off time,
- cooling minimum run time,
- heating minimum off time,
- heating minimum run time,
- fan minimum run/off time if circulation logic needs it,
- heat/cool changeover idle time,
- startup lockout.

Timing values are equipment-profile policy, not scattered constants.

### FR-005 Equipment-owned sequencing

Allow profiles to define whether the thermostat or HVAC equipment owns fan behavior during heating/cooling.

### FR-006 Pending demand

The UI and event stream must distinguish "temperature requests cooling" from "compressor is running." A demand may remain pending behind a lockout.

### FR-007 Local sensor fallback

A validated local sensor can maintain basic control when remote sensors disappear.

### FR-008 Remote sensor fusion

Support sensor calibration, freshness, health, selection, and weighted/robust aggregation.

### FR-009 Restart safety

On daemon or machine restart, cooling must enter a conservative startup lockout when prior compressor off-duration is uncertain.

### FR-010 Persistent event history

Store semantic events and transitions, not only periodic temperature samples.

### FR-011 Simulation

The control core must run entirely against virtual time, virtual sensors, and virtual outputs.

### FR-012 Hardware abstraction

No control-core module imports Linux GPIO APIs.

## Hard invariants

These must become property tests and, where practical, formal/model assertions.

### INV-001 Mutual exclusion

For the current conventional single-stage profile:

```text
NOT (W && Y)
```

There is no valid state in which heating and cooling calls are simultaneous.

### INV-002 Boot de-energized

Before validated configuration + HAL initialization + explicit actuator arming:

```text
W = 0, Y = 0, G = 0
```

### INV-003 Cooling startup lockout

If the compressor off-duration is unknown or less than the configured minimum:

```text
Y = 0
```

regardless of cooling demand.

### INV-004 Changeover isolation

Heat-to-cool and cool-to-heat transitions must include a non-energized changeover state. No direct output-vector transition from a heat call to a cool call is legal.

### INV-005 Fault authority

Critical actuator/configuration fault forces all calls open.

### INV-006 Manual OFF authority

Explicit user `OFF` de-energizes heat/cool calls immediately. Comfort-oriented minimum runtime is not allowed to trap a user in an active call.

### INV-007 Monotonic timing

Equipment lockout calculations never use wall-clock subtraction.

### INV-008 Remote isolation

No packet received from an ESP32 node contains or can imply a relay command. Sensor protocol data is observational only.

### INV-009 API isolation

External API commands cannot express raw GPIO/relay writes.

### INV-010 Profile validation

An output mapping with contradictory calls, duplicate invalid hardware channels, unsupported equipment stages, or unknown relay polarity is rejected before arming.

### INV-011 Sensor plausibility

NaN, infinity, impossible configured ranges, stale samples, and protocol-invalid samples cannot enter the control-temperature calculation as normal observations.

### INV-012 Restart conservatism

Persistence may lengthen a lockout but may never shorten a lockout below what can be proven safe from current monotonic state.

## Policy requirements, not hard invariants

These are configurable and should not be confused with universal HVAC truths:

- minimum heating run time,
- minimum cooling run time,
- target deadbands,
- fan circulation schedules,
- sensor weighting,
- maximum-comfort deviation,
- long-cycle warning thresholds.

## "Maximum cycle time" policy

Long continuous operation is not inherently unsafe. On a design-day load a correctly sized single-stage system may run for a long time.

Therefore V1 treats a maximum cycle duration as an **observability/anomaly threshold**, not an automatic forced shutdown, unless specific equipment documentation requires a limit.

Warnings can incorporate temperature slope:

```text
LONG_COOL_RUNTIME + non-decreasing control temperature
LONG_HEAT_RUNTIME + non-increasing control temperature
```

This is more diagnostic than blindly terminating a valid call.

## Priority ordering

When multiple rules apply simultaneously, the initial precedence is:

1. actuator/configuration critical fault,
2. explicit system shutdown / user OFF,
3. loss of all trustworthy control temperatures,
4. equipment hard lockout/changeover rule,
5. equipment minimum-run policy,
6. fan-mode override where legal,
7. temperature demand/hysteresis,
8. optimizer/setpoint preference.

This order will be encoded in tests. If later equipment research requires a different priority, it must be changed deliberately with tests and documentation.

