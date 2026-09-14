# Formal safety model

`ThermostatSafety.tla` is a deliberately compact model of the equipment-safety
boundary. It does not attempt to reproduce comfort hysteresis or room physics;
those are covered by the executable Rust reference model and simulator.

The formal model concentrates on properties whose violation could create an
unsafe output transition:

- heat/cool mutual exclusion,
- actuator-unarmed and faulted states are de-energized,
- cooling cannot be active while compressor/changeover lockouts are non-zero,
- heat/cool family reversal cannot happen in a single state transition,
- cooling stop re-establishes a lockout,
- startup begins with a conservative cooling lockout.

The default TLC configuration intentionally uses very small integer timer
domains so exhaustive state exploration remains cheap. The timer values do not
represent seconds; only the transition relationships matter.

Run, when `tla2tools.jar` is available:

```text
java -cp tla2tools.jar tlc2.TLC -config ThermostatSafety.cfg ThermostatSafety.tla
```

The executable controller remains the source of timing values. This model is a
second, smaller specification used to challenge the implementation, not a code
generator.

## Current TLC result

Rechecked on 2026-09-14 with TLC 2.19 and the checked-in configuration:

```text
141 states generated
43 distinct states found
0 states left on queue
complete state-graph depth: 8
no error found
```

The bounded model is intentionally small enough for exhaustive exploration. A
successful TLC run is evidence for the modeled transition relationships; it is
not a claim that the Rust implementation or future hardware stack has been
formally verified end-to-end.
