# Elamon protocol, API 2

elamond, the Elamon service, serves HTTPS on port 47823. Its certificate is self-signed; clients
identify the service by the SHA-256 of that certificate (lowercase hex, 64 characters), not by a
name. A certificate the service renews, it signs over with the one before
([certificate rotation](#certificate-rotation)).

All bodies are JSON. Clients must ignore fields they don't know: new fields are added without
changing the API number. A field keeps its name and its meaning for as long as the API number stays
the same.

What a client shows is not in this protocol but in the service's settings ([config.md](config.md)):
the service describes dashboards of widgets ([`GET /api/v1/ui`](#get-apiv1ui)), sends the values
of the one on screen, ready to draw ([`GET /api/v1/stream`](#get-apiv1stream)), and runs the
actions its settings define ([`POST /api/v1/actions/{id}`](#post-apiv1actionsid)). A new reading,
a new integration or another layout is a change to the service alone; the client changes only for a
new kind of widget.

Every shape below, with each optional field filled in, is in
[`elamond/fixtures`](../elamond/fixtures): the service's tests write those files and the app's
tests read them, so the two can't drift apart.

## Versions and what each side can do

- `api` (in `hello`) changes only if something incompatible has to. API 2 came with elamond 0.6;
  nothing of API 1 is served any more, and a client that sees another `api` than its own asks for
  the app or the service to be updated.
- `widgets` (in `hello`) is the version of the dictionary of widget types and their keys. It grows:
  a type or a key is added, never changed.
- `min_app` is the oldest app version that understands this service.
- `version` is the service's own.

### `GET /api/v1/hello` (no token)

```json
{ "app": "elamon", "api": 2, "widgets": 1, "id": "…", "name": "…", "version": "0.6.0",
  "min_app": "0.6.0", "features": ["control", "media", "power", "power_profiles", "processes", "units"] }
```

`features` says what this computer can offer at all (whatever a client's role), sorted:
`control` (`control = true` in the settings), `power` (logind can power off, restart, sleep or
lock), `power_profiles` (power-profiles-daemon or tuned-ppd), `units` (systemd), `media` (a session
bus for MPRIS), `processes`, `displays` (a GPU whose monitors DRM tells of), `nvml` (an NVIDIA GPU
NVML can read). A client ignores those it doesn't know.

### The client says what it is

Every request with a token carries `Elamon-Client: <kind>/<version>`: `android/0.6.0`, later
`web/0.8.0`. The service shows it in `elamond clients`, and gives each client the profile's part for
its kind. The kinds are `android` and `web`; a request without the header, with one that isn't
`<kind>/<version>` (a version of letters, digits and `.+-_`), or with a kind the service doesn't
know, is taken as `android` of an unknown version. The service writes the version down when it
changes: at once, then at most once every 5 minutes for a client that keeps changing it.

What widget types a client draws it says in `GET /api/v1/ui?widgets=…`; the service describes the
dashboards with those alone ([below](#what-a-client-cant-draw)), in that `ui` and in what the
client's streams and snapshots send from then on. A type the service doesn't know is left out, as
if the client hadn't listed it.

## Discovery

The service announces `_elamon._tcp` over mDNS with TXT records:

| key | |
|---|---|
| `id` | the service's id, 32 hex characters, stable across restarts |
| `api` | API version |
| `fp` | first 16 hex characters of the certificate's SHA-256 |

Only on the local network: on wired and wireless interfaces (and bridges, bonds and VLANs
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
default), bridges for containers and virtual machines are left out. Host names follow the
addresses: those in `names` in the settings, in their order, then the computer's Tailscale
name (MagicDNS, `Self.DNSName` of `tailscale status`, without its final dot) when Tailscale runs
with MagicDNS on and the service listens on one of the computer's Tailscale addresses. Each is
there once; the computer's mDNS name (`host.local`) only when `names` has it. `h` holds at most 8:
addresses first, so names never crowd them out, and `elamond pair` says what it left out. Without
anything in `h` the link would be refused, so `elamond pair` then prints only the code.

A client takes each entry of `h` as an IPv4 address, an IPv6 address (with or without brackets) or
a DNS name, and refuses the link whole for anything else or for more than 8. It reaches the
computer at any of them and accepts it only with the `id` and the certificate `fp` of the link, so
a name or address that leads to another computer is never taken for it. Stopping
`elamond pair` withdraws its code. The phone that pairs with it gets the role `elamond pair --role` named, or
`default_role` from the settings ([roles](#roles)), and the profile `elamond pair --profile` named, or `default_profile` ([profiles](#profiles)).

An address may try 5 wrong codes; after that it is answered 429 `too_many_requests`, whatever it
sends, for as long as the code lives (an IPv6 address counts by its /64). Other addresses can still
pair until 20 wrong codes in all use the code up. Attempts are judged one at a time, and each wrong
one is answered after a second; neither that second nor a phone waiting to be let in (below) holds
up anyone else's attempt.

The person at the computer lets the phone in. A phone that gives the right code is shown
at the running `elamond pair` (`Phone "<device>" from <address> asks to pair as <role>. Allow?
[y/N]`), and the service keeps its request open until the answer comes, 90 seconds at most. A yes
gives the usual answer; a no, no answer in time, an `elamond pair` that was stopped or is gone, or a
phone that hangs up is answered 403 `pair_declined`, and the code no longer works. While a phone
waits, its code is spoken for: any other attempt with it gets `no_pairing`. A client waits for the
answer for longer than those 90 seconds (the app waits 100), and says meanwhile to confirm on
the computer. `elamond pair --yes`, for scripts, pairs whichever phone gives the code at once.

At most 64 phones are paired at once, keys of their own ([`rekey`](#post-apiv1rekey)) included.

### `POST /api/v1/pair` (no token)

```json
{ "code": "123456", "device": "Phone name" }
```

200: `{ "token": "…", "client_id": "…", "id": "<service id>", "name": "<computer name>", "role": "<role>", "profile": "<profile id>" }`,
after up to 90 seconds while the person at `elamond pair` decides.
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
what `elamond revoke` is for. The app can carry the token to another phone (Export
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

A key of its own for a phone that holds another's token, as one does after Import pairings.

```json
{ "device": "Phone name" }
```

200, as pairing answers: `{ "token": "…", "client_id": "…", "id": "<service id>", "name": "<computer name>", "role": "<role>", "profile": "<profile id>" }`.
It is a new entry among the paired phones, named `device`, with a new token and the role and profile of the
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
longer parses (it is never written over); 413 for a body over 4 KiB. The app asks each computer
it imported at the first connection to it that works. After `rekey_refused` or `too_many_phones`
it keeps the shared token, so there the two phones stay one entry and revoking it cuts off both,
and doesn't ask again until the app is restarted: asking again won't change them. After any other
failure it asks again at a later connection.

## Certificate rotation

`elamond rotate-cert` gives the service a new key and certificate. Before the old key
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

A phone that missed rotations follows as long as the one it pinned is among the last 5.

## Roles

Each paired client has a role. They are ordered, each allowing what the one before does and more:

| role | sees | may run |
|---|---|---|
| `glance` | the basics: CPU, memory, GPUs, temperatures, fans, disks, disk I/O, network, battery, power, the system | nothing |
| `monitor` | everything there is to read: also processes with their commands and users, monitors, links, media with their covers, the activity log | nothing |
| `operate` | | actions of kind `profile`, `units`, `media` and `actions` |
| `admin` | | `power` and `signal` too |

Any role may ask for a key of its own ([`rekey`](#post-apiv1rekey)), which keeps that role.

The role filters the values themselves, on the service: a widget, a table or a dashboard whose
values the role doesn't see isn't described to the client at all, nor are its values sent. A
dashboard's or a widget's `role` in the settings can only ask for more.

The person at the computer chooses: `elamond pair --role <role>`, otherwise `default_role` from
the settings (`admin` unless set); a client with no role in the list of paired clients is `admin`.
`elamond clients role` changes a paired client's role, which applies from its next request; a stream
it has open gets [`ui`](#get-apiv1stream) at once.

Acting on the computer also needs control, whatever the role: `control = true` in settings that
parse, and no `elamond control off`. A request the role doesn't allow is answered 403
`not_allowed`; one it allows while control is off, 403 `control_disabled`.

## Profiles

Each paired client also has a profile: how it looks and behaves ([config.md](config.md#profiles)).
`elamond pair --profile <id>`, otherwise `default_profile` from the settings; `elamond clients
profile <id|all> <profile>` changes it, from the client's next request (a stream it has open gets
`ui` at once). A client whose profile the settings no longer have gets `default_profile`.

## `GET /api/v1/ui`

What the client shows: its profile, its tabs, the dashboards and the actions, all as far as its role
goes. Query: `widgets`, the widget types the client draws, comma-separated
(`widgets=value,gauge,meter,status,graph,list,table,button,toggle,select,slider,text,group,media`).

```json
{
  "version": "5c1f0e2a9d3b7a41",
  "service": { "id": "…", "name": "…", "version": "0.6.0", "interval_ms": 1000, "history_seconds": 600 },
  "role": "admin",
  "control": { "enabled": true, "off_reason": null },
  "profile": { "id": "owner", "name": "Owner", "theme": "dark", "pure_black": true, "animations": true,
               "interval_ms": 1000, "fahrenheit": false, "network_bits": false, "confirm_actions": true,
               "unlock": { "for": ["power", "signal", "actions", "units"], "grace": "every_time" },
               "android": { "dynamic_color": true, "blur": true, "haptics": true, "keep_screen_on": false } },
  "tabs": ["overview", "processes", "sensors", "control"],
  "header": "header",
  "dashboards": [{ "id": "overview", "title": "Overview", "icon": "overview", "widgets": [ … ] }, …],
  "actions": [{ "id": "power.reboot", "name": "Restart", "kind": "power", "icon": "restart",
                "confirm": true, "unlock": true, "params": [] }, …]
}
```

- `version` changes whenever anything else in the document would: the settings, the client's role
  or profile, control turned on or off, a GPU, disk, sensor or interface that comes or goes (a
  label or title that holds values follows them within 5 seconds). A
  client keeps the last document of each computer (a cache, not settings) to start at once and to
  show while offline, and fetches it again when a stream says `ui` with another version.
- `profile` holds the part every client reads, and the part for this client's kind (`android`)
  only.
- `tabs` are the profile's dashboards the role sees, in order (one for each item of a dashboard
  with `for_each`); `dashboards` the tabs, then the header, then the dashboards their widgets
  open, and those theirs open, each once and as far as the role sees. A dashboard the role sees
  none of the widgets of isn't there, nor an `open` to a dashboard that isn't.
- `header` is the profile's `header` dashboard, whose widgets a client draws as small chips under
  the computer's name on every tab; `null` when the profile has none or the role sees none of it.
- `actions` are those the role may run and the computer can do. `confirm` and `unlock` are final:
  the action's own, the profile's `confirm_actions` and `unlock.for` already applied. A client asks
  before running an action with `confirm`, unlocks the phone first for one with `unlock` (within the
  profile's `unlock.grace`, an unlock for a kind covers the kinds after it in
  `power`, `signal`, `actions`, `units`, `profile`, `media`), and asks for the values of `params` a
  widget doesn't fix.
- `control.off_reason`: `null` while control is on, else `settings` (`control = false`),
  `held_off` (`elamond control off`) or `settings_unreadable` (the settings files stopped
  parsing; the service keeps those it read before). While it's off, action widgets are drawn
  disabled.

### Widgets in `ui`

Each widget as [config.md](config.md#widgets) describes it, resolved: templates applied, `for_each`
spread out, `when` decided, strings without values filled in, defaults written out. Every one has
an `id` unique in the document (`overview/cpu`, `overview/gpus.nvidia`), its `type` and `span`.
Values aren't named by address: the stream sends each widget's state by its id. The keys a client
reads, beside `id`, `type`, `span`, `label`, `icon`, `color` and `open`:

| type | keys |
|---|---|
| `value`, `gauge`, `meter`, `status` | `unit`, `format`, `min`, `max` (a number; one that changes comes in the state), `spark` (`value`) |
| `bars` | `unit`, `format`, `max` |
| `graph` | `series: [{ label, color, unit, format }]`, `range` (s), `height`, `min`, `max`, `floor` |
| `list` | `items: [{ label, unit, format }]` |
| `table` | `source`, `columns: [{ field, label, format, unit, bar: { max }, grow }]` (`max` a number, a field or `"top"`), `sort: { field, desc }`, `sorts`, `sort_labels` (field → the label of its chip; a field without one, its column's label or the field), `search` (bool), `search_fields` (the fields a client searches, when `search`: rows carry them), `hide_empty`, `row_open`, `row_actions: [{ action, label, params }]`, `expand: [{ field, label, format, unit }]` |
| `button` | `action`, `params`, `open` |
| `toggle` | `action`, `param`, `on`, `off` |
| `select`, `slider` | `action`, `param` |
| `text` | `text` (when it holds no values; otherwise it comes in the state), `level` |
| `group` | `widgets`, `collapsed` |
| every type | `caption` (`true` when it holds values: its text comes in the state), `collapsed` (`table`) |
| `media` | |

### What a client can't draw

A widget type the client didn't list in `widgets` is replaced by the nearest it did: `gauge` and
`meter` (and `status`) by `value`, `select` by buttons in a `group` (each button's id
`<id>/<value>`), `slider` by `select` or those buttons (11 of its values at most), a `group` by
the widgets in it… and where there's none, by a `text` with `level` `warning` saying the app needs
updating to show it. An unknown key on a known type, a client ignores.

## `GET /api/v1/stream`

Newline-delimited JSON (`application/x-ndjson`), one object per line, each with a `type`. Query:

| name | default | |
|---|---|---|
| `dashboard` | the first tab | the dashboard on screen: values are read and sent for it alone; one the client doesn't get (any more) is the first tab, and the `data` lines say which they carry |
| `interval` | the profile's `interval_ms` | milliseconds between `data` lines, from the service's `interval_ms` up to 60 000 |
| `widgets` | those of the client's last `GET /api/v1/ui` | the widget types it draws, as `ui` takes them |

Lines:

1. `open`, first: `{ "type": "open", "stream": "<id>", "ui": "<version>" }`. The id names this
   stream for [changing what it sends](#post-apiv1streamid); `ui` is the version of the client's
   `ui` now: a client holding another fetches it.
2. `data`, every interval: `{ "type": "data", "t": <Unix ms>, "dashboard": "<id>", "widgets": { … } }`,
   the state of each widget of the dashboard by its id, nested groups included, and of the
   header's widgets (`header/os`…) whatever dashboard is on screen. The first after opening, or
   after the dashboard changed, also has `history`: the graphs' points so far.
3. `ui`, whenever the client's `ui` changes: `{ "type": "ui", "ui": "<version>" }`. The stream goes
   on, with the dashboard it had (or the first tab, when that one is gone for this client).

A widget's state:

| type | state |
|---|---|
| `value`, `gauge`, `meter`, `status` | `{ "v": 42.5, "text": "On battery", "level": "warning", "blend": 0.4, "max": 16000000000, "caption": "4.1 GiB of 16 GiB" }`: `v` the value (number, string, bool or `null` when missing) in the widget's unit; `text` when `map` gave it a word, or the value is missing and the widget has `empty` (filled in); `level` and `blend` ([levels](config.md#levels)); `max` when it is an address; `caption` filled in |
| `graph` | `{ "v": [1.5, 0.2], "max": 16000000000, "text": "Asleep" }`, the newest point of each series, at the line's `t`; `max` when it is an address; `text`, the widget's `empty` while the first series has no value |
| `bars` | `{ "v": [12.5, 80.0, …], "labels": ["0", "1", …], "levels": ["normal", "critical", …] }`; `max` when it is an address |
| `list` | `{ "items": [{ "v": …, "text": …, "level": … }] }`, in the order of `items` |
| `table` | `{ "rows": [{ "key": "4242", "level": "normal", "f": { "pid": 4242, "name": "firefox", … } }], "total": 312 }`: at most `limit` rows in the table's order, each with the fields its columns, `expand`, `row_open` and `row_actions` need; `total`, how many there are in all |
| `button` | `{ "enabled": true }` |
| any, with a `caption` that holds values | `"caption": "…"` beside the rest |
| `toggle`, `select`, `slider` | `{ "v": "server", "enabled": true }` |
| `text` | `{ "text": "…", "level": "normal" }` when it holds values |
| `group` | none |
| `media` | `{ "players": [ … ], "volume": { "level": 40.0, "muted": false } }` (%) |

Numbers in states come rounded to one decimal (`blend` to three); a client formats them by the
widget's `format` and `unit` ([config.md](config.md#formats)).

`history`: `{ "<widget id>": { "t": [ … ], "v": [[ … ], [ … ]] } }` for each `graph` and each
`value` with `spark`, oldest first, one array per series, `null` where a value was missing,
reaching the graph's `range` back (a spark's: all the service keeps). A client appends each `data`
line's `v` at its `t` and drops what falls out of the range. Levels are `normal`,
`warning` and `critical`; `blend` (0–1) is how far it has turned toward the next and is left out at 0.

A media player: `{ id, name, status (Playing|Paused|Stopped), title?, artist?, album?, length? (s),
position? (s), volume? (%), has_art, can_next, can_previous, can_play, can_pause, can_seek }`; `volume`
of the computer is left out where it can't be read.

The service reads costlier sources (NVML, DRM, the process list, MPRIS) only while some client's
dashboard on screen needs them, and lets a powered-down GPU sleep. A table of processes fills a tick
after it is first asked for: their CPU and disk use are rates, which need two readings.

### `POST /api/v1/stream/{id}`

Changes what an open stream of this client sends, from its next line, without reconnecting:

```json
{ "dashboard": "cpu", "interval": 2000, "sort": { "processes/table": "mem" } }
```

Each key is optional. `sort` asks a table, by its widget id, for another of its `sorts`
(descending); `null` in place of a field (`{ "processes/table": null }`) returns the table to its own
`sort`, field and direction. A new dashboard or order comes with the next reading, not after an
interval; a new dashboard's first line has `history`. 200 `{ "ok": true }`; 404 `no_such_stream`
when the stream has ended or isn't this client's; 404 `no_such_dashboard` for one not in the
client's `ui`; 400 `bad_sort` with the table's id in `message`. A refused request (400 `bad_sort`,
404 `no_such_dashboard`) applies none of its keys: the stream goes on as it was. A body over 4 KiB
is a plain-text 413.

## `GET /api/v1/snapshot`

One `data` object (without `type`, with `history` unless `history=0`) for `dashboard=<id>` (the
first tab without it), after waiting a few ticks for a reading that has everything it needs; 404
`no_such_dashboard` for one not in the client's `ui`. `widgets` as for the stream.

## `POST /api/v1/actions/{id}`

```json
{ "params": { "mode": "server" } }
```

Runs an action of `ui`'s `actions`. The body may be left out when there are no parameters to
give. Each parameter is checked against the action's: a choice among its choices, a number in its
range on its step, an integer, a number, a string, a bool; one missing without a default, one of
the wrong kind, or one the action hasn't, is 400 `bad_param` with the parameter's name in
`message`. 200: `{ "ok": true }`, and for an action the settings define `{ "ok": true,
"exit_code": 0, "timed_out": false, "output": "…" }` (its first 4 KiB and last 12 KiB, also when it
timed out): `ok` says it ran, `exit_code` (`null` when a signal ended it) and `timed_out` how it
ended. 404 `no_such_action` for an id the computer doesn't have or can't do now; 403 `not_allowed`
for one it can that the client's role may not run (so not in its `ui`), `control_disabled`,
`not_permitted`; 409 `process_changed`; 429 `too_many_requests`; 502 `failed` with a `message`.
They are checked in this order: the rate, the role, control, then the parameters.

`process.signal` with `start` signals only the process that started then: if the pid now belongs to
another, 409 `process_changed`; a negative `start` is 400 `bad_param`. `unit.*` act on the unit of the link given, only one the settings
name; system units also need polkit's leave ([the guide](guide.md#power-the-profile-and-system-units-polkit)).

## `GET /api/v1/media/art?player=`

The cover (PNG, JPEG or WebP, 4 MiB at most), when the player has it in a local file; for roles that
see media. 404 `no_art`.

### How often

A client may make 10 acting requests (actions, and [`rekey`](#post-apiv1rekey)) in a row, then one
more each second; beyond that it gets 429 `too_many_requests`. Reading isn't counted. The rate is a
family's: a client and the keys of their own its token gave share it.

### Connections

The service holds 256 connections at once, and 16 from one address (an IPv6 one by its /64) that
haven't yet made a request with a paired client's token; more are closed as they come, before the
TLS handshake. A connection has 10 seconds for its TLS handshake, then 15 for its first request,
and 15 for the headers of each request after it, an idle wait between requests included: a client
reconnects as it would after any closed connection. A stream, once its headers are read, isn't
bound by these. A client has 4 streams open at most; a fifth ends its oldest.

### Activity log

Every acting request is recorded, let through or refused, with how it ended. The service keeps up
to 500, in a private file in its data directory (`activity.jsonl`), so they survive restarts.
A family keeps 100 of them at most: its newest push out its own oldest, never another client's.
Clients read it as the `activity` collection (role `monitor`). Each entry:

```
{ t (Unix ms), phone (the client's name then), client_id, what, result ("ok" or the error code),
  message? (what failed, or how an action ended: "exit 0", "timed out"),
  family? (the client_id its key came from, for a client that got one with another's token) }
```

`what` reads like `power reboot`, `profile balanced`, `signal term 1234 firefox`,
`action Mode server`, `unit restart nginx.service`, `media play_pause`, `volume 40 %`,
`phone added: Pixel 9`. Of a run of 429 refusals only the first is recorded. Requests that were let
through are also in the journal (`journalctl --user -u elamond`).

## Errors

Errors are `{ "error": "<code>" }`, some with a `message`, with these statuses (the same list is in
[`elamond/fixtures/errors.json`](../elamond/fixtures/errors.json)):

| status | codes |
|---|---|
| 400 | `bad_param` (with `message`, the parameter), `bad_sort`, `unknown_signal` |
| 401 | `unauthorized` |
| 403 | `no_pairing`, `wrong_code`, `pair_declined`, `too_many_phones` (64 are paired), `rekey_refused`, `control_disabled`, `not_permitted` (polkit refused, or a protected process), `not_allowed` (the client's role doesn't allow it) |
| 404 | `not_found`, `no_such_process`, `no_such_action`, `no_such_dashboard`, `no_such_stream`, `no_art` |
| 409 | `process_changed` |
| 429 | `too_many_requests` |
| 500 | `storage` |
| 502 | `failed`, with `message` |

`failed` carries a `message`: either one of `unknown_player`, `no_value` (`volume.set` with
neither `level` nor `muted`), `no_track`, `no_session`, `no_system_bus`, `no_session_bus`,
`no_power_profiles`, `volume_failed`, `mute_failed`, `timed_out`, or the system's own error text
(D-Bus, a command). A request whose JSON
or query the service can't read at all gets a plain-text 400, 415 or 422, and a body over its limit
(4 KiB for `pair`, `rekey`, `actions` and `stream`) a plain-text 413.
