# Elamon protocol, API 1

The service serves HTTPS on port 47823. Its certificate is self-signed; clients identify the service
by the SHA-256 of that certificate (lowercase hex, 64 characters), not by a name.

All bodies are JSON. Numbers in bytes are integers; rates are bytes per second; temperatures are
°C; frequencies MHz; power W. Clients must ignore fields they don't know: new fields are added
without changing the API number. A field is never renamed or given a new meaning.

Every shape below, with each optional field filled in, is in [`elamond/fixtures`](../elamond/fixtures):
the service's tests write those files and the app's tests read them, so the two can't drift apart.

## Versions

- `api` (in `hello` and `info`) changes only if something incompatible ever has to; a client that
  sees a higher `api` than it knows should ask to be updated.
- `min_app` (in `hello` and `info`) is the oldest app version that understands this service.
- `version` / `service_version` is the service's own version; a client may expect a minimum one for
  newer features and fall back without them.

## Discovery

The service announces `_elamon._tcp` over mDNS with TXT records:

| key | |
|---|---|
| `id` | the service's id, 32 hex characters, stable across restarts |
| `api` | API version |
| `fp` | first 16 hex characters of the certificate's SHA-256 |

## Pairing

`elamond pair` makes a six-digit code, valid for 5 minutes and 5 wrong attempts, and prints it
with a QR code holding

    elamon://pair?v=1&id=<id>&name=<name>&port=<port>&fp=<sha256>&code=<code>&h=<addr>,<addr>…

`h` lists the computer's IPv4 addresses, local network first, then VPNs. Stopping `elamond pair`
withdraws its code.

### `POST /api/v1/pair` (no token)

```json
{ "code": "123456", "device": "Phone name" }
```

200: `{ "token": "…", "client_id": "…", "id": "<service id>", "name": "<service name>" }`
403: `{ "error": "wrong_code" }` or `{ "error": "no_pairing" }` when no code is pending.

The token goes in every other request as `Authorization: Bearer <token>`. Without a valid one
the service answers 401 `{ "error": "unauthorized" }`. A phone removed with `elamond revoke` gets
401 from then on, and any stream it has open ends.

### `GET /api/v1/hello` (no token)

`{ "app": "elamon", "api": 1, "id": "…", "name": "…", "version": "0.4.0", "min_app": "0.3.0" }`

## Data

### `GET /api/v1/stream`

Newline-delimited JSON (`application/x-ndjson`), one object per line, each with a `type`:

1. `info`, then again whenever it changes
2. `history`, if asked for
3. `sample`, every interval, until the client disconnects

Query parameters:

| name | default | |
|---|---|---|
| `interval` | the service's | milliseconds between samples, from the service's own `interval_ms` up to 60 000 |
| `procs` | 0 | how many processes to include, heaviest first, at most 10 000 |
| `sort` | `cpu` | `cpu`, `mem`, `gpu` or `io`: what "heaviest" means |
| `history` | 0 | 1 to get the last minutes first |
| `media` | 0 | 1 to include `media` in samples: the players, and the volume when `control` is on |

The service reads costlier sources (NVML, DRM, process list) only while some client streams and
asks for them, and lets a powered-down GPU sleep. The first sample with processes comes a tick
after they are first asked for: process CPU and disk use are rates, which need two readings.

### `GET /api/v1/snapshot`

One `sample` (without `type`). It takes `procs`, `sort` and `media` like the stream and waits up
to a few ticks for a reading that has everything asked for; `interval` and `history` don't apply.

### `GET /api/v1/info`, `GET /api/v1/history`

The `info` and `history` objects alone.

## Objects

A field marked `?` is `null` when the computer doesn't report it. Only `media`, `process_count`
and `processes` are left out of a sample when they weren't asked for.

`info`:

```
id, name, service_version, api, min_app, interval_ms (the service's tick), history_seconds (how far
history reaches), os, kernel, arch, cpu_model, cpu_physical_cores?, cpu_logical_cores,
memory_total, boot_time,
gpus: [{ id (PCI slot), vendor (nvidia|amd|intel|other), name, driver, boot_display }],
capabilities: ["processes", "displays", "history", "sensors", …]
```

`sample`:

```
t (Unix ms, the service's clock), uptime (s),
cpu: { usage (0–100), cores [], freq [], temp?, load [1, 5, 15 min] },
memory: { total, used, available, swap_total, swap_used },
gpus: [{ id, state (active|sleeping|unknown), usage?, mem_used?, mem_total?, temp?, power?,
         freq?, freq_max?, mem_freq?, fan? (%) }],
temps: [{ chip, kind (cpu|gpu|storage|wireless|board|battery|other), device?, label, value,
          high?, crit? }],
fans: [{ chip, label, rpm }],
battery?: { name, model?, percent?, status (charging|discharging|full|not_charging|unknown),
            power?, energy_now?, energy_full?, energy_design? (Wh), cycles?, time_left? (s) },
power: { on_ac?, profile?, profiles [], governor?, epp?, ppd_profile?, ppd_profiles [] },
disks: [{ device, mount, fs, total, available, removable }],
disk_io: [{ name, model?, read, write }],
net: [{ name, kind (wired|wireless|vpn|virtual), up, addrs [], rx, tx, rx_total, tx_total,
        speed? (Mbit/s) }],
displays: [{ connector, gpu, connected, active, dpms? (On|Standby|Suspend|Off), internal,
             vendor?, model?, year?, width_mm?, height_mm?, mode?, preferred?, max_refresh?,
             position? [x, y] }],
  mode: { width, height, refresh },
services: [{ id, name, url, unit?, scope (user|system), editable, port?, up?, local_only,
             unit_state? }],
process_count?, processes?: [{ pid, ppid?, name, cmd, user?, cpu (% of the whole CPU), mem,
                               status, start (Unix s), read, write, gpu?, gpu_mem? }],
media?: { players: [{ id, name, status (Playing|Paused|Stopped), title?, artist?, album?,
                      length? (µs), position? (µs), volume?, has_art, can_next, can_previous,
                      can_play, can_pause, can_seek }],
          volume?: { level (1.0 = 100 %), muted } }
```

`ppd_profile` and `ppd_profiles` are power-profiles-daemon's, which the app can switch; `profile`
and `profiles` are the firmware's platform profile. A process `status` is the kernel's state as
a word (Runnable, Sleeping, Idle, Zombie, Stopped, …).

`history`: parallel arrays, oldest first, one entry per service tick, reaching `history_seconds`
back:

```
t [], cpu [], cpu_temp [], mem [], swap [], rx [], tx [], read [], write [], battery_power [],
gpus: [{ id, usage [], temp [] }]
```

`rx`/`tx` sum wired and wireless links only, so VPN and bridge traffic isn't counted twice.
Missing values are `null`.

## Control

Everything below needs a token. Requests that change the computer, and media, also need
`control = true` in the service's settings, or the service answers 403 `control_disabled`. Adding,
changing and removing services only changes the app's list of links; starting and stopping
their units needs control.

| request | body | |
|---|---|---|
| `GET /api/v1/control` | | `{ enabled, power: { poweroff, reboot, suspend: "yes"\|"challenge"\|"no"\|"na", lock }, profiles?: { active, available }, actions: [{ id, name, confirm }], volume }` |
| `POST /api/v1/control/power` | `{ action: "poweroff"\|"reboot"\|"suspend"\|"lock" }` | |
| `POST /api/v1/control/profile` | `{ profile }` | power-profiles-daemon |
| `POST /api/v1/control/signal` | `{ pid, signal: "term"\|"kill" }` | never the service, its parents or PID 1 |
| `POST /api/v1/control/action` | `{ id }` | `{ exit_code?, timed_out, output }` |
| `POST /api/v1/media` | `{ player, op: "play_pause"\|"next"\|"previous"\|"seek"\|"volume", value? }` | MPRIS; seek in seconds |
| `POST /api/v1/media/volume` | `{ level?, muted? }` | the default output, level 0–1 |
| `GET /api/v1/media/art?player=` | | the cover, when the player has a local file |
| `GET /api/v1/services` | | the services with their state, as in samples |
| `POST /api/v1/services` | `{ id?, name, url, unit?, scope? }` | add, or change one added from the app; answers the saved `{ id, name, url, unit, scope }` |
| `DELETE /api/v1/services/{id}` | | services from the settings files can't be changed or removed here |
| `POST /api/v1/services/{id}/unit` | `{ op: "start"\|"stop"\|"restart" }` | needs control |
| `GET /api/v1/services/detected` | | `[{ port, local_only, process? }]`: ports something listens on |
| `GET /api/v1/units?scope=user\|system` | | `[{ name, description, active }]` |

Action ids come from the action's name and command, not its place in the settings, so a phone
holding an old list can't run a different command after the settings change; an id that no
longer exists answers 404 `no_such_action`. An action's output keeps its first 4 KiB and last
12 KiB; a timed-out action answers with what it printed so far.

Successful actions answer `{ "ok": true }`.

## Errors

Errors are `{ "error": "<code>" }` with these statuses (the same list is in
[`elamond/fixtures/errors.json`](../elamond/fixtures/errors.json)):

| status | codes |
|---|---|
| 400 | `unknown_signal`, `empty_command`, `no_unit`, `bad_name`, `bad_url`, `bad_unit`, `bad_scope` |
| 401 | `unauthorized` |
| 403 | `no_pairing`, `wrong_code`, `control_disabled`, `not_permitted` (polkit refused, or a protected process) |
| 404 | `not_found`, `no_such_process`, `no_such_action`, `no_such_service`, `no_art` |
| 500 | `storage` |
| 502 | `failed`, with `"message"` |

`failed` carries a `message`: either one of `unknown_action`, `unknown_operation`,
`unknown_player`, `unknown_profile`, `no_value`, `no_track`, `no_session`, `no_system_bus`,
`no_session_bus`, `no_power_profiles`, `volume_failed`, `mute_failed`, `timed_out`, or the
system's own error text (D-Bus, a command). A request whose JSON or query the service can't read
at all gets a plain-text 400, 415 or 422.
