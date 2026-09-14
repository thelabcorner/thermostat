# Sensor Network Plan

## Goals

- Measure temperatures where people actually spend time.
- Keep basic thermostat operation independent of remote networking.
- Detect bad/stale sensors instead of averaging them blindly.
- Preserve raw observations for later thermal-model training.
- Make transport replaceable (Wi-Fi/MQTT first, ESP-NOW gateway later if useful).

## Sensor hierarchy

### Tier 0: local control sensor

At least one sensor physically associated with the thermostat/controller is considered the default resilient control source.

Requirements:

- local to the Pi or wired to a local microcontroller,
- does not depend on Internet,
- ideally does not depend on Wi-Fi,
- has configured calibration metadata,
- participates in the same plausibility/health system as remote sensors.

### Tier 1: ESP32 room nodes

Each node initially contains:

- ESP32,
- DS18B20 digital temperature sensor,
- stable node identity,
- firmware version,
- monotonic sample sequence,
- sensor health information,
- optional RSSI/power metrics.

## DS18B20 considerations

The DS18B20 is convenient and digitally addressed, but software must respect its real accuracy rather than displaying false precision.

Plan:

- sample at a modest cadence (for example 5-30 s; final value experimentally selected),
- never block thermostat control waiting for a remote conversion,
- calibrate sensor offsets against a reference if useful,
- keep raw and calibrated values separately in telemetry.

## Wire/network packet model

Remote nodes publish observations only.

Conceptual payload:

```json
{
  "protocol": 1,
  "node_id": "office-01",
  "sensor_id": "28-xxxxxxxxxxxx",
  "sequence": 18422,
  "uptime_ms": 98220111,
  "temperature_c": 22.1875,
  "sample_age_ms": 42,
  "firmware": "0.1.0",
  "rssi_dbm": -61
}
```

No message type accepted from a sensor node can command HVAC equipment.

## Transport V1

Recommended initial transport:

```text
ESP32 -> Wi-Fi -> local MQTT broker -> thermostat sensor-ingest adapter
```

Benefits:

- easy packet inspection,
- easy replay/simulation,
- mature libraries,
- no custom mesh routing,
- transport failure is easy to model.

MQTT is an ingress adapter, not part of the controller core.

## Future transport option

ESP-NOW may later be useful for low-power/direct sensor links. A gateway design is preferable:

```text
ESP32 room nodes -> ESP-NOW -> gateway ESP32 -> USB/MQTT/local IPC -> Pi
```

The application-level observation schema should remain transport-independent.

## Sensor validation pipeline

Each observation passes:

1. schema/protocol validation,
2. identity authorization,
3. sequence/duplicate handling,
4. numeric finite check,
5. absolute configured plausibility range,
6. timestamp/age validation,
7. rate-of-change plausibility,
8. calibration transform,
9. health update,
10. fusion eligibility decision.

## Health states

```text
HEALTHY
DEGRADED
STALE
OUTLIER
OFFLINE
DISABLED
```

Health is explicit state, not inferred ad hoc in UI code.

## Fusion V1

Do not use a simple mean of every available sensor.

Initial algorithm candidates to benchmark in simulation:

- weighted median,
- trimmed weighted mean,
- Huber M-estimator.

The chosen V1 algorithm should be simple enough to explain and deterministic.

Inputs include:

- configured room weight,
- health state,
- freshness,
- calibration offset,
- optional currently selected comfort zone.

## Failure behavior

- One remote node offline: remove from fusion after freshness threshold.
- One node implausible: mark outlier; do not allow it to drag control temperature.
- All remote nodes offline, local valid: continue local thermostat behavior.
- Local node invalid, multiple remote nodes trustworthy: policy may continue using remote quorum after V1 validation.
- All trusted sources invalid: safe control-temperature fault; no blind heating/cooling.

## Thermal-model data

Preserve enough metadata to later fit house response models:

- raw/calibrated room temperatures,
- control temperature,
- outdoor temperature,
- W/Y/G action intervals,
- user setpoints/mode changes,
- sensor health,
- optional humidity later.

Do not downsample away transients needed to estimate startup/decay behavior until retention needs are measured.

