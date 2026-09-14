# Project Charter

## Mission

Build a local-first residential thermostat/controller that is more observable, testable, and extensible than a typical consumer thermostat while remaining conservative about HVAC actuation.

The system must safely control the existing conventional 24 VAC forced-air installation, support distributed room sensing, and create a foundation for future weather-aware and model-predictive optimization without putting those experimental layers in the critical actuation path.

## Primary objectives

### O1. Correct conventional HVAC control

Support the current single-stage equipment using logical `W`, `Y`, and `G` calls with configurable equipment profiles.

### O2. Equipment protection

Enforce compressor startup/minimum-off protection, minimum run/off policies where appropriate, heat/cool changeover delays, safe startup, and invalid-state rejection.

### O3. Local resilience

Normal thermostat operation must continue without:

- Internet connectivity,
- weather APIs,
- cloud services,
- remote ESP32 sensors, provided a trustworthy local temperature sensor remains available,
- the web UI,
- successful telemetry writes.

### O4. Deterministic and explainable behavior

Given the same state, monotonic timestamp, configuration, and event sequence, the controller must produce the same decisions and outputs.

Every relay transition should be explainable through an event record containing the observed demand, applied safety gate, transition, and resolved output vector.

### O5. Distributed sensing

Use ESP32 nodes to measure temperature in occupied/important rooms, detect stale/bad sensors, calibrate individual nodes, and combine observations robustly.

### O6. Deep verification

Treat temporal control as a testable state machine. Unit tests alone are insufficient; property testing, fuzzing, model/simulation testing, restart/fault injection, and mutation testing are part of the acceptance criteria.

### O7. Extensible optimization

Create an upper-layer setpoint optimizer that can later use forecasts, measured house response, occupancy, and energy price information without ever gaining direct relay authority.

## Non-goals for V1

- Variable-speed communicating HVAC protocols.
- Modulating gas-valve control.
- Direct compressor modulation.
- PID/PWM control of the existing single-stage compressor.
- Cloud dependency.
- Voice-assistant integration.
- Geofencing.
- Automatic commissioning without human inspection of actual wiring/equipment.
- Replacing safety functions already implemented by the furnace control board.
- Bypassing furnace limit switches, pressure switches, rollout switches, or condenser protection devices.

## Success criteria

The first hardware-capable milestone is successful only when:

1. A deterministic simulator can reproduce heat, cool, fan-only, OFF, and AUTO demand transitions.
2. The property test suite has never found simultaneous heat/cool outputs.
3. Cooling startup lockout survives arbitrary process restarts and wall-clock jumps.
4. Remote-sensor loss degrades cleanly to the local sensor.
5. Total temperature-source loss de-energizes HVAC calls according to the defined fault policy.
6. Invalid/corrupt equipment configuration cannot energize relays.
7. Relay active polarity is proven on the bench before enabling a 24 VAC circuit.
8. The virtual actuator and GPIO actuator pass the same output contract tests.
9. Bench hardware demonstrates de-energized outputs during Pi boot, daemon crash, and controller shutdown.
10. The exact outdoor condenser model and compressor constraints are documented.

## Long-term success criteria

After baseline operation is trustworthy, the system should be able to answer:

- Which rooms are driving comfort demand?
- How long did each equipment mode run today?
- Why did a requested call not start immediately?
- What is the observed heating/cooling rate at a given outdoor temperature?
- Is HVAC effectiveness degrading over time?
- Would preheating/precooling reduce cost while staying inside comfort bounds?
- How much confidence do we have in each temperature sensor?

## Guiding architecture statement

The thermostat is a deterministic equipment controller surrounded by optional intelligence, not an automation platform that happens to own GPIO pins.

