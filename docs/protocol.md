# Elamon protocol, API 2

elamond, the Elamon service, serves HTTPS on port 47823. Its certificate is self-signed; clients
identify the service by the SHA-256 of that certificate (lowercase hex, 64 characters), not by a
name. The certificate holds until `elamond rotate-cert` replaces it with another
([a new certificate](#a-new-certificate)), which every phone is then paired again with.

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
tests read them, so the two can't drift apart. Those elamond 0.7 adds are in
[`elamond/fixtures/0.7`](../elamond/fixtures/0.7) until the service writes them and the app reads
them ([below](#what-came-with-elamond-07)), those 0.8 adds in
[`elamond/fixtures/0.8`](../elamond/fixtures/0.8) ([below](#what-came-with-elamond-08)), and
those 0.8.1 adds in [`elamond/fixtures/0.8.1`](../elamond/fixtures/0.8.1)
([below](#what-came-with-elamond-081)).

## Versions and what each side can do

- `api` (in `hello`) changes only if something incompatible has to. API 2 came with elamond 0.6;
  nothing of API 1 is served any more, and a client that sees another `api` than its own asks for
  the app or the service to be updated.
- `widgets` (in `hello`) is the version of the dictionary of widget types and their keys. It grows:
  a type or a key is added, never changed. 1 came with elamond 0.6; 2 with 0.7, which added the
  type `timeline` and the key `ranges` of `graph` and `timeline` (with
  [`/history`](#get-apiv1history) behind it); 3 with 0.8, which added the type `log` (with
  [`/log`](#get-apiv1log) behind it) and row actions that open a dashboard (`open`). A client
  needs it for nothing it draws, as the service describes only the types the client lists and a
  client ignores keys it doesn't know; it says which dictionary the service speaks, so a client
  can tell why it never sees a type. A key that changes only how a client words what it draws
  comes without a new number: elamond 0.8.1 added `legend` of `timeline` to 3.
- `min_app` is the oldest app version that understands this service.
- `version` is the service's own.

### `GET /api/v1/hello` (no token)

```json
{ "app": "elamon", "api": 2, "widgets": 3, "id": "…", "name": "…", "version": "0.8.1", "min_app": "0.6.0" }
```

Anyone on the network may ask, so it says which service this is and which versions it speaks, and
nothing of what the computer can do: that a paired client reads in `ui` (`service.features`).

### The client says what it is

Every request with a token carries `Elamon-Client: <kind>/<version>`: `android/0.6.0`, later
`web/0.8.0`. The service shows it in `elamond clients`, with the address the client's requests came
from last, and gives each client the profile's part for its kind. The kinds are `android` and
`web`; a request without the header, with one that isn't `<kind>/<version>` (a version of letters,
digits and `.+-_`), or with a kind the service doesn't know, is taken as `android` of an unknown
version. The service writes the version and the address down when they change: at once, then at
most once every 5 minutes for a client that keeps changing them.

What widget types a client draws it says in `GET /api/v1/ui?widgets=…`; the service describes the
dashboards with those alone ([below](#what-a-client-cant-draw)), in that `ui` and in what the
client's streams and snapshots send from then on. A type the service doesn't know is left out, as
if the client hadn't listed it.

### What came with elamond 0.7

UPSes and power stations ([sources](config.md#sources)), the long history behind graphs, and
running a station's commands. The API stays 2: all of it is fields a client may ignore, a widget
type a client that doesn't list it never gets, and a request it needn't make.

| new | where |
|---|---|
| `ups` in `service.features` | [`ui`](#get-apiv1ui) |
| `ranges` of `graph` and `timeline`; [`GET /api/v1/history`](#get-apiv1history) | [widgets in `ui`](#widgets-in-ui) |
| the widget type `timeline`, its state `segments` | [`ui`](#widgets-in-ui), [the stream](#get-apiv1stream) |
| the `level` of a source's own value (`eco.status`) in a widget's state | [the stream](#get-apiv1stream) |
| a toggle's `v` read through its `map` | [the stream](#get-apiv1stream) |
| actions of kind `nut`; `confirm_text` in a choice and in the request; 403 `confirm_required` | [station actions](#station-actions-kind-nut) |
| 404 `no_such_widget` | [`/history`](#get-apiv1history), [errors](#errors) |

What an app of 0.6.2 or before sees of a 0.7 service: what it saw before, and the source's
dashboards drawn with the types it knows. A `timeline` reaches it as a `text` (it doesn't list the
type); `ranges`, `ups`, a choice's `confirm_text` and `segments` it ignores; it unlocks for a `nut`
action as for `actions`, as for any kind it doesn't know, and the toggles, sliders and selects of a
station work as any other. A command that needs typing it can't send: the service
answers 403 `confirm_required`, which it shows by its code.

An app of 0.7 with an older service: no `ranges` come, so it offers no long ranges; there is no
`timeline` to draw, nor `confirm_text` to ask for.

The fixtures 0.7 adds are in `elamond/fixtures/0.7/`: `ui-ups-source.toml` (the settings), with the
station they are read with in its comments; `ui-ups.json` (its `ui` for an admin `android` client);
`stream-ups.json` (a `data` line of its dashboard); `history.json` and `history-timeline.json`
(`/history` of its graph and of its timeline); `action-request-typed.json` (a command typed in).
They move up beside the others once the service writes them and the app reads them. Then, with the
service at 0.7.0, these change as well: `hello.json` (`version` 0.7.0, `widgets` 2), `errors.json`
(`no_such_widget` and `confirm_required` added), `ui.json` (its medium graph gains `ranges`, and so
its `version`, which `stream-open.json` and `stream-ui.json` carry), and `ui-default.json` (each
graph but the small ones gains `ranges`; the default profile's `unlock.for` gains `nut`).

### What came with elamond 0.8

Values from commands, web services and files ([sources](config.md#sources)), the systemd journal
in a widget, and row actions that open a dashboard. The API stays 2: all of it is a widget type a
client that doesn't list it never gets, fields a client may ignore, and a request it needn't make.

| new | where |
|---|---|
| `widgets` 3 | [`hello`](#get-apiv1hello-no-token) |
| the widget type `log` (`scope`, `unit`, `level`, `lines`, `search`) and its state | [`ui`](#widgets-in-ui), [the stream](#a-logs-state) |
| `log` in a stream's change | [`POST /api/v1/stream/{id}`](#post-apiv1streamid) |
| [`GET /api/v1/log`](#get-apiv1log) | new |
| row actions with `open` | [`ui`](#row-actions-that-open-a-dashboard) (`table`) |
| sources of kind `command`, `http` and `file` | [config.md](config.md#sources) only: their values reach a client as any others do |

No error code is new: `/log` answers `bad_param`, `no_such_widget` and `failed` as `/history` and
the actions do.

What an app of 0.7 sees of a 0.8 service: what it saw before, and the sources' values in the
widgets it knows. A `log` reaches it as a `text` of level `warning`, `Update the app to read the
journal.` (it doesn't list the type), so Control's *Journal* button opens a page that says so; it
gets no row action with `open`, nor the dashboards only those open (`unit_log.<link>`); `widgets` 3
it ignores.

An app of 0.8 with an older service: there is no `log` to draw nor row action with `open`, and
`/log` answers 404 `not_found`.

The fixtures 0.8 adds are in `elamond/fixtures/0.8/`: `ui-sources.toml` (the settings), with the
sources' reads and the journal they are read with in its comments; `ui-sources.json` (its `ui` for
an admin `android` client); `stream-sources.json` (a `data` line of its `sources` dashboard,
without `history`: the sources' values, and its two logs' first states on the stream);
`stream-log-next.json` (a later line of its `journal` dashboard: new lines and `more`);
`subscribe-log.json` (a stream's change of two logs' filters); `log.json` (`/log` of the
journal's widget); `hello.json` (`widgets` 3). They move up beside the others once the service
writes them and the app reads them. Then, with the service at 0.8.0, these change as well:
`hello.json` (`version` 0.8.0, `widgets` 3) and `ui-default.json` (Control's *Journal* button,
the Links table's row action `{ open, label }`, and the `journal` dashboard).

### What came with elamond 0.9

Events: what the rules of the settings and `elamond notify` tell, kept by the service, sent on by
it to ntfy topics, and shown by a `log`. The API stays 2 and `widgets` 3: a `log` of the service's
events is a `log`, its lines and its state those of a journal's, so an app of 0.8 shows it as it
is. Nothing a client may ask makes an event, a rule or a target: they are the settings' and the
computer's own.

| new | where |
|---|---|
| `scope` `events` of `log` | [widgets in `ui`](#widgets-in-ui), [its state](#a-logs-state), [`GET /api/v1/log`](#get-apiv1log) |

What an older app sees of a 0.9 service: an app of 0.8 or 0.8.1 shows the events as any `log`
(the standard `events` dashboard, behind Control's Events button); an app of 0.7 or before gets
the `log` as a `text` saying to update, as it does a journal's. An app of 0.9 with an older
service finds no such `log` in its `ui`.

The fixtures 0.9 adds are in `elamond/fixtures/0.9/`: `ui-events.toml` (the settings: the rules
elamond starts with, a rule of the settings' own and a target, and a dashboard of the events) with
the readings and what each rule made of them in its comments; `ui-events.json` (its `ui` for an
admin `android` client); `stream-events.json` (the dashboard's first `data` line); `log-events.json`
(`GET /api/v1/log` of it, before its oldest line); `events.jsonl` (the file the events are kept
in); `ntfy.json` (what the target and one with names are sent for each event: the bodies of the
requests to ntfy, the topic in them a placeholder).

### What came with elamond 0.8.1

A timeline's legend in shares of the time. The API stays 2 and `widgets` 3: it is one key a client
may ignore.

| new | where |
|---|---|
| `legend` of `timeline` (`duration`, `percent` or `both`) | [widgets in `ui`](#widgets-in-ui) |

What an app of 0.7 or 0.8 sees of a 0.8.1 service: what it saw before; it ignores `legend` and
words the legend in durations. An app of 0.6.2 or before gets a `timeline` as a `text`, as before.
An app of 0.8.1 with an older service gets no `legend`, and words it in durations as well.

The fixtures 0.8.1 adds are in `elamond/fixtures/0.8.1/`: `ui-legend.toml` (the settings: four
timelines of the 0.7 fixture station, without `legend` and with each of its values), with what an
app shows under each in its comments; `ui-legend.json` (its `ui` for an admin `android` client).
They move up beside the others once the service writes them there and the app reads them.

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
it announces nothing.

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
`default_role` from the settings ([roles](#roles)), and a profile of its own ([profiles](#profiles)).

### Where the computer is: `endpoints`

A phone paired by code knows only the address it reached the computer at, and a link holds what was
true when it was made. So the service tells paired phones where it is now, as `endpoints`: in the
answer to `pair`, in `ui` (`service.endpoints`) and in a stream's `ui` line. They are what `h`
would hold, by the same rules, up to 16: the addresses the service listens on that a phone could
reach, the local network first, then VPNs (of each interface its IPv4 addresses, or its IPv6 ones
when it has none; never loopback or link-local ones, nor, listening on every address, bridges for
containers and virtual machines), then the names in `names`, then the computer's Tailscale name.
They change as the computer's networks do (Tailscale coming up, an interface named in `listen`
getting an address), and with them `ui`'s `version`.

```json
"endpoints": ["192.0.2.10", "100.64.0.7", "desk.example.net", "desk.tail0000.ts.net"]
```

A client adds them to the addresses it tries for that computer (it keeps 16 at most, each taken
only as an address or a DNS name, as `h`'s are), only from an answer that came over the certificate
it pinned for that computer and to its token; the one that worked last stays the first it tries.
None of them makes a computer of another: whatever answers there is accepted only with the paired
id and certificate.

20 wrong codes in all, from wherever they come, use the code up. Attempts are judged one at a
time, and each wrong one is answered after a second; neither that second nor a phone waiting to be
let in (below) holds up anyone else's attempt.

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
{ "code": "123456", "device": "Phone name", "previous": { "client_id": "0123abcd", "token": "…" } }
```

200: `{ "token": "…", "client_id": "…", "id": "<service id>", "name": "<computer name>", "role": "<role>", "profile": "<profile id>", "endpoints": ["…"] }`,
after up to 90 seconds while the person at `elamond pair` decides.
403: `{ "error": "wrong_code" }`, `{ "error": "no_pairing" }` when no code is pending (or a phone
that gave it waits to be let in), `{ "error": "pair_declined" }` when the phone wasn't let in,
`{ "error": "too_many_phones" }` when 64 are paired already; 413 for a body over 4 KiB. `device` is cleaned
(controls and Unicode format characters taken out, 64 characters kept) and is `Phone` when nothing
is left.

`profile` is the phone's own profile, a table the service adds for it to
[phones.toml](config.md#phones) as it pairs, which uses the profile `elamond pair --profile` named
or `default_profile`; where phones.toml can't take one, the profile the phone has instead.
`endpoints` [as above](#where-the-computer-is-endpoints).

`previous`, optional, is for a phone that pairs again with a computer it is paired with already
(after a new pairing link, or to reach it at another address): its entry there and that entry's
token, as proof that it is the same phone. A client sends it only over the certificate it pinned
for that computer, never to one it is still to trust. With the right code, a `previous` whose token
is that entry's makes the new entry take the old one's own profile (its table in phones.toml, and
the profile it had), and unpairs the old entry once the new one is added: it is the same phone,
whose old token the app drops. The person at `elamond pair` is told so in its question (`It
replaces "Galaxy S25" (0123abcd) and keeps its profile galaxy-s25.`). The role is still the one `elamond pair`
gives, never the old entry's; `elamond pair --profile` still decides, when given. A `previous` that
proves nothing (an entry not paired, a token not its own) changes nothing, and the pairing goes
on as without it: no profile is taken and nothing unpaired. Without a `previous`, a profile left by
an unpaired phone of the same name is given to the new one only when the person at `elamond pair`
says so (never with `--yes`, nor with `--profile`).

The token goes in every other request as `Authorization: Bearer <token>`. Without a valid one
the service answers 401 `{ "error": "unauthorized" }`. A phone removed with `elamond revoke` gets
401 from then on, and any stream it has open ends.

The token is all the service knows a phone by: whatever the app asks before using it (unlocking
the phone, say) only the app enforces, so a token copied off a phone works without it. That is
what `elamond revoke` is for, and what a phone that forgets the computer asks itself
([`unpair`](#post-apiv1unpair)). A phone that holds another's token (the app before 0.6.2 carried
one over with Export pairings) is the phone it was given to until it asks for a key of its own
([`rekey`](#post-apiv1rekey)); afterwards each is an entry of its own, the new one *added with the
key of* the old. The two are a family: they share one share of the activity log. `elamond revoke
ID` of the old one leaves the new one paired and names it loudly; `elamond revoke ID --with-added`
unpairs the whole family (by the old one's id even once it is gone), and `elamond clients role ID
ROLE --with-added` changes all their roles. `elamond backup` and `elamond restore` carry the
service's side: its id, certificate and key, and the hashes of the tokens, so a restored service
accepts the tokens it gave out before.

### `POST /api/v1/rekey`

A key of its own for a phone that holds another's token, as one did after Import pairings in the
app before 0.6.2.

```json
{ "device": "Phone name" }
```

200, as pairing answers: `{ "token": "…", "client_id": "…", "id": "<service id>", "name": "<computer name>", "role": "<role>", "profile": "<profile id>", "endpoints": ["…"] }`.
It is a new entry among the paired phones, named `device`, with a new token, the role of the entry
whose token asked and a profile of its own in phones.toml that uses that entry's. That token stays good: the phone it came from keeps working until
`elamond revoke` of its own entry, which leaves the new one paired. `elamond clients` shows the
new entry as added with the key of the one that asked, and `elamond revoke` of that one names the
entries added with its key.

Any role may ask, and none is raised (a `role` in the body is ignored). A token gives one key of
its own: only an entry paired with a code may ask, and once; the entry that asked again, or the
new one, gets 403 `rekey_refused` (so a token copied to several phones gives one of them a key; the
others pair). 403 `too_many_phones` when 64 phones are paired. It needs no control, but is recorded
in the [activity log](#activity-log) as `phone added: <device>` under the phone that asked, with the
new entry's id and role in `message`. 500 `storage` when the list of phones can't be written, or no
longer parses (it is never written over); 413 for a body over 4 KiB.

### `POST /api/v1/unpair`

A phone that forgets the computer unpairs itself: the entry whose token asks is unpaired, as
`elamond revoke` of it alone would, and nothing else: never the entries added with its key, which
stay paired. Any role may ask, and it needs no control (it works with `control = false` and after
`elamond control off`). No body is read.

200 `{ "ok": true }`: from then on its token gets 401 and any stream it has open ends. Its own
table in [phones.toml](config.md#phones) stays, whatever is in it, with a note above it
(`# Unpaired <date>; kept for a phone paired again (elamond pair offers it to one named "<name>").`),
so that `elamond pair` offers it to the phone when it pairs again. The service's journal says
`pairing: "<name>" (<id>) unpaired itself`, and the [activity log](#activity-log) `unpaired itself`.
500 `storage` when the list of phones can't be written,
or no longer parses (it is never written over).

A service before 0.6.2 answers 404 `not_found`; a client then forgets the computer all the same and
says to run `elamond revoke` there, as it does when it can't reach the computer.

## A new certificate

`elamond rotate-cert` gives the service a new key and certificate, deleting the old key; its id
stays the same. The service presents the new certificate within seconds (from the next connection;
those open keep theirs). Nothing leads a phone from the certificate it pinned to the new one: every
phone sees the certificate changed, and is unpaired (`elamond revoke`) and paired again. Services
before 0.6.2 also signed a new certificate over with the old one, served at
`GET /api/v1/identity/rotations`; that route is gone (404 `not_found`), and an app that asks it
reports the certificate changed.

## Roles

Each paired client has a role. They are ordered, each allowing what the one before does and more:

| role | sees | may run |
|---|---|---|
| `glance` | the basics: CPU, memory, GPUs, temperatures, fans, disks, disk I/O, network, battery, power, the system, UPSes | nothing |
| `monitor` | everything there is to read: also processes with their commands and users, monitors, links, media with their covers, the activity log, the journal, the events (unless the settings give them to another role), a UPS's driver, serial numbers and versions | nothing |
| `operate` | | actions of kind `profile`, `units`, `media` and `actions`, and `nut` of a source whose `role` is `operate` |
| `admin` | | `power`, `signal` and `nut` too |

Any role may ask for a key of its own ([`rekey`](#post-apiv1rekey)), which keeps that role.
The values of a source of kind `command`, `http` or `file` are seen from that source's `role` up,
`monitor` unless its settings say ([config.md](config.md#sources)).

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
Pairing gives each phone one of its own in [phones.toml](config.md#phones), using the profile
`elamond pair --profile <id>` names, otherwise `default_profile` from the settings; a change to it
there reaches the phone as any change of the settings does (a stream it has open gets `ui` at once).
`elamond clients profile <id|all> <profile>` gives it another, from the client's next request. A
client whose profile the settings no longer have gets `default_profile`.

## `GET /api/v1/ui`

What the client shows: its profile, its tabs, the dashboards and the actions, all as far as its role
goes. Query: `widgets`, the widget types the client draws, comma-separated
(`widgets=value,gauge,meter,status,graph,list,table,button,toggle,select,slider,text,group,media,timeline,log`).

```json
{
  "version": "5c1f0e2a9d3b7a41",
  "service": { "id": "…", "name": "…", "version": "0.8.1", "interval_ms": 1000, "history_seconds": 600,
               "features": ["control", "media", "power", "power_profiles", "processes", "units", "ups"],
               "endpoints": ["192.0.2.10", "100.64.0.7", "desk.example.net"] },
  "role": "admin",
  "control": { "enabled": true, "off_reason": null },
  "profile": { "id": "galaxy-s25", "name": "Galaxy S25", "theme": "dark", "pure_black": true, "animations": true,
               "interval_ms": 1000, "fahrenheit": false, "network_bits": false, "confirm_actions": true,
               "unlock": { "for": ["power", "signal", "actions", "units"], "grace": "every_time" },
               "android": { "dynamic_color": true, "blur": true, "haptics": true, "keep_screen_on": false },
               "source": { "file": "elamond.d/phones.toml", "line": 4 } },
  "tabs": ["overview", "processes", "sensors", "control"],
  "header": "header",
  "dashboards": [{ "id": "overview", "title": "Overview", "icon": "overview", "widgets": [ … ] }, …],
  "actions": [{ "id": "power.reboot", "name": "Restart", "kind": "power", "icon": "restart",
                "confirm": true, "unlock": true, "params": [] }, …]
}
```

- `version` changes whenever anything else in the document would: the settings, the client's role
  or profile, control turned on or off, the endpoints, a GPU, disk, sensor or interface that comes or goes (a
  label or title that holds values follows them within 5 seconds). A
  client keeps the last document of each computer (a cache, not settings) to start at once and to
  show while offline, and fetches it again when a stream says `ui` with another version.
- `service.features` says what this computer can offer at all (whatever the client's role),
  sorted: `control` (`control = true` in the settings), `power` (logind can power off, restart,
  sleep or lock), `power_profiles` (power-profiles-daemon or tuned-ppd), `units` (systemd), `media`
  (a session bus for MPRIS), `processes`, `displays` (a GPU whose monitors DRM tells of), `nvml` (an
  NVIDIA GPU NVML can read), `ups` (the settings have a `nut` [source](config.md#sources), whether
  upsd answers or not). A client ignores those it doesn't know.
- `service.endpoints`: where the computer is now ([pairing](#where-the-computer-is-endpoints)).
- `profile` holds the part every client reads, and the part for this client's kind (`android`)
  only. For an `admin`, `source` says where the profile's table is, for the person to change it:
  the file that set it last (relative to the settings' folder, `~/.config/elamon`, when it is in
  it; `elamond.toml`, `elamond.d/phones.toml`) and the line of its `[profiles.<id>]` there; for
  other roles it is left out.
- `tabs` are the profile's dashboards the role sees, in order (one for each item of a dashboard
  with `for_each`); `dashboards` the tabs, then the header, then the dashboards their widgets
  open, and those theirs open, then those the tables' row actions open
  ([below](#row-actions-that-open-a-dashboard)), each once and as far as the role sees. A
  dashboard the role sees none of the widgets of isn't there, nor an `open` to a dashboard that
  isn't.
- `header` is the profile's `header` dashboard, whose widgets a client draws as small chips under
  the computer's name on every tab; `null` when the profile has none or the role sees none of it.
- `actions` are those the role may run and the computer can do. `confirm` and `unlock` are final:
  the action's own, the profile's `confirm_actions` and `unlock.for` already applied. A client asks
  before running an action with `confirm`, unlocks the phone first for one with `unlock` (within the
  profile's `unlock.grace`, an unlock for a kind covers the kinds after it in
  `power`, `nut`, `signal`, `actions`, `units`, `profile`, `media`), and asks for the values of
  `params` a widget doesn't fix. A choice with `confirm_text` is asked to be typed in
  ([station actions](#station-actions-kind-nut)). The built-in actions come first, in their order,
  then the [station actions](#station-actions-kind-nut) by source and the command one before the
  settings by variable, then the settings' own by id.
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
| `graph` | `series: [{ label, color, unit, format }]`, `range` (s), `height`, `min`, `max`, `floor`, `ranges` |
| `timeline` | `range` (s), `ranges`, `legend` (`duration`, `percent` or `both`; only when the settings give it) |
| `list` | `items: [{ label, unit, format }]` |
| `table` | `source`, `columns: [{ field, label, format, unit, bar: { max }, grow }]` (`max` a number, a field or `"top"`), `sort: { field, desc }`, `sorts`, `sort_labels` (field → the label of its chip; a field without one, its column's label or the field), `search` (bool), `search_fields` (the fields a client searches, when `search`: rows carry them), `hide_empty`, `row_open`, `row_actions: [{ action, label, params } or { open, label }]`, `expand: [{ field, label, format, unit }]` |
| `button` | `action`, `params`, `open` |
| `toggle` | `action`, `param`, `on`, `off` |
| `select`, `slider` | `action`, `param` |
| `text` | `text` (when it holds no values; otherwise it comes in the state), `level` |
| `group` | `widgets`, `collapsed` |
| every type | `caption` (`true` when it holds values: its text comes in the state), `collapsed` (`table`) |
| `media` | |
| `log` | `scope` (`system`, `user` or `unit`), `unit` (with `scope` `unit`), `level`, `lines`, `search` (bool) |

`ranges` is `["1h", "24h", "7d", "30d", "1y", "all"]`, on a `graph` (not a `small` one) and on a
`timeline`, unless the widget's settings say `ranges = false`: the long ranges
[`/history`](#get-apiv1history) answers for it. A client offers them beside the widget's own
`range`, which the stream fills as before: say chips `10 min` (its own), `1 h`, `24 h`, `7 days`,
`30 days`, `Year`, `All`, leaving out one as long as its own. On a long one it fetches `/history`,
draws its points over that window, adds a stream's point only once `step_ms` has passed since the
newest it has, and fetches again when it comes back to the screen. Which one is chosen is the
client's to keep, not the service's.

A `timeline`'s `legend` says what its legend tells of each state over the range shown: `duration`
(how long it lasted, as without the key), `percent` (its share of the time something was known,
on line and on battery: `On battery · 4%`), or `both` (`On battery · 4% · 12 min`). The client
works the shares out from the segments it draws, so they follow the range and the stream; *No
data* has none and is told by its duration, and where nothing was known at all there are no shares,
only durations. A share above nothing but under 1% is `<1%`; the others are whole, each rounded
on its own, so they needn't add up to 100.

A `toggle`'s `on` and `off` are values its action's `param` takes: one the action doesn't take (a
station's command it doesn't offer, or `commands` doesn't allow) leaves the toggle out.

A `log` shows lines of the systemd journal ([config.md](config.md#the-journal-log)): `scope` is
`system` (as far as the service's user may read it), `user` (that user's own), `unit` (the
journal of `unit`, a link's unit) or, from 0.9, `events` (no journal: the service's own
[events](config.md#events-and-notifications)); `level` the least level it shows at first, of `debug`, `info`,
`notice`, `warning`, `error` and `critical`; `lines` how many a client keeps on screen (10–1000);
`search` whether it offers a search field. Its lines come in the stream ([its
state](#a-logs-state)), as the client filters them ([`log`](#post-apiv1streamid)), and older ones
from [`/log`](#get-apiv1log). Its role is `monitor` at least, and with `scope` `events` the
role the settings give the events (`[events]`'s `role`).

### Row actions that open a dashboard

A table's row action is `{ action, label, params }` or, from widgets 3, `{ open, label }`: `open`
names a dashboard made `for_each` over the table's `source`, and the action on a row opens
`<open>.<the row's key>` (`unit_log` on the row of the link `example`: `unit_log.example`). A
client shows it only on the rows whose dashboard is in its `ui`, where each of those dashboards
is. A client that doesn't list `log` gets no row action with `open` (an app before 0.8 reads
`action` as required), nor the dashboards only those open.

### What a client can't draw

A widget type the client didn't list in `widgets` is replaced by the nearest it did: `gauge` and
`meter` (and `status`) by `value`, `select` by buttons in a `group` (each button's id
`<id>/<value>`), `slider` by `select` or those buttons (11 of its values at most), a `group` by
the widgets in it, a `timeline` by a `text` that holds its source's status and how long it has
been so (`"{eco.status} · {eco.status.since:duration}"`, `On battery · 12 min`), its state's
`level` the status's… and where there's none, by a `text` with `level` `warning` saying the app
needs updating to show it (a `log`: `Update the app to read the journal.`). An unknown key on a
known type, a client ignores.

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
   after the dashboard changed, also has `history`: the graphs' points so far. It comes at once,
   from the reading there is (with the next reading for a client that opened another stream within
   half the interval): a widget whose source isn't read yet (the processes, monitors, media
   players, the volume, NVIDIA GPUs or the links, read only while a client shows them) has no state
   in a line until it is, nor a `log` until the service reads its journal; a client shows it
   loading. The first reading that has them sends a line at once.
3. `ui`, whenever the client's `ui` changes: `{ "type": "ui", "ui": "<version>", "endpoints": [ … ] }`,
   `endpoints` as they are now ([pairing](#where-the-computer-is-endpoints)). The stream goes
   on, with the dashboard it had (or the first tab, when that one is gone for this client).

A widget's state:

| type | state |
|---|---|
| `value`, `gauge`, `meter`, `status` | `{ "v": 42.5, "text": "On battery", "level": "warning", "blend": 0.4, "max": 16000000000, "caption": "4.1 GiB of 16 GiB" }`: `v` the value (number, string, bool or `null` when missing) in the widget's unit; `text` when `map` gave it a word, or the value is missing and the widget has `empty` (filled in); `level` and `blend` ([levels](config.md#levels)), or, for a widget without `levels` and limits, the level a value has of its own (a source's `eco.status`, `On battery`: `warning`); `max` when it is an address; `caption` filled in |
| `graph` | `{ "v": [1.5, 0.2], "max": 16000000000, "text": "Asleep" }`, the newest point of each series, at the line's `t`; `max` when it is an address; `text`, the widget's `empty` while the first series has no value |
| `bars` | `{ "v": [12.5, 80.0, …], "labels": ["0", "1", …], "levels": ["normal", "critical", …] }`; `max` when it is an address |
| `list` | `{ "items": [{ "v": …, "text": …, "level": … }] }`, in the order of `items` |
| `table` | `{ "rows": [{ "key": "4242", "level": "normal", "f": { "pid": 4242, "name": "firefox", … } }], "total": 312 }`: at most `limit` rows in the table's order, each with the fields its columns, `expand`, `row_open` and `row_actions` need; `total`, how many there are in all |
| `button` | `{ "enabled": true }` |
| any, with a `caption` that holds values | `"caption": "…"` beside the rest |
| `toggle`, `select`, `slider` | `{ "v": "server", "enabled": true }`; a toggle's `v` read through its `map` when it has one (`on` → `outlet.1.load.on`), a value the map hasn't as it is |
| `text` | `{ "text": "…", "level": "normal" }` when it holds values |
| `group` | none |
| `media` | `{ "players": [ … ], "volume": { "level": 40.0, "muted": false } }` (%) |
| `timeline` | `{ "segments": [{ "from": 1699992000000, "to": 1699999280000, "state": "line" }, …] }` (below) |
| `log` | `{ "lines": [{ "c": 1699999995234567, "t": 1699999995234, "level": "warning", "unit": "example.service", "id": "example", "text": "…" }], "reset": true, "more": 12, "note": "…" }` ([below](#a-logs-state)) |

A timeline's `segments` cover its `range` back from the line's `t` (Unix ms), oldest first, each
from where the one before ends: `line` (on line, or on bypass), `battery`, or `none` where nothing
is known (before the source was first read, while elamond didn't run, while upsd couldn't be
reached). Segments shorter than a 400th of the range that come one after another are made one,
`battery` if any of them was, else `none` if any was, else `line`; one short segment alone stays
as it is (but past 100 segments, a short `none` alone, upsd out of reach for a moment again and
again, is part of the segment before it), and a client draws every segment a pixel or two wide at
least. Neighbours of one state are one segment.

Numbers in states come rounded to one decimal (`blend` to three); a client formats them by the
widget's `format` and `unit` ([config.md](config.md#formats)).

`history`: `{ "<widget id>": { "t": [ … ], "v": [[ … ], [ … ]] } }` for each `graph` and each
`value` with `spark`, oldest first, one array per series, `null` where a value was missing,
reaching the graph's `range` back (a spark's: all the service keeps; just after the service
started, what it had before comes from the [long history](#get-apiv1history), a point every
10 seconds). A timeline's segments come in every line instead. A client appends each `data`
line's `v` at its `t` and drops what falls out of the range. Levels are `normal`,
`warning` and `critical`; `blend` (0–1) is how far it has turned toward the next and is left out at 0.

A media player: `{ id, name, status (Playing|Paused|Stopped), title?, artist?, album?, length? (s),
position? (s), volume? (%), has_art, can_next, can_previous, can_play, can_pause, can_seek }`; `volume`
of the computer is left out where it can't be read.

The service reads costlier sources (NVML, DRM, the process list, MPRIS) only while some client's
dashboard on screen needs them, and lets a powered-down GPU sleep. A table of processes fills a tick
after it is first asked for: their CPU and disk use are rates, which need two readings.

### A log's state

A `log`'s state is the stream's own, not the reading's: what this stream sent of it, and how its
client filters it.

```json
{ "lines": [{ "c": 1699999995234567, "t": 1699999995234, "level": "warning", "unit": "example.service", "id": "example",
              "text": "Slow request: GET /status took 2.1 s" }],
  "reset": true, "more": 12, "note": "Only this user's journal: add the user to systemd-journal to read the system's." }
```

- `lines`, oldest first. Each has `c`, its time in the journal (Unix µs), which a client keeps only
  to send as [`/log`](#get-apiv1log)'s `before`; `t`, the same in Unix ms; `level`; `unit`, the
  systemd unit it came from, and `id`, the program's name (`SYSLOG_IDENTIFIER`, else the command),
  each left out where the journal hasn't one; `text`, the message, 2000 characters at most (cut
  with `…`), terminal colours and control characters taken out but line breaks, a tab a space.
- `reset`, only when `true`: the client replaces what it has with these lines, the newest that
  match, as many as the widget's `lines` and 64 KiB at most. It comes in the widget's first state
  on a stream, after its filter changed, and after the service started reading the journal anew.
- Otherwise, `lines` are those that came since the last this stream sent, maybe none (`[]`): 50
  and 16 KiB a `data` line at most.
- `more`, only when above 0: lines the stream left out before these, those that matched past 50
  or 16 KiB (the oldest are left out), and those that left the service's buffer, the journal's last
  1000 lines, before the stream sent them, matching or not. A client shows `… 12 more` above these.
- `note`, only when there is one: why there may be more than these lines, or none. A `system` log,
  or one of a system unit, while the service's user may not read the system journal (`Only this
  user's journal: add the user to systemd-journal to read the system's.`); no `journalctl` on the
  computer; a `journalctl` that ended, with the first line of its error (the service starts it
  again every 10 seconds at most while a client shows the widget). A client shows it above the
  lines.

A `log` of `scope` `events` has the same state, of events instead of a journal's lines: `id` is
the id of the rule that made the event (`notify`, for `elamond notify`'s), `unit` is left out,
`level` is `info`, `warning` or `critical`, `text` the event with names, `c` its time in µs (made
unlike any other's), and there is no `note`. Its lines are there from the first state: the
service keeps the events itself, the last 1000 of them at hand for the stream.

A line matches when its level is the filter's or worse, and, with a search, when its `text`,
`unit` or `id` holds the search, whatever the case. Until the client asks for another
([below](#post-apiv1streamid)), the filter is the widget's `level` and no search. The service reads
the journal only while some client's stream shows a `log` of it, and until it does, the `log` has
no state: a client shows it loading.

### `POST /api/v1/stream/{id}`

Changes what an open stream of this client sends, from its next line, without reconnecting:

```json
{ "dashboard": "cpu", "interval": 2000, "sort": { "processes/table": "mem" } }
```

Each key is optional. `sort` asks a table, by its widget id, for another of its `sorts`
(descending); `null` in place of a field (`{ "processes/table": null }`) returns the table to its own
`sort`, field and direction.

`log` (from elamond 0.8) asks a `log`, by its widget id, for another filter:

```json
{ "log": { "sources/app_log": { "level": "error", "search": "timeout" }, "sources/journal": null } }
```

A filter is the whole of it: `level` one of the six, the widget's own when left out; `search` 100
characters at most, none when left out or empty. `null` in place of a filter returns the widget to
its own `level` and no search; a widget left out keeps the filter it has. Filters are kept by
widget id, as orders are, and a new one makes the widget's next state a `reset`.

A new dashboard, order or filter comes at once, from the reading there is, not after an interval;
a new dashboard's first line has `history`. At once no more than every half interval: a change
soon after another comes with the next reading. Changes the stream hasn't taken up yet (its client
doesn't read it) become one, each key the last asked. 200 `{ "ok": true }`; 404 `no_such_stream`
when the stream has ended or isn't this client's; 404 `no_such_dashboard` for one not in the
client's `ui`; 400 `bad_sort` with the table's id in `message`; 400 `bad_param`, with `log` in
`message`, for a `log` key that names no `log` of the stream's dashboard (as this request leaves
it) or header, a level not of the six, or a search over 100 characters. A refused request (400
`bad_sort` or `bad_param`, 404 `no_such_dashboard`) applies none of its keys: the stream goes on as
it was. A body over 4 KiB is a plain-text 413.

## `GET /api/v1/snapshot`

One `data` object (without `type`, with `history` unless `history=0`) for `dashboard=<id>` (the
first tab without it), after waiting a few ticks for a reading that has everything it needs (a
widget whose source still isn't read then has no state, as in a stream; a `log` has none there
at all: its lines come from [`/log`](#get-apiv1log)); 404 `no_such_dashboard` for one not in the
client's `ui`. `widgets` as for the stream.

## `GET /api/v1/history`

The long history of one graph or timeline, over one of the long ranges. The service keeps every
value graphs and sparks show, and every number of a UPS source, at three steps
([config.md](config.md#the-long-history)): every 10 seconds for a day, every 5 minutes for 30 days,
every hour from then on. Query:

| name | |
|---|---|
| `widget` | the id of a `graph` or a `timeline` in the client's `ui`: the one it has now, for its role, profile and the widget types it last said it draws |
| `range` | `1h`, `24h`, `7d`, `30d`, `1y` or `all` |

A graph's:

```json
{ "widget": "ups.eco/watts", "range": "24h", "from": 1700000040000, "to": 1700086440000, "step_ms": 120000,
  "t": [1700000040000, 1700000160000, …], "v": [[206.0, null, …], [210.0, 212.5, …]] }
```

| `range` | from the points of every | `step_ms` | buckets |
|---|---|---|---|
| `1h` | 10 s | 10 s | 360 |
| `24h` | 10 s | 2 min | 720 |
| `7d` | 5 min | 15 min | 672 |
| `30d` | 5 min | 1 h | 720 |
| `1y` | 1 h | 12 h | 730 |
| `all` | 1 h | the fewest whole hours, 1 at least, that keep the buckets to 800 | from the oldest point on |

`to` is now rounded down to a whole step (from the Unix epoch), `from` that many buckets before it
(for `all`, the bucket of the oldest point of any of its series). Each bucket runs from its `t`
(Unix ms, its start) for a step, the last ending at `to`; `v` has an array for each of the graph's
`series`, in their order, of each bucket's mean of the points in it, `null` for a bucket without
one: in the client's units, to one decimal. With no point at all, `all` has no buckets and
`from` is `to`. The points before `to` are all there is of that range: a client draws from `from`
to now, and adds a stream's point to them only once `step_ms` has passed since the newest it has.

A timeline's:

```json
{ "widget": "ups.eco/power", "range": "7d", "from": 1699395200123, "to": 1700000000123,
  "segments": [{ "from": 1699395200123, "to": 1699977600000, "state": "none" }, …] }
```

`to` is now (Unix ms), `from` the range before it (for `all`, the first change the service has
kept); `segments` cover that, by the rules of a timeline's state in the [stream](#get-apiv1stream).

Any of the six ranges is answered, whatever `ranges` lists. 404 `no_such_widget` for an id that
isn't a `graph` or a `timeline` in the client's `ui`: as everywhere, a client gets the history only
of what its role sees. 400 `bad_param`, with `widget` or `range` in `message`, for one missing or
not one of those; 500 `storage` when the history can't be read at all (a file that can't be read
is taken as empty). A service before 0.7 answers 404 `not_found`; its `ui` has no `ranges` either.

## `GET /api/v1/log`

Older lines of a `log`, for a client scrolled back past those it has. Query:

| name | |
|---|---|
| `widget` | the id of a `log` in the client's `ui` |
| `before` | Unix µs, optional: lines older than it, the `c` of the oldest line the client has; without it, the newest |
| `level` | optional: the least level, as in a [filter](#post-apiv1streamid); the widget's own without it |
| `search` | optional, 100 characters at most |

```json
{ "widget": "journal/system", "lines": [{ "c": 1699999801000000, "t": 1699999801000, "level": "warning", … }, …],
  "before": 1699999612345678 }
```

The service looks back through the journal from `before`, at the lines of the level or worse,
until 100 of them match the search or it has looked at 2000, and answers those that match, oldest
first, as a state's `lines` are. `before` is where to go on from: the `c` of the oldest line it
looked at, for the client to send as `before` for the lines before those; `null` once it reached
the start of the journal. A page may have fewer than 100 lines, or none, and still a `before`. It
runs `journalctl`, for 5 seconds at most, two such requests at a time (others wait their turn).

404 `no_such_widget` for an id that isn't a `log` in the client's `ui`; 400 `bad_param`, with
`widget`, `before`, `level` or `search` in `message`, for one missing where it is needed or not as
above. For a `log` of `scope` `events` the service answers from the events it keeps, at once,
100 lines a page. 502 `failed` when `journalctl` can't be run (its error in `message`) or takes longer
(`timed_out`). A service before 0.8 answers 404 `not_found`.

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
ended; for a [station's](#station-actions-kind-nut), `{ "ok": true, "output": "done" }`. 404
`no_such_action` for an id the computer doesn't have or can't do now; 403 `not_allowed`
for one it can that the client's role may not run (so not in its `ui`), `control_disabled`,
`not_permitted`, `confirm_required`; 409 `process_changed`; 502 `failed` with a `message`.
They are checked in this order: the role, control, then the parameters. The body is
`{ "params": … }`, and `confirm_text` for a command that is typed in; a client leaves
`confirm_text` out otherwise, and a service before 0.7 ignores it.

`process.signal` with `start` signals only the process that started then: if the pid now belongs to
another, 409 `process_changed`; a negative `start` is 400 `bad_param`. `unit.*` act on the unit of the link given, only one the settings
name; system units also need polkit's leave ([the guide](guide.md#power-the-profile-and-system-units-polkit)).

### Station actions (kind `nut`)

A UPS [source](config.md#sources) whose settings list `commands` or `settings` adds actions of
kind `nut`, ids of the service's own, once upsd has told what the station offers (they stay
offered after, while upsd can't be reached); until then the widgets that run them are left out:

| id | parameter | |
|---|---|---|
| `nut.command.<source>` | `cmd`: a choice of the station's commands that `commands` allows, in the station's order | each labelled by upsd's description of it, else, for `outlet.N.load.on` and `.off`, the outlet's `desc` and `on` or `off`, else the command; named as the station is (`device.model`, else `ups.model`, else the source's id) |
| `nut.set.<source>.<variable>` | `value`: a `choice` of its values (labelled as words: `enabled` → `Enabled`), or a `range`, its one range on a step of 1, in the variable's unit | one for each writable variable of the station that `settings` allows and that takes a list of values or one range (others, and one of several ranges, aren't offered; the journal says so once); named by upsd's description of it, else the variable |

Their least role is the source's `role` (`admin` unless it says `operate`); each asks before it
runs (`confirm`), and unlocks as the profile's `unlock.for` says for the kind `nut`.

A command that cuts what powers the computer, or a whole station's load, is typed in: its choice
carries `confirm_text`, the command itself, which the client asks the person to type (exactly,
whatever the profile's `confirm_actions`), and sends beside the parameters:

```json
{ "params": { "cmd": "outlet.1.load.off" }, "confirm_text": "outlet.1.load.off" }
```

Those are the commands on an outlet the settings call `critical` but the one that turns it on
(`outlet.1.load.off`), and the commands upst-core counts as dangerous that aren't about one
outlet (`load.off`, `shutdown.*`, `calibrate.start`…); turning another outlet off is asked as any
action is. Without the text, or with another, the service refuses: 403 `confirm_required`, with the text
in `message`. A `confirm_text` where none is needed is ignored.

They are checked in this order: an id the computer doesn't have, 404 `no_such_action`; the role,
403 `not_allowed`; control (`control = false`, `elamond control off`), 403 `control_disabled`; a
`cmd` that isn't among the choices, 403 `not_allowed` with `cmd` in `message`; a parameter missing
or of the wrong kind, or a `value` off its choices, range or step, 400 `bad_param`; the typed
text, 403 `confirm_required`; the source's password file missing, unreadable or open to others,
502 `failed` (its `message` says which, never what is in it). Then the
action waits for any other on the same source to end (one at a time on a source), and the service
gives it to upsd and waits up to 25 seconds for the station to answer:

| the station | answer |
|---|---|
| did it | 200 `{ "ok": true, "output": "done" }` |
| refused it, or upsd did | 502 `failed`, `message` its words (`invalid value`, `command not supported`, `access denied (check the user's permissions in upsd.users)`) |
| still at it after 15 seconds, or no answer in 25 | 502 `failed`, `message` `timed_out` |
| upsd took it but can't say whether the station did it (a upsd without tracking) | 502 `failed`, `message` `accepted by upsd but not confirmed by the station` |

The password is never in an answer, a state, the activity log or the journal.

## `GET /api/v1/media/art?player=`

The cover (PNG, JPEG or WebP, 4 MiB at most), when the player has it in a local file; for roles that
see media. 404 `no_art`.

### Connections

A connection has 10 seconds for its TLS handshake, then 15 for its first request,
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
`phone added: Pixel 9`, `nut eco outlet.2.load.off` (a station's command: the source and the
command), `nut eco experimental.battery.charge.max 90` (a setting: the source, the variable and
its value); a station's `message` is `done` or why it failed. Requests that were let
through are also in the journal (`journalctl --user -u elamond`).

## Errors

Errors are `{ "error": "<code>" }`, some with a `message`, with these statuses (the same list is in
[`elamond/fixtures/errors.json`](../elamond/fixtures/errors.json)):

| status | codes |
|---|---|
| 400 | `bad_param` (with `message`, the parameter), `bad_sort`, `unknown_signal` |
| 401 | `unauthorized` |
| 403 | `no_pairing`, `wrong_code`, `pair_declined`, `too_many_phones` (64 are paired), `rekey_refused`, `control_disabled`, `not_permitted` (polkit refused, or a protected process), `not_allowed` (the client's role doesn't allow it), `confirm_required` (a station's command that is typed in, without its text; the text in `message`) |
| 404 | `not_found`, `no_such_process`, `no_such_action`, `no_such_dashboard`, `no_such_widget` (no such graph, timeline or log in the client's `ui`), `no_such_stream`, `no_art` |
| 409 | `process_changed` |
| 500 | `storage` |
| 502 | `failed`, with `message` |

`failed` carries a `message`: either one of `unknown_player`, `no_value` (`volume.set` with
neither `level` nor `muted`), `no_track`, `no_session`, `no_system_bus`, `no_session_bus`,
`no_power_profiles`, `volume_failed`, `mute_failed`, `timed_out`, or the system's own error text
(D-Bus, a command, `journalctl`, upsd or the station; for a station's command upsd took without
saying the station did it, `accepted by upsd but not confirmed by the station`). A request whose JSON
or query the service can't read at all gets a plain-text 400, 415 or 422, and a body over its limit
(4 KiB for `pair`, `rekey`, `actions` and `stream`) a plain-text 413.

In [`errors.json`](../elamond/fixtures/errors.json) the codes come in this table's order:
`confirm_required` after `not_allowed`, `no_such_widget` after `no_such_dashboard`.
