# Elamon protocol, API 1

elamond, the Elamon service, serves HTTPS on port 47823. Its certificate is self-signed; clients
identify the service by the SHA-256 of that certificate (lowercase hex, 64 characters), not by a
name. A certificate the service renews, it signs over with the one before
([certificate rotation](#certificate-rotation)).

All bodies are JSON. Numbers in bytes are integers; rates are bytes per second; temperatures are
°C; frequencies MHz; power W. Clients must ignore fields they don't know: new fields are added
without changing the API number. A field is never renamed or given a new meaning: a better name
comes as a new field beside the old one ([names before 0.5](#names-before-05)).

Every shape below, with each optional field filled in, is in
[`elamond/fixtures`](../elamond/fixtures): the service's tests write those files and the app's
tests read them, so the two can't drift apart.

## Versions

- `api` (in `hello` and `info`) changes only if something incompatible ever has to; a client that
  sees a higher `api` than it knows should ask to be updated.
- `min_app` (in `hello` and `info`) is the oldest app version that understands this service.
- `version` / `service_version` is the service's own version; a client may expect a minimum one
  for newer features and fall back without them.

### Names before 0.5

What elamond 0.5 calls *links* were *services* before, and the service itself was the *service*:

| since 0.5 | before |
|---|---|
| `service_version` in `info` | `service_version` |
| `links` in samples | `services` |
| `"links"` in `capabilities` | `"services"` |
| `/api/v1/links`, `/api/v1/links/…` | `/api/v1/services`, `/api/v1/services/…` |
| error `no_such_link` | `no_such_service` |

For apps 0.4, elamond still sends the old fields beside the new ones, with the same values, and
answers the old routes as the new ones; they go only with a new `api`. Only the error code is
answered by its new name alone. A client reads the new name and falls back to the old one, and
knows both error codes. elamond 0.4 answers a path it doesn't have, like `/api/v1/links`, with 404
`not_found`: then the client asks for the same under `/api/v1/services`.

## Discovery

The service announces `_elamon._tcp` over mDNS with TXT records:

| key | |
|---|---|
| `id` | the service's id, 32 hex characters, stable across restarts |
| `api` | API version |
| `fp` | first 16 hex characters of the certificate's SHA-256 |

Since 0.5 only on the local network: on wired and wireless interfaces (and bridges, bonds and VLANs
over them) that the service listens on, with the addresses it listens on there; never on a VPN
or on a bridge for containers or virtual machines. Where none of its interfaces is one of those,
it announces nothing. After a [certificate rotation](#certificate-rotation), `fp` is the new
certificate's.

## Pairing

`elamond pair` makes a six-digit code, valid for 5 minutes, and prints it with a QR code holding

    elamon://pair?v=1&id=<id>&name=<name>&port=<port>&fp=<sha256>&code=<code>&h=<host>,<host>…

`h` lists the addresses the service listens on (`listen` in its settings), local network first,
then VPNs: of each interface its IPv4 addresses, or its IPv6 ones when it has no IPv4 address the
service listens on; never loopback or link-local ones. Listening on every address (`"::"`, the
default), bridges for containers and virtual machines are left out. Since 0.5, host names follow
the addresses: those in `names` in the settings, in their order, then the computer's Tailscale
name (MagicDNS, `Self.DNSName` of `tailscale status`, without its final dot) when Tailscale runs
with MagicDNS on and the service listens on one of the computer's Tailscale addresses. Each is
there once; the computer's mDNS name (`host.local`) only when `names` has it. `h` holds at most 8:
addresses first, so names never crowd them out, and `elamond pair` says what it left out. Without
anything in `h` the link would be refused, so `elamond pair` then prints only the code. Before
0.5, `h` was every IPv4 address of the computer's wired, wireless and VPN interfaces.

A client takes each entry of `h` as an IPv4 address, an IPv6 address (with or without brackets) or
a DNS name, and refuses the link whole for anything else or for more than 8. It reaches the
computer at any of them and accepts it only with the `id` and the certificate `fp` of the link, so
a name or address that leads to another computer is never taken for it. Stopping
`elamond pair` withdraws its code. The phone that pairs with it gets the role `elamond pair --role` named, or
`default_role` from the settings ([roles](#roles)).

An address may try 5 wrong codes; after that it is answered 429 `too_many_requests`, whatever it
sends, for as long as the code lives (an IPv6 address counts by its /64). Other addresses can still
pair until 20 wrong codes in all use the code up. Attempts are judged one at a time, and each wrong
one is answered after a second; neither that second nor a phone waiting to be let in (below) holds
up anyone else's attempt.

Since 0.5 the person at the computer lets the phone in. A phone that gives the right code is shown
at the running `elamond pair` (`Phone "<device>" from <address> asks to pair as <role>. Allow?
[y/N]`), and the service keeps its request open until the answer comes, 90 seconds at most. A yes
gives the usual answer; a no, no answer in time, an `elamond pair` that was stopped or is gone, or a
phone that hangs up is answered 403 `pair_declined`, and the code no longer works. While a phone
waits, its code is spoken for: any other attempt with it gets `no_pairing`. A client waits for the
answer for longer than those 90 seconds (the app 0.5 waits 100), and says meanwhile to confirm on
the computer. `elamond pair --yes`, for scripts, pairs whichever phone gives the code at once, as
before 0.5; a service before 0.5 never asks.

At most 64 phones are paired at once, keys of their own ([`rekey`](#post-apiv1rekey)) included.

### `POST /api/v1/pair` (no token)

```json
{ "code": "123456", "device": "Phone name" }
```

200: `{ "token": "…", "client_id": "…", "id": "<service id>", "name": "<computer name>", "role": "<role>" }`
(`role` since 0.5), after up to 90 seconds while the person at `elamond pair` decides.
403: `{ "error": "wrong_code" }`, `{ "error": "no_pairing" }` when no code is pending (or a phone
that gave it waits to be let in), `{ "error": "pair_declined" }` when the phone wasn't let in,
`{ "error": "too_many_phones" }` when 64 are paired already; 429 `{ "error": "too_many_requests" }`
from an address that tried too many wrong codes; 413 for a body over 4 KiB. `device` is cleaned
(controls and Unicode format characters taken out, 64 characters kept) and is `Phone` when nothing
is left.

The token goes in every other request as `Authorization: Bearer <token>`. Without a valid one
the service answers 401 `{ "error": "unauthorized" }`. A phone removed with `elamond revoke` gets
401 from then on, and any stream it has open ends.

The token is all the service knows a phone by: whatever the app asks before using it (unlocking
the phone, say) only the app enforces, so a token copied off a phone works without it. That is
what `elamond revoke` is for. Since 0.5 the app can carry the token to another phone (Export
pairings, sealed with a passphrase). Until that phone asks for a key of its own
([`rekey`](#post-apiv1rekey)), which the app does at its first connection after the import, the
service sees it as the phone the token was given to; afterwards each is an entry of its own, the
new one *added with the key of* the old. The two are a family: they share one rate of acting and
one share of the activity log. `elamond revoke ID` of the old one leaves the new one paired and
names it loudly; `elamond revoke ID --with-added` unpairs the whole family (by the old one's id
even once it is gone), and `elamond clients role ID ROLE --with-added` changes all their roles.
`elamond backup` and `elamond restore` carry the service's side: its id, certificate and key, and
the hashes of the tokens, so a restored service accepts the tokens it gave out before.

### `POST /api/v1/rekey`

Since 0.5: a key of its own for a phone that holds another's token, as one does after Import
pairings.

```json
{ "device": "Phone name" }
```

200, as pairing answers: `{ "token": "…", "client_id": "…", "id": "<service id>", "name": "<computer name>", "role": "<role>" }`.
It is a new entry among the paired phones, named `device`, with a new token and the role of the
entry whose token asked. That token stays good: the phone it came from keeps working until
`elamond revoke` of its own entry, which leaves the new one paired. `elamond clients` shows the
new entry as added with the key of the one that asked, and `elamond revoke` of that one names the
entries added with its key.

Any role may ask, and none is raised (a `role` in the body is ignored). A token gives one key of
its own: only an entry paired with a code may ask, and once; the entry that asked again, or the
new one, gets 403 `rekey_refused` (so a token copied to several phones gives one of them a key; the
others pair). 403 `too_many_phones` when 64 phones are paired. It needs no control, but counts as
acting: it is limited as acting is, the family's one rate ([how often](#how-often)), and recorded
in the [activity log](#activity-log) as `phone added: <device>` under the phone that asked, with the
new entry's id and role in `message`. 500 `storage` when the list of phones can't be written, or no
longer parses (it is never written over); 413 for a body over 4 KiB. A service before 0.5 answers
404 `not_found`: there the two phones stay one entry, and revoking it cuts off both. The app asks
each computer it imported at the first connection to it that works; after a 404 it keeps the shared
token and doesn't ask again until the app is restarted, after any other failure it asks again at a
later connection. A client takes `rekey_refused` and `too_many_phones` as it takes that 404: asking
again won't change them.

### `GET /api/v1/hello` (no token)

`{ "app": "elamon", "api": 1, "id": "…", "name": "…", "version": "0.4.0", "min_app": "0.3.0" }`

## Certificate rotation

`elamond rotate-cert` (since 0.5) gives the service a new key and certificate. Before the old key
is deleted, it signs a statement that the new certificate follows the old one. The service keeps
the last 5 statements and presents the new certificate within seconds (from the next connection;
those open keep theirs). Its id stays the same.

That is for routine renewal. A key that may have leaked mustn't vouch for its successor:
`elamond rotate-cert --no-statement` gives a new key and certificate that no statement leads to,
deleting the old key and the statements kept so far, so the route below answers `[]` and every
phone sees the certificate changed. Whoever has the old key can still pose as the computer to the
phones that pinned it, so they are unpaired and paired again.

### `GET /api/v1/identity/rotations` (no token)

The statements that lead to the certificate the service presents, oldest first; `[]` when it was
never rotated:

```json
[{ "format": "elamon-rotate-v1", "old_fp": "<sha256>", "new_fp": "<sha256>", "t": 1794396297,
   "old_cert": "<base64 DER>", "signature": "<base64 DER>" }]
```

| field | |
|---|---|
| `format` | the version of the signed text; only `elamon-rotate-v1` so far |
| `old_fp` | SHA-256 of `old_cert`, lowercase hex |
| `new_fp` | SHA-256 of the certificate that replaced it |
| `t` | Unix time of the rotation, seconds |
| `old_cert` | the old certificate, DER, standard base64 with padding |
| `signature` | ECDSA P-256 with SHA-256 (DER-encoded, as `SHA256withECDSA` gives it) by the old certificate's key over the signed text, standard base64 |

The signed text is these four lines in UTF-8, joined by `\n`, with no newline after the last:

    elamon-rotate-v1
    <old_fp>
    <new_fp>
    <t>

The route needs no token, like `hello` and `pair`: every statement proves itself, and a phone
needs them before it can trust the service enough to send its token. Every shape is in
[`rotations.json`](../elamond/fixtures/rotations.json), a real chain of three.

### Following a rotation

A client that meets another certificate than the one it pinned (`P`) on a computer it paired
with, before it reports the certificate changed:

1. reads `/api/v1/identity/rotations` over a connection that accepts any certificate and keeps
   the one presented (`C`), on an address where `hello` gives the paired id;
2. finds the statement whose `old_fp` is `P` and, from there on, checks each: `format` is one it
   knows, `old_fp` is where the one before ended (`P` for the first), SHA-256 of `old_cert` is
   `old_fp`, and `signature` verifies under `old_cert`'s public key (a P-256 key) over the signed
   text;
3. stops with success at the first statement whose `new_fp` is `C`: it pins `C` in place of `P`
   and goes on. Anything else, a broken step or a chain that never reaches `C`, means the
   certificate changed.

A phone that missed rotations follows as long as the one it pinned is among the last 5. Apps before
0.5 don't follow: after a rotation they say the certificate changed, and pair again. A service
before 0.5 answers the route 404 `not_found`.

## Roles

Since 0.5 each paired phone has a role. They are ordered, each allowing what the one before does
and more:

| role | |
|---|---|
| `glance` | `info`, `history`, `stream` and `snapshot` with the basics: CPU, memory, GPUs, temperatures, fans, disks, disk I/O, network, battery and power. `procs` and `media` are ignored (no `processes`, `process_count` or `media`), and `displays`, `links` and `services` are `[]`. And `GET /api/v1/control`, to learn its role |
| `monitor` | everything there is to read: processes with their commands and users, monitors, links with the ports in use and the units to pick from, media with their covers, the activity log |
| `operate` | adding, changing and removing links; starting, stopping and restarting their units, those the settings name; media and the volume; the power profile; the actions in the settings |
| `admin` | power (shut down, restart, sleep, lock) and signals to processes |

Any role may ask for a key of its own ([`rekey`](#post-apiv1rekey)), which keeps that role.

Phones paired before 0.5 have no role and are `admin`, which is what they could do then. The
person at the computer chooses: `elamond pair --role <role>`, otherwise `default_role` from the
settings (`admin` unless set); `elamond clients role` changes a paired phone's role, which applies
from its next request, and in a stream it has open from the next sample.

Acting on the computer also needs control, whatever the role: `control = true` in settings that
parse, and no `elamond control off`. A request the role doesn't allow is answered 403
`not_allowed`; one it allows while control is off, 403 `control_disabled`.

`GET /api/v1/control` gives the phone its `role`, and in `allowed` what it may do now:

| name | role | needs control | covers |
|---|---|---|---|
| `processes` | monitor | | `processes` and `process_count` in samples |
| `displays` | monitor | | `displays` in samples |
| `links` | monitor | | `links` in samples, `GET /api/v1/links`, `/api/v1/links/detected`, `/api/v1/units` |
| `media.read` | monitor | | `media` in samples, `GET /api/v1/media/art` |
| `log` | monitor | | `GET /api/v1/control/log` |
| `links.edit` | operate | | `POST /api/v1/links`, `DELETE /api/v1/links/{id}` |
| `units` | operate | yes | `POST /api/v1/links/{id}/unit` |
| `media` | operate | yes | `POST /api/v1/media` |
| `volume` | operate | yes | `POST /api/v1/media/volume` |
| `profile` | operate | yes | `POST /api/v1/control/profile` |
| `actions` | operate | yes | `POST /api/v1/control/action` |
| `power` | admin | yes | `POST /api/v1/control/power` |
| `signal` | admin | yes | `POST /api/v1/control/signal` |

A name is in `allowed` when the role allows it and, where it needs control, control is on; the
list is in this order. Whether the computer offers a thing is told beside it, in `power`,
`profiles`, `actions` and `volume`. A client ignores names it doesn't know: later versions may add
some.

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

`procs` and `media` are ignored for a `glance` phone ([roles](#roles)). When a phone's role
changes, its open stream follows from the next sample: processes come back a tick or two later,
as they do when first asked for.

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
and `processes` are left out of a sample, when they weren't asked for or the phone's role doesn't
allow them; a `glance` phone's samples also have `displays`, `links` and `services` empty
([`sample-glance.json`](../elamond/fixtures/sample-glance.json)).

`info`:

```
id, name, service_version, service_version (its old name), api, min_app, interval_ms (the service's
tick), history_seconds (how far history reaches), os, kernel, arch, cpu_model, cpu_physical_cores?,
cpu_logical_cores, memory_total, boot_time,
gpus: [{ id (PCI slot), vendor (nvidia|amd|intel|other), name, driver, boot_display }],
capabilities: ["processes", "displays", "history", "sensors", "links", …]
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
links: [{ id, name, url, unit?, scope (user|system), editable, port?, up?, local_only,
          unit_state? }],
services (links under their old name),
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

`history`: parallel arrays, oldest first, one entry per tick of the service, reaching
`history_seconds` back:

```
t [], cpu [], cpu_temp [], mem [], swap [], rx [], tx [], read [], write [], battery_power [],
gpus: [{ id, usage [], temp [] }]
```

`rx`/`tx` sum wired and wireless links only, so VPN and bridge traffic isn't counted twice.
Missing values are `null`.

## Control

Everything below needs a token, and the [role](#roles) for it. Requests that change the computer
also need control: `control = true` in the service's settings and no `elamond control off` on the
computer, or the service answers 403 `control_disabled`. Since 0.5, settings that stop parsing
hold control off too until they parse again (the service keeps the settings it read before for the
rest). Adding, changing and removing links only changes the list the app shows: it needs the
`operate` role, not control. The routes under `/api/v1/links` answer under `/api/v1/services` too.

| request | body | |
|---|---|---|
| `GET /api/v1/control` | | `{ enabled, off_reason?, power: { poweroff, reboot, suspend: "yes"\|"challenge"\|"no"\|"na", lock }, profiles?: { active, available }, actions: [{ id, name, confirm }], volume, role, allowed: [] }` |
| `GET /api/v1/control/log?limit=` | | the activity log, newest first, at most `limit` entries (all kept, up to 500, by default) |
| `POST /api/v1/control/power` | `{ action: "poweroff"\|"reboot"\|"suspend"\|"lock" }` | |
| `POST /api/v1/control/profile` | `{ profile }` | power-profiles-daemon |
| `POST /api/v1/control/signal` | `{ pid, signal: "term"\|"kill", start? }` | never the service, its parents or PID 1 |
| `POST /api/v1/control/action` | `{ id }` | `{ exit_code?, timed_out, output }` |
| `POST /api/v1/media` | `{ player, op: "play_pause"\|"next"\|"previous"\|"seek"\|"volume", value? }` | MPRIS; seek in seconds |
| `POST /api/v1/media/volume` | `{ level?, muted? }` | the default output, level 0–1 |
| `GET /api/v1/media/art?player=` | | the cover (PNG, JPEG or WebP, 4 MiB at most), when the player has it in a local file; reading, so no control needed (before 0.5 it was) |
| `GET /api/v1/links` | | the links with their state, as in samples |
| `POST /api/v1/links` | `{ id?, name, url, unit?, scope? }` | add, or change one added from the app; answers the saved `{ id, name, url, unit, scope }` |
| `DELETE /api/v1/links/{id}` | | links from the settings files can't be changed or removed here |
| `POST /api/v1/links/{id}/unit` | `{ op: "start"\|"stop"\|"restart" }` | needs control, and a unit the settings name |
| `GET /api/v1/links/detected` | | `[{ port, local_only, process? }]`: ports something listens on |
| `GET /api/v1/units?scope=user\|system` | | `[{ name, description, active }]`: the loaded units of that scope a link may be given, those the settings name |

In `GET /api/v1/control`, `enabled` says whether control is on now, `elamond control off`
included; `role` and `allowed` are the asking phone's ([roles](#roles)), new in 0.5. `off_reason`
(since 0.5) says why control is off, and is `null` while it's on: `settings` (`control = false`),
`held_off` (`elamond control off`) or `settings_unreadable` (the settings files stopped parsing;
[`control-off.json`](../elamond/fixtures/control-off.json)). `power`, `profiles`, `actions` and
`volume` go only as far as the role: beyond it they read as on a computer without them (`"na"`
and `false`, `null`, `[]`, `false`), so an app from before roles offers no more than the phone may
do.

A link opens a web page: its `url` is an `http://` or `https://` one, or a bare host and port
(`{host}:8096`), which a client opens as `http://`. Since 0.5 another scheme (`javascript:`,
`intent:`, `file:`, …) is refused with 400 `bad_url`, and a link that has one anyway (in the
settings, or in `links.json` from before) is left out of what phones get.

Since 0.5 a link's unit is only ever one the computer's settings name: on a `[[links]]` entry, or
in `units`, in the same scope. Only service units (`NAME.service`), never one whose name starts
with `-` or the Elamon service's own, and a template's instance (`name@instance.service`) only as
the settings name it. `POST /api/v1/links` with any other unit is answered 403 `not_allowed` (a
malformed name 400 `bad_unit`); `POST /api/v1/links/{id}/unit` asks again, so a unit stored before
or no longer named is refused there with 403 `not_allowed` and its state isn't read
(`unit_state` is `null`). System units also need polkit's leave
([the guide](guide.md#power-the-profile-and-system-units-polkit)).

Action ids come from the action's name and command, not its place in the settings, so a phone
holding an old list can't run a different command after the settings change; an id that no
longer exists answers 404 `no_such_action`. An action's output keeps its first 4 KiB and last
12 KiB; a timed-out action answers with what it printed so far.

A signal's `start` is the process's start time as samples give it (Unix s). With it, the service
signals only the process that started then: if the pid now belongs to another, it answers 409
`process_changed` and signals nothing. Without it, whatever has the pid gets the signal, as
before 0.5.

Successful actions answer `{ "ok": true }`.

### How often

A phone may make 10 acting requests (every one in the table but the `GET`s, and
[`rekey`](#post-apiv1rekey)) in a row, then one more each second; beyond that it gets 429
`too_many_requests`. Reading isn't counted. Since 0.5 the rate is a family's: a phone and the keys
of their own its token gave share it.

### Connections

The service holds 256 connections at once, and 16 from one address (an IPv6 one by its /64) that
haven't yet made a request with a paired phone's token; more are closed as they come, before the
TLS handshake. A connection has 10 seconds for its TLS handshake, then 15 for its first request,
and 15 for the headers of each request after it, an idle wait between requests included: a client
reconnects as it would after any closed connection. A stream, once its headers are read, isn't
bound by these.

### Activity log

Every acting request is recorded, let through or refused, with how it ended. The service keeps up
to 500, in a private file in its data directory (`activity.jsonl`), so they survive restarts.
Since 0.5 a family (a phone and the keys of their own its token gave) keeps 100 of them at most:
its newest push out its own oldest, never another phone's. Each entry:

```
{ t (Unix ms), phone (its name then), client_id, what, result ("ok" or the error code),
  message? (what failed, or how an action ended: "exit 0", "timed out"),
  family? (the client_id its key came from, for a phone that got one with another's token) }
```

`what` reads like `power reboot`, `profile balanced`, `signal term 1234 firefox`,
`action Back up now`, `unit restart nginx.service`, `media play_pause`, `volume 40 %`,
`link add Jellyfin`, `link remove Jellyfin`, `phone added: Pixel 9` (a key of its own, the
`message` `id <client_id>, <role>`). Of a run of 429 refusals only the first is
recorded. A request whose body the service can't read at all isn't. Requests that were let through
are also in the journal (`journalctl --user -u elamond`), as before.

## Errors

Errors are `{ "error": "<code>" }` with these statuses (the same list is in
[`elamond/fixtures/errors.json`](../elamond/fixtures/errors.json)):

| status | codes |
|---|---|
| 400 | `unknown_signal`, `empty_command`, `no_unit`, `bad_name`, `bad_url`, `bad_unit`, `bad_scope` |
| 401 | `unauthorized` |
| 403 | `no_pairing`, `wrong_code`, `pair_declined` (the person at `elamond pair` didn't let the phone in), `too_many_phones` (64 are paired), `rekey_refused` (this token gave its key of its own already, or came of one), `control_disabled`, `not_permitted` (polkit refused, or a protected process), `not_allowed` (the phone's role doesn't allow it, or the settings don't name the unit) |
| 404 | `not_found`, `no_such_process`, `no_such_action`, `no_such_link` (`no_such_service` before 0.5), `no_art` |
| 409 | `process_changed` (a signal's `start` names another process than the pid's) |
| 429 | `too_many_requests` (acting too often, or too many wrong pairing codes from one address) |
| 500 | `storage` |
| 502 | `failed`, with `"message"` |

`not_allowed`, `process_changed`, `too_many_requests`, `pair_declined`, `too_many_phones` and
`rekey_refused` are new in 0.5; an app from before them takes them as an error it has no words
for.

`failed` carries a `message`: either one of `unknown_action`, `unknown_operation`,
`unknown_player`, `unknown_profile`, `no_value`, `no_track`, `no_session`, `no_system_bus`,
`no_session_bus`, `no_power_profiles`, `volume_failed`, `mute_failed`, `timed_out`, or the
system's own error text (D-Bus, a command). A request whose JSON or query the service can't read
at all gets a plain-text 400, 415 or 422, and a body over its limit (4 KiB for `pair` and `rekey`)
a plain-text 413.
