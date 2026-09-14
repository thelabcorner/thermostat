# Research Notes and References

This file records the initial external evidence used to shape the architecture. It is not a substitute for the exact manuals for the installed board/condenser that will be captured during commissioning.

## 1. Goodman GMSS96 family

### Manufacturer spec material

Goodman GMSS96 / GCSS96 specification material describes the family as a single-stage, multi-speed gas furnace with a self-diagnostic integrated control board and low-voltage thermostat terminals.

Reference:

- https://apps.goodmanmfg.com/brochures/files/5697bee2d3a66SS-GMSS96.pdf

### Installation / sequence behavior

Available GMSS96-family installation documentation describes:

- furnace-owned heating sequence,
- thermostat cooling call using `R/Y/G`,
- fan-only using `R/G`,
- selectable heating blower off delay on 96% models in the referenced generation,
- cooling blower continuing for an internal off-delay after the cooling call ends.

Secondary indexed reference used to locate the relevant sequence section:

- https://www.manualslib.com/manual/1059468/Goodman-Gmss96.html

Project decision derived from this:

> The current equipment profile uses `W` without `G` for normal heat, `Y+G` for cool, and avoids implementing arbitrary thermostat-side heat blower warmup/post-run timers that the furnace board already owns.

The exact installed control-board revision and actual configured blower delay remain commissioning items.

## 2. Add-A-Wire / four-conductor retrofit

Venstar ACC0410 documents the same general architecture required by this house: two thermostat signals share an existing conductor so another conductor can be repurposed for common.

References:

- https://venstar.com/thermostats/accessories/add-a-wire/
- https://files.venstar.com/accessories/ACC0410ManualRev2.pdf

Project decision:

> Model Y and G as independent logical thermostat outputs. Treat the multiplexer as an electrical transport layer and verify the exact purchased product on the bench, especially simultaneous Y+G operation.

## 3. ESPHome thermostat controller

ESPHome's thermostat component provides useful prior art for explicit temporal constraints:

- minimum cooling off time,
- minimum cooling run time,
- minimum heating off/run times,
- minimum idle time,
- fan timing,
- startup delay,
- explicit heating/cooling action separation,
- fan-with-heating/cooling configuration.

Reference:

- https://esphome.io/components/climate/thermostat/

Project decision:

> Timing constraints belong in a dedicated policy/safety layer with equipment-specific configuration. Pending demand should be represented explicitly rather than lost while a timer blocks actuation.

## 4. Linux GPIO

The Linux GPIO character-device API and libgpiod ecosystem are the modern userspace GPIO path; deprecated sysfs GPIO should not anchor a new implementation.

Reference:

- https://docs.kernel.org/userspace-api/gpio/chardev.html
- https://libgpiod.readthedocs.io/

Project decision:

> Keep Linux GPIO behind a narrow HAL and build/test the controller against a virtual actuator first.

## 5. Compressor protection

A roughly five-minute restart delay is a common commercial thermostat/compressor protection convention. The final value should be at least as conservative as the actual outdoor equipment documentation.

Project decision:

> Use 300 seconds as a provisional cooling startup/minimum-off lockout until the condenser model is identified. On restart, apply the full lockout because monotonic off-history is no longer provable.

## 6. General thermostat software findings

Common patterns across Home Assistant, ESPHome, Tasmota, and DIY thermostat projects:

- hysteresis/deadband is a better baseline for binary HVAC than naive PID/PWM,
- minimum cycle timing belongs in the equipment-control layer,
- sensor-loss handling must be explicit,
- local operation should not depend on weather/cloud services,
- fan behavior differs by equipment type,
- restart/startup semantics are important and often under-specified in hobby implementations.

The project intentionally borrows mature ideas while avoiding their assumptions where the local equipment differs.

## 7. Open questions for targeted research

Before hardware implementation, research/measure:

1. exact Goodman integrated board revision for `GMSS960804CNAA`,
2. exact outdoor condenser model/manual,
3. compressor manufacturer minimum off/run recommendations,
4. actual furnace transformer VA rating,
5. exact Add-A-Wire/multiplexer electrical implementation,
6. exact relay board boot characteristics,
7. Pi model peak power and brownout behavior on selected converter,
8. candidate local temperature sensor accuracy/thermal placement,
9. relay hardware watchdog/enable topology.

## Research discipline

For equipment-specific behavior, preference order is:

1. exact installed unit/board manufacturer manual,
2. manufacturer family manual matching revision,
3. measured behavior under controlled commissioning,
4. reputable thermostat prior art,
5. hobby-project assumptions only as ideas to test.

