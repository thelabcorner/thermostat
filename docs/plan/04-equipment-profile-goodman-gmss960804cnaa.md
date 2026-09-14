# Installed Equipment Profile: Goodman GMSS960804CNAA

## Observed unit

The installed EnergyGuide label identifies the furnace as:

```text
Goodman Company, L.P.
Model GMSS960804CNAA
Weatherized Natural Gas Furnace
EnergyGuide AFUE: 96.1%
```

The GMSS96 family is a single-stage, high-efficiency gas furnace with a multi-speed circulating blower.

## Thermostat-control implications

### Heating

The furnace integrated control module owns the gas ignition sequence and circulating-blower timing.

Thermostat semantic action:

```text
HEAT => W
```

Default profile must **not** require `G` with `W`.

The furnace board is expected to:

1. receive `R-W` heat call,
2. perform its safety/inducer/ignition sequence,
3. establish flame,
4. start the circulating blower according to its designed sequence,
5. close the gas valve when the call ends,
6. continue post-purge / blower operation according to board timing.

GMSS96-family installation documentation describes a selectable heat blower off-delay, with historical documentation showing 100/150-second options and a factory-shipped 150-second setting for the referenced generation. The **actual installed board/jumper setting must be inspected** before we encode this as measured installation metadata.

Thermostat software should not recreate that delay by holding `G` after `W`.

### Cooling

GMSS96-family documentation describes a cooling thermostat call using `Y + G`. The furnace control energizes the circulating blower at the configured cool speed while the outdoor cooling equipment responds to `Y`.

Default profile:

```text
COOL => Y + G
```

The family documentation also describes an internal cool blower off-delay (45 seconds in the referenced installation manual generation). This means the thermostat generally opens both `Y` and `G` when cooling demand ends and allows the furnace board to perform its own post-run behavior.

We will verify the actual installed board behavior during commissioning.

### Fan-only

Default:

```text
FAN_ONLY => G
```

The furnace control board selects the appropriate configured blower behavior.

## Current profile draft

```yaml
profile_id: goodman-gmss960804cnaa-current-installation
equipment_family: conventional_gas_furnace_split_ac
stages:
  heat: 1
  cool: 1

fan_ownership:
  heat: equipment
  cool: thermostat_call_required

actions:
  idle: []
  heat: [W]
  cool: [Y, G]
  fan_only: [G]

forbidden:
  - [W, Y]

timing:
  cooling_min_off_s: 300   # provisional until condenser manual captured
  cooling_min_run_s: TBD
  heating_min_off_s: TBD
  heating_min_run_s: TBD
  heat_cool_changeover_s: TBD

equipment_internal_behavior_observed_or_documented:
  heat_blower_off_delay_s: "board-configured; verify installed setting"
  cool_blower_off_delay_s: "family documentation: 45; verify installed board"
```

This is planning syntax, not yet the final configuration schema.

## Required commissioning inspection

Before enabling hardware:

- photograph/read the furnace low-voltage terminal strip,
- identify integrated control board part/revision,
- capture transformer VA rating,
- inspect heat-off-delay jumper/cut setting,
- identify cooling speed tap/configuration only for documentation (do not alter it as part of thermostat work),
- verify `R`, `C`, `W`, `Y`, `G` terminal labeling,
- measure nominal `R-C` AC voltage unloaded and with existing controls,
- identify the outdoor condenser exact model and control requirements,
- document any existing condensate safety switch, float switch, service interlock, or accessory wired into the Y circuit.

## Cooling-system unknown

The furnace model does not identify the compressor/condenser. Compressor protection ultimately belongs to the outdoor equipment specification.

Until documented:

- use a conservative 5-minute cooling startup/minimum-off lockout,
- do not implement automatic rapid restart for any reason,
- treat minimum run time as configurable rather than universal,
- preserve any existing hardwired condenser/condensate safety chain.

## Why this profile matters

Generic thermostat software often exposes flags such as `fan_with_heating`. That is useful only if equipment-specific configuration is correct. This installation demonstrates why the project must not hard-code "heat = W + G" as a universal truth.

