# Elamon protocol, API 1

The service serves HTTPS on port 47823. Its certificate is self-signed; clients identify the service
by the SHA-256 of that certificate (lowercase hex, 64 characters), not by a name.

All bodies are JSON. Numbers in bytes are integers; rates are bytes per second; temperatures are
°C; frequencies MHz; power W. Clients must ignore fields they don't know: new fields are added
without changing the API number. A field is never renamed or given a new meaning.

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

`h` lists the computer's IPv4 addresses, local network first, then VPNs.

### `POST /api/v1/pair` (no token)

```json
{ "code": "123456", "device": "Phone name" }
```

200: `{ "token": "…", "client_id": "…", "id": "<service id>", "name": "<service name>" }`
403: `{ "error": "wrong_code" }` or `{ "error": "no_pairing" }` when no code is pending.

The token goes in every other request as `Authorization: Bearer <token>`. Without a valid one
the service answers 401 `{ "error": "unauthorized" }`.

### `GET /api/v1/hello` (no token)

`{ "app": "elamon", "api": 1, "id": "…", "name": "…", "version": "0.1.0" }`

## Data

### `GET /api/v1/stream`

Newline-delimited JSON (`application/x-ndjson`), one object per line, each with a `type`:

1. `info`, then again whenever it changes
2. `history`, if asked for
3. `sample`, every interval, until the client disconnects

Query parameters:

| name | default | |
|---|---|---|
| `interval` | the service's | milliseconds between samples, at least the service's own |
| `procs` | 0 | how many processes to include, heaviest first |
| `sort` | `cpu` | `cpu`, `mem`, `gpu` or `io`: what "heaviest" means |
| `history` | 0 | 1 to get the last minutes first |

The service reads costlier sources (NVML, DRM, process list) only while some client streams and
asks for them, and lets a powered-down GPU sleep.

### `GET /api/v1/snapshot`

One `sample` (without `type`), same parameters as the stream.

### `GET /api/v1/info`, `GET /api/v1/history`

The `info` and `history` objects alone.

## Objects

`info`:

```
id, name, service_version, api, os, kernel, arch, cpu_model, cpu_physical_cores,
cpu_logical_cores, memory_total, boot_time,
gpus: [{ id (PCI slot), vendor (nvidia|amd|intel|other), name, driver, boot_display }],
capabilities: ["processes", "displays", "history", "sensors", …]
```

`sample`:

```
t (Unix ms), uptime (s),
cpu: { usage (0–100), cores [], freq [], temp?, load [1, 5, 15 min] },
memory: { total, used, available, swap_total, swap_used },
gpus: [{ id, state (active|sleeping|unknown), usage?, mem_used?, mem_total?, temp?, power?,
         freq?, freq_max?, mem_freq?, fan? (%) }],
temps: [{ chip, kind (cpu|gpu|storage|wireless|board|battery|other), device?, label, value,
          high?, crit? }],
fans: [{ chip, label, rpm }],
battery?: { name, model?, percent?, status (charging|discharging|full|not_charging|unknown),
            power?, energy_now?, energy_full?, energy_design? (Wh), cycles?, time_left? (s) },
power: { on_ac?, profile?, profiles [], governor?, epp? },
disks: [{ device, mount, fs, total, available, removable }],
disk_io: [{ name, model?, read, write }],
net: [{ name, kind (wired|wireless|vpn|virtual), up, addrs [], rx, tx, rx_total, tx_total,
        speed? (Mbit/s) }],
displays: [{ connector, gpu, connected, active, dpms?, internal, vendor?, model?, year?,
             width_mm?, height_mm?, mode?, preferred?, max_refresh?, position? [x, y] }],
  mode: { width, height, refresh },
process_count?, processes?: [{ pid, ppid?, name, cmd, user?, cpu (% of the whole CPU), mem,
                               status, start (Unix s), read, write, gpu?, gpu_mem? }]
```

`history`: parallel arrays, oldest first, one entry per service tick:

```
t [], cpu [], cpu_temp [], mem [], swap [], rx [], tx [], read [], write [], battery_power [],
gpus: [{ id, usage [], temp [] }]
```

`rx`/`tx` sum wired and wireless links only, so VPN and bridge traffic isn't counted twice.
Missing values are `null`.

## Control

Everything below needs a token. Requests that change the computer also need `control = true` in
the service's settings, or the service answers 403 `{ "error": "control_disabled" }`. When polkit
refuses, it's 403 `{ "error": "not_permitted" }`; other failures are 502
`{ "error": "failed", "message": "…" }`. Successful actions answer `{ "ok": true }`.

| request | body | |
|---|---|---|
| `GET /api/v1/control` | | `{ enabled, power: { poweroff, reboot, suspend: "yes"\|"challenge"\|"no"\|"na", lock }, profiles: { active, available }, actions: [{ id, name, confirm }], volume }` |
| `POST /api/v1/control/power` | `{ action: "poweroff"\|"reboot"\|"suspend"\|"lock" }` | |
| `POST /api/v1/control/profile` | `{ profile }` | power-profiles-daemon |
| `POST /api/v1/control/signal` | `{ pid, signal: "term"\|"kill" }` | |
| `POST /api/v1/control/action` | `{ id }` | `{ exit_code, timed_out, output }` |
| `POST /api/v1/media` | `{ player, op: "play_pause"\|"next"\|"previous"\|"seek"\|"volume", value? }` | MPRIS; seek in seconds |
| `POST /api/v1/media/volume` | `{ level?, muted? }` | the default output, level 0–1 |
| `GET /api/v1/media/art?player=` | | the cover, when the player has a local file |
| `GET /api/v1/services` | | the services with their state, as in samples |
| `POST /api/v1/services` | `{ id?, name, url, unit?, scope? }` | add, or change one added from the app |
| `DELETE /api/v1/services/{id}` | | |
| `POST /api/v1/services/{id}/unit` | `{ op: "start"\|"stop"\|"restart" }` | needs control |
| `GET /api/v1/services/detected` | | `[{ port, local_only, process? }]`: ports something listens on |
| `GET /api/v1/units?scope=user\|system` | | `[{ name, description, active }]` |

Samples carry `services: [{ id, name, url, unit?, scope, editable, port?, up?, local_only,
unit_state? }]` while a client is connected, `power.ppd_profile` and `power.ppd_profiles`, and,
for a stream opened with `media=1`, `media: { players: [{ id, name, status, title?, artist?,
album?, length?, position?, volume?, has_art, can_next, can_previous, can_play, can_pause,
can_seek }], volume?: { level, muted } }`.
