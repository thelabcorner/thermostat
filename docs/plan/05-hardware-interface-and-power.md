# Hardware Interface and Power Plan

## 24 VAC control model

The HVAC transformer provides nominal 24 VAC between `R` and `C`.

Thermostat calls are formed by closing `R` to the respective control input:

```text
R -> W : heat request
R -> Y : cooling request
R -> G : fan request
```

`R`/`C` should not be modeled as DC positive/negative. They are the two sides of an AC secondary.

## Existing four-wire constraint

The original wall run has no independent `C` conductor. The current plan uses an Add-A-Wire-style multiplexer so two logical control signals share one physical conductor and the former fan conductor becomes `C`.

Software remains unaware of wire multiplexing:

```text
logical Y -> relay -> multiplexer path
logical G -> relay -> multiplexer path
```

The exact accessory must be identified and bench-tested before installation. We will not assume every third-party "C-wire adapter" implements the same diode polarity or simultaneous-Y/G behavior as the Venstar reference topology.

## Pi power conversion

### Rectified voltage

Nominal 24 VAC is RMS.

Ideal peak:

```text
24 * sqrt(2) ~= 33.9 V
```

A bridge rectifier subtracts diode drops, but transformer no-load regulation and high AC line can move the actual bus. Therefore a downstream DC regulator designed for only 24 VDC input is not sufficient merely because the transformer is called "24 V."

### Converter acceptance criteria

The selected module must specify:

- 24 VAC input compatibility (preferred) or a DC input range with sufficient rectified-voltage margin,
- regulated 5 V output suitable for the chosen Pi,
- continuous current with thermal margin,
- acceptable startup/inrush behavior,
- isolation class clearly understood,
- over-current and thermal protection,
- operating temperature suitable for the enclosure.

### Power budget

Before acceptance:

```text
P_pi_peak
+ P_relay_coils_peak
+ P_sensor/local_display/etc
+ converter losses
<= safe available HVAC transformer VA margin
```

Do not assume the furnace transformer has spare capacity merely because it can power a conventional thermostat.

If margin is poor, use a dedicated listed power supply rather than overloading the furnace control transformer.

## Relay interface

### Required contact topology

Use isolated, normally-open dry contacts:

```text
relay common -> R
relay NO #1  -> W
relay NO #2  -> Y
relay NO #3  -> G
```

The spare channel remains unassigned initially.

### Why normally open

Removing relay coil power should remove HVAC calls. This aligns electrical failure state with software fail-safe state.

### Relay board acceptance criteria

- contact rating comfortably exceeds 24 VAC control-circuit requirements,
- coil/control current within Pi-side power budget,
- known active-high vs active-low behavior,
- no relay chatter during Pi boot,
- no unintended relay closure while GPIO is high-impedance,
- input isolation topology documented,
- safe common/ground arrangement documented,
- deterministic behavior when Pi loses power.

An "optocoupled" board is not automatically isolated if its JD-VCC/VCC grounds are tied together. We will document the exact board rather than trusting marketing text.

## Hardware actuator enable

Software cannot fully control GPIO electrical behavior before Linux has booted.

Preferred design direction:

1. Relay coil power defaults OFF at hardware level.
2. `thermostatd` initializes all output lines inactive.
3. Configuration and line ownership are validated.
4. A separate actuator-enable path is asserted.
5. Loss of enable or controller heartbeat de-energizes relay power.

The simplest implementation may be a hardware power-enable transistor/relay with a safe pull state. A stronger later design is a small watchdog microcontroller that owns relay outputs and requires a valid command/heartbeat protocol from the Pi.

The latter is attractive because a frozen Linux process is then electrically distinguishable from a healthy controller.

## GPIO design

Use Linux GPIO character-device interfaces, not deprecated sysfs GPIO.

Requirements:

- lines requested exclusively,
- logical active state abstracted from physical level,
- line names/chip IDs configured explicitly,
- startup request includes inactive defaults where supported,
- vector writes used where possible,
- every write checked for error,
- actuator fault latches the controller into safe state.

## Add-A-Wire bench test plan

Before connecting the furnace:

1. Use an isolated 24 VAC source/fused bench setup.
2. Verify no output with all relays open.
3. Assert logical `G` only; measure expected receiver output.
4. Assert logical `Y` only; measure expected receiver output.
5. Assert `Y + G`; verify both receiver outputs simultaneously.
6. Rapidly sequence all legal action transitions and look for transient false outputs.
7. Observe behavior during Pi/relay power loss.

The controller simulator should replay the same output sequence used on the bench.

## Commissioning stages

```text
software simulator
  -> GPIO loopback/LEDs
  -> relay contacts with continuity meter
  -> isolated bench 24 VAC + dummy loads
  -> Add-A-Wire bench topology
  -> HVAC low-voltage board with equipment service power controlled
  -> supervised heat/fan tests
  -> supervised cooling test after condenser identification
```

No stage is skipped because the previous one "looks obvious."

