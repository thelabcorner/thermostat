# Faults, Recovery, and Security

## Failure model

The design assumes components will fail independently.

## Fault matrix

| Failure | Expected behavior |
| --- | --- |
| Internet unavailable | No effect on local thermostat control |
| Weather API unavailable/bad | Optimizer unavailable; deterministic control continues |
| MQTT unavailable | Remote sensors age out; local sensor continues |
| One remote sensor frozen | Mark stale; remove from fusion |
| One remote sensor implausible | Mark outlier; do not use normally |
| All remote sensors lost | Local sensor control continues |
| All trusted temperature sources lost | Heat/cool de-energize; fault visible |
| SQLite telemetry write fails | Control continues; fault/metrics degrade |
| Required config load invalid | Actuator never arms |
| UI crashes | Control continues |
| API gateway crashes | Control continues |
| `thermostatd` crashes | Hardware must fall toward de-energized outputs; restart enters safe boot/lockout |
| Pi reboots while cooling | Outputs drop; on restart full compressor startup lockout |
| Wall time jumps | No effect on active control timers |
| GPIO write fails | Latch actuator fault; attempt safe de-energization/disable |
| Relay-enable heartbeat lost | Preferred hardware design de-energizes relay power |

## Crash consistency

The daemon must not rely on "finally" blocks/destructors to make hardware safe. A process can be killed without cleanup.

Electrical defaults and actuator-enable hardware therefore carry part of the safety responsibility.

## Watchdog layers

### Software watchdog

Run the daemon under systemd with restart policy and watchdog/health notification.

### Hardware watchdog target

A later relay I/O bridge can require periodic valid heartbeats from the Pi and drop all outputs on timeout.

This prevents a hung-but-not-dead Linux controller from holding calls forever solely because its last GPIO levels persist.

## Privilege model

Principle of least privilege:

- `thermostatd`: access to specific GPIO character device lines, local IPC socket, config/storage.
- API/UI gateway: no GPIO device access.
- MQTT broker: no GPIO access.
- web UI: browser sandbox only.
- remote sensors: publish restricted sensor topics only.

Do not run the web UI/gateway as root to obtain GPIO access.

## Local API security

Even a LAN-only thermostat can be attacked by another compromised local device.

Plan:

- daemon IPC bound to Unix socket by default,
- gateway authenticates state-changing requests,
- CSRF protection for browser session where applicable,
- rate limits for configuration changes,
- installer configuration separated from comfort controls,
- no shell/command execution API,
- strict numeric limits on setpoints/timers.

## Sensor authorization

MQTT topic names alone are not identity.

V1 local network can use broker credentials per device or a constrained shared provisioning model. Later ESP-NOW transport should use peer encryption where appropriate.

Regardless of transport, a malicious sensor can at worst manipulate temperature observations; it must not possess an actuation command channel.

## Configuration safety

Installer changes are transactional:

1. receive candidate config,
2. parse,
3. schema validate,
4. semantic validate,
5. de-energize equipment if hardware mapping/timing changed materially,
6. persist revision,
7. initialize/verify actuator,
8. explicitly arm.

If any step fails, remain on old validated config when safe to do so, or remain unarmed.

## Recovery behavior

### Controller restart

1. outputs/enable default safe,
2. load + validate equipment config,
3. initialize sensors/storage,
4. initialize GPIO inactive,
5. establish conservative cooling startup lockout,
6. wait for valid control temperature,
7. arm actuator,
8. resume demand evaluation.

### Database corruption

Do not attempt to recover hardware mappings from guessed/default records. Keep actuator unarmed until a known-good equipment/install config exists.

Comfort settings may fall back to explicit safe defaults only if the installer profile remains valid.

## Freeze-protection consideration

Turning all HVAC off after complete sensor failure is equipment-safe but can be undesirable for a house in freezing weather.

The initial implementation stays conservative: no temperature information means no blind furnace call.

A later freeze-protection mode may be added only if it has an independent trustworthy local temperature source or hardware thermostat/interlock. It should not be implemented by "run heat periodically and hope."

