# Implementation Plan

This directory is the working contract for the thermostat project. Code should be checked against these documents rather than allowing implementation details to silently redefine the intended behavior.

## Planning principles

1. **Safety before convenience.** A correct fail-safe idle is preferable to an incorrect attempt to preserve comfort.
2. **Deterministic core.** The safety controller is a pure state transition system driven by validated inputs and monotonic time.
3. **Equipment-specific behavior belongs in profiles.** `W`, `Y`, and `G` sequencing must not be generalized from one HVAC system to every system.
4. **Separate intent from actuation.** UI, schedules, sensors, weather, and optimization produce intents; only the controller produces relay vectors.
5. **Model before hardware.** Every important equipment transition must be executable in simulation before GPIO exists.
6. **Make uncertainty explicit.** Unknown equipment details become commissioning gates, not assumptions.
7. **Test temporal behavior.** A thermostat can have correct Boolean logic and still damage equipment through bad timing.
8. **Fail closed on configuration ambiguity.** Unknown relay polarity, invalid GPIO mapping, or corrupt equipment configuration means no HVAC calls.
9. **Keep remote services optional.** Internet loss must not degrade local thermostat safety or ordinary temperature control.
10. **Record enough state to explain every decision.** Each transition should be observable as cause -> gate -> resulting output.

## Document map

| Document | Purpose |
| --- | --- |
| [`00-project-charter.md`](00-project-charter.md) | Scope, objectives, non-goals, success criteria |
| [`01-requirements-and-invariants.md`](01-requirements-and-invariants.md) | Functional requirements and hard properties |
| [`02-architecture.md`](02-architecture.md) | Process, package, trust, and data-flow boundaries |
| [`03-control-state-machine.md`](03-control-state-machine.md) | State variables, transition ordering, timers, relay resolution |
| [`04-equipment-profile-goodman-gmss960804cnaa.md`](04-equipment-profile-goodman-gmss960804cnaa.md) | Current installed-equipment behavior |
| [`05-hardware-interface-and-power.md`](05-hardware-interface-and-power.md) | 24 VAC, converter, relays, GPIO, commissioning |
| [`06-sensor-network.md`](06-sensor-network.md) | Local + ESP32 sensor protocol, health, fusion |
| [`07-persistence-api-observability.md`](07-persistence-api-observability.md) | Storage, IPC/API, event model, UI boundary |
| [`08-test-and-validation-strategy.md`](08-test-and-validation-strategy.md) | Unit/property/fuzz/model/simulation/HIL strategy |
| [`09-faults-recovery-security.md`](09-faults-recovery-security.md) | Failure matrix, restart semantics, privilege boundaries |
| [`10-roadmap.md`](10-roadmap.md) | Ordered implementation phases and release gates |
| [`11-research-notes.md`](11-research-notes.md) | Source material and decisions derived from research |

## Decision hierarchy

When documents disagree, resolve the inconsistency before implementation. The intended authority order is:

1. hard safety invariants,
2. actual installed-equipment manufacturer documentation,
3. verified measured behavior of the installed system,
4. project architecture contract,
5. comfort behavior,
6. optimization behavior.

No weather/energy optimization is permitted to weaken an equipment invariant.

## Definition of "done" for the planning phase

Planning is complete enough to begin the controller core when:

- equipment actions and forbidden relay combinations are explicit,
- precedence among OFF/fault/min-runtime/lockout/hysteresis is explicit,
- restart behavior is defined,
- time-source semantics are defined,
- sensor-validity semantics are defined,
- configuration schema requirements are known,
- the initial test oracle can be written without GPIO,
- remaining hardware unknowns are listed as commissioning gates.

