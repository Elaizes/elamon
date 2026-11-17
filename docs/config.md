# Elamon settings reference (settings schema 3)

Everything the app and the web page show, and everything a phone may run, is set on the computer,
in elamond's settings: the dashboards and their widgets, the actions, how each phone looks and
behaves (its *profile*). The app keeps nothing of it; it draws what the service describes. These
settings are TOML on every distribution; on NixOS the module's `settings` mirror them one to one.

The files are all there is. The service reads these, in this order, a later one adding to and
overriding an earlier one, and nothing under them:

1. `~/.config/elamon/elamond.toml` (or the file `ELAMON_CONFIG` names): the whole of the settings,
   as `elamond setup` or the service's first start writes it ([the starter file](#the-starter-file))
   and you change it;
2. `~/.config/elamon/elamond.d/*.toml`, by name: drop-ins, among them [`phones.toml`](#phones),
   where each paired phone has a profile of its own.

What isn't in them the service doesn't have: a dashboard taken out of elamond.toml is gone, not
put back from somewhere. Only the built-in actions (`power.*`, `profile.set`, `unit.*`…) come
with elamond itself, being what it can do rather than settings.

A table with an id (`[dashboards.power]`, `[actions.mode]`, `[links.jellyfin]`,
`[profiles.owner]`, `[templates.history]`, `[sources.eco]`) merges with the one of the same id
before it, key by key, as `[history]` does; a list (`widgets`, `listen`, `dashboards` in a profile,
`commands` or `command` of a source) replaces the one before it whole.
So a drop-in can add a dashboard, change one key of one in elamond.toml, or replace its widgets.

`elamond check [FILE]` reads the settings as the service would and names the file and line of each
mistake; `elamond edit [FILE]` opens the file in `$VISUAL` or `$EDITOR` and doesn't keep a broken
one; `elamond schema` prints the JSON Schema of these settings, for editors that check TOML against
one (taplo, Even Better TOML: `#:schema` at the top of the file, as the starter files have it);
`elamond defaults` prints the
settings elamond starts with ([below](#the-defaults-beside-defaultstoml)); `elamond values` lists
the values this computer has now, and `elamond values <source>` those of a [source](#sources),
read there and then.

## The starter file

Where there is no elamond.toml, `elamond setup` and the service's first start write the settings
elamond starts with, in two files:

- `elamond.toml`, short: `schema = 3` and the top-level keys (control, where it listens, names,
  the role and profile new phones get), each with a line on what it does, commented-out examples of
  a link and an action, and the `default` profile;
- `elamond.d/dashboards.toml` (unless one is there already): every standard dashboard (`header`,
  `overview`, `ups`, `processes`, `sensors`, `control`, and the pages their tiles open) and the
  templates they use.

Together they are the whole of the settings, as one file would be: nothing is built in under
them. Both are yours from then on: elamond never writes them again. Each begins with a line for
editors, `#:schema elamond-schema.json` (`../elamond-schema.json` in elamond.d/), naming the JSON
Schema of the settings that the service keeps beside them, as it keeps defaults.toml; editors that
read it (taplo, Even Better TOML) then suggest keys and mark mistakes.

```toml
#:schema elamond-schema.json
…
schema = 3

# Let paired phones act on this computer (power, the power profile, units, media, the actions
# below), each as far as its role allows.
control = false
…
[profiles.default]
name = "Default"
theme = "system"
…
```

dashboards.toml is read after elamond.toml and before the drop-ins whose names come after its own,
phones.toml among them; a drop-in that changes a standard dashboard is named so it comes after it
(`elamond.d/mine.toml`), or the change is made in dashboards.toml itself. An elamond.toml of 0.6.1,
the whole of the settings in one file, stays as it is and as good: the two files are only how a new
start is laid out.

An empty elamond.toml is sound settings too, of no dashboards and no profiles: phones paired with
it see nothing. `elamond check` says so: it warns of settings without dashboards, and of a
`default_profile` that isn't among the profiles. Where there is no elamond.toml at all, the service
writes the starter files at its next start ([above](#the-starter-file)); until then the drop-ins are
all there is.

`schema` says which settings these are. `3` is a whole file, as this page describes; another number
is a mistake (a file written for a newer elamond). A drop-in may say
`schema = 3` too, and nothing else.

### The defaults beside: defaults.toml

The service keeps `~/.config/elamon/defaults.toml` beside your settings: the settings each version
of elamond starts with, read-only, for you to compare and copy from. It isn't read as settings
(only elamond.toml and elamond.d/ are). Its first line names the version that wrote it; when another
version starts, it writes its own and keeps the one before as `defaults.toml.old`.

| command | |
|---|---|
| `elamond defaults` | prints the settings elamond starts with: the two starter files as one, from `schema` on |
| `elamond defaults --diff` | what in your settings is other than in those, and what changed in them between defaults.toml.old and this version: by table, `added`, `changed` (and which keys) or `removed` |
| `elamond defaults TABLE` | one table of them, as TOML to copy into your settings: `dashboards.sensors`, `profiles.default`, `templates.sensor_table` |

```
$ elamond defaults --diff
Your settings beside the defaults of elamond 0.6.1:
  changed  (top level)          control, listen
  changed  profiles.default     theme
  added    profiles.owner
  added    dashboards.desk
  added    actions.mode
  removed  dashboards.battery

What changed in the defaults since elamond 0.6.0 (defaults.toml.old):
  changed  dashboards.sensors   widgets

Copy a table to start from: elamond defaults dashboards.sensors
```

A table the version before didn't have, or changed, is one your settings may want: the new version
doesn't add it to them by itself.

### Nix

On NixOS and with Home Manager, `services.elamond.settings` holds the defaults with `mkDefault`,
and `schema = 3`: what you set there overrides them key by key, a list (a dashboard's `widgets`)
replacing theirs whole, and a table of your own (`dashboards.desk`, `profiles.owner`) adds to them.
`services.elamond.defaults` holds them as they are, read-only. The settings are written into the
Nix store and `ELAMON_CONFIG` names them (Home Manager links elamond.toml to them); the drop-ins in
~/.config/elamon/elamond.d/, phones.toml among them, are still your own to write. Under the NixOS
module, an elamond.toml of your own in ~/.config/elamon is read right after the module's, as a drop-in would be, so nothing it held is
lost; the journal and `elamond check` say so.

## Phones

Each paired phone has a profile of its own, in `~/.config/elamon/elamond.d/phones.toml`: a drop-in
the service writes when a phone pairs, which is yours to edit like any other. A key you add to a
phone's table changes how that phone looks, at once; `use` names the profile it starts from.

```toml
# Profiles of the paired phones, one each: elamond pair adds them (docs/config.md#phones).
# Change how a phone looks here, key by key; `use` is the profile it starts from.

# Galaxy S25, paired 2026-11-12 (0123abcd)
[profiles.galaxy-s25]
name = "Galaxy S25"
use = "owner"
theme = "dark"

# Pixel 9, paired 2026-11-12 (4567cdef)
[profiles.pixel-9]
name = "Pixel 9"
use = "default"
```

| when | phones.toml |
|---|---|
| `elamond pair` | a table for the phone, its id from the phone's name (`Galaxy S25` → `galaxy-s25`; one taken already gets `-2`, `-3`…; a name without a letter or digit of `a-z`, `0-9` → `phone`), with `name` (the phone's) and `use`: the profile `--profile` names, or `default_profile`; a comment above says which phone it is. The phone's entry among the paired ones (`elamond clients`) has it as its profile. |
| `elamond pair --profile ID`, ID a phone's table that no paired phone has | the phone takes that table as its own, as it is, instead of a new one |
| the same phone pairs again, with its key still good | the new entry takes the table the old one had, and the old entry is unpaired: the app shows its key to the service ([protocol](protocol.md#post-apiv1pair-no-token)) |
| a phone pairs whose key is gone (the phone was reset, or its entry unpaired), and an unpaired phone of the same name had a table | `elamond pair` asks whether to give that table to it; `--yes` gives it none by name, nor does `--profile`, which decides |
| a phone on Elamon 0.6.1 gets a key of its own after Import pairings | a table for it, using the profile of the phone the key came from |
| `elamond revoke` | the phone's table goes when it is as pairing wrote it (`name` and `use` alone) and nothing else has it: no profile uses it, it isn't `default_profile`, no other phone has it as its profile; otherwise it stays, with a note, for a phone paired again to take |
| the phone forgets the computer (*Forget computer* in the app, [`unpair`](protocol.md#post-apiv1unpair)) | its entry is unpaired, and its table stays, always, with a note above it (`# Unpaired 2026-11-15; kept for a phone paired again (elamond pair offers it to one named "Galaxy S25").`): paired again, the phone is offered it by name |

`elamond check` names each table of phones.toml that no paired phone has as its own: when it was
unpaired (from its note), and that `elamond pair` offers it to a phone of the same name, or that it
can be taken out. A table a phone has as its profile without it being its own (`elamond clients
profile`), or that a profile uses, isn't named.

`elamond clients profile ID PROFILE` still gives a phone any profile of the settings, one shared by
several phones say; its own table stays in phones.toml, for `elamond clients profile ID <its table>`
to give back. A phone whose profile is gone from the settings gets `default_profile`.

The service writes phones.toml only from the lock the paired phones are kept under, in one step
(a new file renamed over it), keeping its permissions (only yours, for a new one), comments and
everything else in it as they were; of the comments it changes only the lines it wrote above a
phone's table, which phone has it and that it was unpaired, when another phone takes the table. A
link there it leaves alone: a phone then pairs without a table, with `use`'s profile as its
profile. Settings that wouldn't parse with a new table in, it doesn't write.

Ids — of dashboards, actions, links, profiles, templates, widgets, action parameters and sources
— are 1 to 32 characters `a-z`, `0-9`, `_` and `-`, starting with a letter or a digit. Ids with a
dot are the service's own (built-in actions, a station's actions, dashboards made by `for_each`).

## Top level

The default is what a key left out is; the starter file writes most of them out.

| key | type | default | |
|---|---|---|---|
| `schema` | `3` | | which settings these are ([above](#the-starter-file)) |
| `listen` | list of addresses and interface names (letters, digits and `_.@-`) | `["::"]` | where the service listens ([guide](guide.md#where-the-service-listens)) |
| `port` | integer | 47823 | |
| `names` | list of host names | `[]` | names that reach this computer, offered after its addresses: in the pairing link, and to paired phones |
| `name` | string | the host name | the computer's name in the app |
| `mdns` | bool | `true` | announce the service on the local network |
| `interval_ms` | integer, 250–10 000 | 1000 | the service's tick: how often it reads the computer |
| `history_minutes` | integer, 1–1440 | 10 | how far back graphs reach |
| `control` | bool | `false` | let paired clients act on the computer, as far as their roles allow |
| `default_role` | `glance` \| `monitor` \| `operate` \| `admin` | `admin` | the role of a client paired without `--role` |
| `default_profile` | profile id | `default` | the profile a phone's own table uses when `elamond pair` isn't given `--profile`, and the profile of a phone whose profile is gone |
| `links` | table of [links](#links) | | |
| `actions` | table of [actions](#actions) | | |
| `profiles` | table of [profiles](#profiles) | | |
| `dashboards` | table of [dashboards](#dashboards) | | |
| `templates` | table of [templates](#templates) | | |
| `sources` | table of [sources](#sources) | | UPSes and power stations; what commands print, web services answer and files hold |
| `history` | [table](#the-long-history) | | how long the hourly history is kept |

Settings schema 1 (elamond 0.5 and before) had `[[links]]`, `[[actions]]` and `units`; they are
refused with a note on what replaces them: `[links.<id>]`, `[actions.<id>]`, and a unit named on
its link.

## Links

Web pages this computer hosts, listed in the `links` table (the Control dashboard shows it) with
whether something answers on their port and the state of their unit.

```toml
[links.jellyfin]
name = "Jellyfin"
url = "http://{host}:8096"
unit = "jellyfin.service"
scope = "system"
```

| key | type | default | |
|---|---|---|---|
| `name` | string | required | |
| `url` | string | required | `http://` or `https://`, or a bare `host:port`; `{host}` becomes the address the client reaches the computer at |
| `unit` | string | | a systemd service unit clients may start, stop and restart (`unit.*` actions) |
| `scope` | `user` \| `system` | `user` | the unit's |
| `icon` | [icon](#icons) | `globe` | |

## Sources

Where values come from beside the computer's own readings: each `[sources.<id>]` is of a `kind`.

| kind | reads |
|---|---|
| `nut` | a UPS or a power station, as a NUT server (upsd) serves it ([below](#upses-and-power-stations-nut)) |
| `command` | what a program prints ([below](#commands-web-services-and-files-command-http-file)) |
| `http` | what a web service answers |
| `file` | what a file holds |

The id names the source's values (`eco.battery.charge`, `app.state`: [a source's
values](#a-sources-values)), and a `nut` source's dashboard (`ups.eco`). It is an id as others are,
but it may not be the first part of one of the computer's addresses (`cpu`, `mem`, `swap`, `gpu`,
`temp`, `fan`, `fans`, `disk`, `disk_io`, `net`, `battery`, `power`, `volume`, `link`, `system`,
`displays`), nor `item`. Each kind takes its own keys: `elamond check` names a key of another
(`no key "url" here`) and a kind that isn't one of the four (`no source kind "x"; …`). Settings
without sources are as they were before elamond 0.7.

`elamond values <id>` reads the source there and then (within 5 seconds for a `nut` one, its
`timeout` for the others) and prints its values as the service would name them, each with its unit,
format and role, or why it couldn't read it. `elamond values` alone ends with a line naming the
sources (`Sources: app, eco, zone (elamond values <id>)`).

### UPSes and power stations: `nut`

elamond reads a UPS or a power station as upsd serves it, through the upst-core library (of the
upst project), shows its values beside the computer's ([values](#a-sources-values)), keeps their
[history](#the-long-history), and runs the station's commands and settings the source allows
([station actions](#station-actions)). Any UPS NUT has a driver for is served by upsd; an EcoFlow
station by nutflow, a NUT server of its own.

```toml
[sources.eco]
kind = "nut"
ups = "ups"
capacity_wh = 2048
user = "elamon"
password_file = "/home/me/.config/elamon/nut.password"
commands = ["outlet.*.load.*", "beeper.*"]
settings = ["experimental.battery.charge.max", "experimental.output.xboost"]
outlets."1".critical = true
```

| key | type | default | |
|---|---|---|---|
| `kind` | `"nut"` | required | |
| `ups` | string, 1–64 characters, no spaces | required | the UPS's name on upsd, as in `upsc ups@localhost` |
| `host` | string | `"localhost"` | where upsd is |
| `port` | integer, 1–65 535 | 3493 | |
| `capacity_wh` | number above 0 | | the battery's energy, for `<id>.estimate.watts` (nutflow reports no rating to guess it from) |
| `interval_ms` | integer, 200–60 000 | 1000 | how often upsd is asked while some client has a dashboard (or the header) on screen that shows one of the source's values |
| `background_ms` | integer, `interval_ms`–60 000 | 10 000, or `interval_ms` when that is more | how often otherwise, while its history is kept or a graph or timeline of the settings shows it; when neither, upsd isn't asked at all |
| `history` | bool | `true` | keep the [long history](#the-long-history) of every number the source has |
| `recorder` | a socket path, or `host[:port]` | | an `upst record` to fill the history and the timeline from, with what happened while elamond wasn't watching |
| `user`, `password_file` | string; an absolute path | | the NUT user that runs commands and settings, and the file whose first line is its password: both or neither |
| `commands` | list of patterns | `[]` | the station's commands clients may run; `*` stands for any run of characters (`outlet.*.load.*`), as in nutflow's `allow` |
| `settings` | list of patterns | `[]` | the station's writable variables clients may set |
| `role` | `operate` \| `admin` | `admin` | the least role that may run the source's commands and settings |
| `outlets."<N>".critical` | bool | `false` | an outlet that powers this computer: its commands but the one that turns it on are typed in |

`elamond check` also finds: `user` without `password_file` or the other way round, or a relative
`password_file`; `commands` or `settings` without them; a pattern of anything but `a-z`, `0-9`,
`.`, `_`, `-` and `*`; an outlet that isn't a whole number, or a key there but `critical`; a
`recorder` that is empty. It never connects to upsd: what the station has, it doesn't check.

The password file is yours to write, readable by you alone (`chmod 600`). elamond reads its first
line each time it runs a command or a setting, and the password goes nowhere else: not to
clients, the journal, errors, `elamond values` or `elamond check`, nor backups. A file that is
missing, unreadable or that others may read is a warning in `elamond check` (on NixOS the build
checks the settings where the file isn't), and an action of the source then fails (`failed`,
saying which). Keep it out of `elamond.d/` as a `.toml`, which would be read as settings:
`elamond check` warns of that too. The NUT user is upsd's (`upsd.users`, with `instcmds` and
`actions = SET`) or nutflow's (with `allow`), and needs no more than `commands` and `settings` name.

Commands and settings are offered only as the station offers them, as far as `commands` and
`settings` allow: a phone sends a command's name or a setting's value, never anything the service
didn't check against both.

On NixOS and with Home Manager:

```nix
services.elamond.settings.sources.eco = {
  kind = "nut";
  ups = "ups";
  user = "elamon";
  password_file = "/run/secrets/elamon-nut";   # a secret outside the Nix store (sops-nix, agenix…)
  commands = [ "outlet.*.load.*" ];
  outlets."1".critical = true;
};
```

`elamond values eco` also prints the station's commands and writable settings, with their lists
or ranges and whether `commands` and `settings` allow them; the estimates and the timeline come
from the running service.

### Commands, web services and files: `command`, `http`, `file`

What a program prints, what a web service answers or what a file holds, read every so often and
shown as values ([a source's values](#a-sources-values)): a service's own status page, a figure a
script works out, a number from /sys.

```toml
[sources.up]
kind = "command"
command = ["uptime", "--pretty"]
format = "text"
interval = 60

[sources.app]
kind = "http"
url = "http://localhost:8080/status"
token_file = "/home/me/.config/elamon/app.token"
interval = 30

[sources.charge]
kind = "file"
path = "/sys/class/power_supply/BAT0/capacity"
format = "text"
interval = 5
```

| key | kinds | type | default | |
|---|---|---|---|---|
| `kind` | | `"command"` \| `"http"` \| `"file"` | required | |
| `command` | `command` | list of strings, the first not empty | required | the program and its arguments, run without a shell |
| `url` | `http` | `http://` or `https://`, a host, no spaces | required | asked with `GET` |
| `token_file` | `http` | an absolute path | | a file whose first line is a token, sent as `Authorization: Bearer <token>` |
| `header` | `http` | 1–64 of `A-Z`, `a-z`, `0-9` and `-` | | the header to send the token in instead (`X-Api-Key: <token>`); only with `token_file` |
| `path` | `file` | an absolute path | required | |
| `format` | all three | `json` \| `text` \| `lines` | `json` | how what it reads is taken ([values](#a-sources-values)) |
| `interval` | all three | integer, seconds, 1–86 400 | 10 | how often it is read while it is needed |
| `timeout` | all three | integer, seconds, 1–60 | 5 | a read that takes longer fails; it may be longer than `interval` |
| `history` | all three | bool | `false` | keep the [long history](#the-long-history) of its numbers |
| `role` | all three | `glance` \| `monitor` \| `operate` \| `admin` | `monitor` | the least role that sees its values |

A source is read only while it is needed: while some client has a dashboard (or the header) on
screen that shows one of its values, while a graph or a spark of the settings shows one, or with
`history = true`; otherwise not at all. Needed again, it is read at once if its last read is older
than `interval`. One read of a source runs at a time, the next waiting for it; a slow one never
holds up the computer's readings or another source's.

- A command runs as elamond's user, with nothing on its input, in a process group of its own:
  at its `timeout` it is killed, and once it ends whatever it started is killed too. What it prints
  is read up to 64 KiB, and the first line it writes to stderr says why when it fails.
- A web service is asked through curl (on elamond's PATH; the Nix package brings its own), never
  through a proxy, and no redirect is followed. Its answer is read up to 64 KiB.
- A file is read up to 64 KiB, and only a regular file is: /proc and /sys files are, a FIFO, a
  socket or a device isn't (`not a regular file`).

The token file is yours to write, readable by you alone (`chmod 600`). elamond reads its first
line, trimmed, at each request and sends it in the header alone: it goes to no client, the
journal, errors, `elamond values` or `elamond check`, nor backups. A token file that is missing,
unreadable or empty fails the read, and so does one that others may read
(`token_file may be read by others: chmod 600 it`); `elamond check` warns of each but the empty one.
A user or password in the URL (`http://user:pass@…`) is a mistake: `put the credentials in
token_file, not in the URL`.

`elamond check` also finds: an empty `command`, or one whose program is `""`; a `url` that isn't
`http://` or `https://` with a host, or that has spaces in it; a `header` of other characters, or
without `token_file`; a relative `token_file` or `path`. It runs, asks and reads nothing (but the
token file's permissions). The journal says when a source starts failing and when it reads again
(`source app: HTTP 404`), once each, and never a command's arguments or what it wrote to stderr.

## The long history

Graphs reach back further than `history_minutes`: the service keeps every value a graph or a
spark of the settings shows, and every number of each source with `history = true` (256 a
source at most), at three steps, each point the mean of the readings in it:

| step | kept | |
|---|---|---|
| 10 seconds | a day | the `1h` and `24h` ranges |
| 5 minutes | 30 days | `7d` and `30d` |
| an hour | for good, or `keep` days | `1y` and `all` |

```toml
[history]
keep = "all"   # or a number of days, 30 or more: hourly points older than that go
```

| key | type | default | |
|---|---|---|---|
| `keep` | `"all"`, or an integer of days, 30 or more | `"all"` | how long the hourly points are kept; the others keep their windows |

It is in `~/.local/share/elamon/history/` (the service's data folder), a file for each value and
step, 8 bytes a point, and a file of power changes for each `nut` source (its last 10 000). The
service writes it every 5 minutes and as it stops, never more often and without waiting for the
disk, so a crash loses those minutes at most; it trims each file to its window as it goes, and
after a restart the graphs start from it. A value takes about 0.3 MB its first year and 70 KB a
year after: a computer's graphs some 4 MB, a power station's ninety-odd numbers some 25 MB the
first year and 6 MB a year after. `elamond check` says how much there is:
`history: 12.3 MiB in 274 files (oldest 2026-11-16), not in backups`.

It isn't in backups: `elamond backup` leaves it out, as it is large and nothing the service needs
to be the same computer to its phones; copy the folder to keep it. Deleting it loses the long
ranges and nothing else.

With a `recorder`, at each start a source fills in the history and the timeline from where it
left off, from what `upst record` kept meanwhile (all of it the first time), and starts its
estimates from the last hour of it. A recorder that can't be reached, or that doesn't know the
UPS, is one line in the journal; the source works without it.

## Actions

What a client may run on the computer. There are built-in ones, and the ones the settings add,
each running a command the settings give: a client can choose among the values the settings allow,
never write a command.

```toml
[actions.mode]
name = "Mode"
command = ["leannix", "{mode}"]
params.mode = { choices = ["server", "desktop"], labels = { server = "Server", desktop = "Desktop" } }
role = "operate"
```

| key | type | default | |
|---|---|---|---|
| `name` | string | required | |
| `command` | list of strings | required | the program and its arguments, run without a shell; an argument that is exactly `"{param}"` is replaced by that parameter's value |
| `params` | table of [parameters](#parameters) | `{}` | |
| `role` | `operate` \| `admin` | `operate` | the least role that may run it |
| `confirm` | bool | `true` | the client asks before running it (unless the profile turns confirmations off) |
| `unlock` | bool | `false` | the client unlocks the phone first, whatever the profile says |
| `timeout` | integer, seconds, 1–3600 | 60 | the command is stopped after it |
| `icon` | [icon](#icons) | `terminal` | |

A `{name}` argument naming no parameter is a mistake `elamond check` reports; any other argument is
passed as it is, braces included. Parameters are substituted only whole, so a value never becomes
part of another argument or an option.

### Parameters

| form | kind | the value passed |
|---|---|---|
| `{ choices = ["a", "b"], labels = { a = "A" }, default = "a" }` | `choice` | one of `choices`, as written |
| `{ range = [0, 100], step = 5, unit = "%", default = 50 }` | `range` | a number within the range, on a step from its start; written as an integer when `step` and the start are (`100`, never `100.0` or `1e2`); zero never as `-0` |
| `{ type = "bool", default = false }` | `bool` | `true` or `false` |

Each may have a `label` (what a form calls it; by default its id as words, `fan_speed` →
"Fan speed"), and a choice `labels` for its values (by default each value as words, likewise). A
parameter without `default` must be given.

### Built-in actions

| id | kind | role | parameters | |
|---|---|---|---|---|
| `power.poweroff`, `power.reboot`, `power.suspend`, `power.lock` | `power` | admin | | logind; those the computer can't do are left out |
| `profile.set` | `profile` | operate | `profile`: choice of the power profiles there are | power-profiles-daemon |
| `process.signal` | `signal` | admin | `pid`: integer; `signal`: `term` \| `kill`; `start`: integer, optional | never the service, its parents or PID 1; with `start`, only the process that started then |
| `unit.start`, `unit.stop`, `unit.restart` | `units` | operate | `link`: choice of the links with a unit | |
| `media.play_pause`, `media.next`, `media.previous` | `media` | operate | `player`: string | MPRIS |
| `media.seek` | `media` | operate | `player`: string; `position`: number, seconds | |
| `volume.set` | `media` | operate | `level`: range 0–100 (%), optional; `muted`: bool, optional | the default output |
| `nut.command.<source>` | `nut` | the source's `role` | `cmd`: choice of the station's commands `commands` allows | [below](#station-actions) |
| `nut.set.<source>.<variable>` | `nut` | the source's `role` | `value`: choice or range, as the station has it | |

Actions the settings add are of kind `actions`. The kind is what a profile's `unlock.for` names.
Every action also needs control: `control = true`, and no `elamond control off`.

### Station actions

A `nut` [source](#sources) with `commands` adds `nut.command.<source>`, whose one parameter `cmd`
is a choice of the commands the station offers that a pattern of `commands` matches, each
labelled as upsd describes it (`Turn off AC outlets, group 2`). With `settings`, each writable
variable a pattern matches that takes a list of values (`ENUM`) or one range (`RANGE`) is an
action `nut.set.<source>.<variable>` with one parameter `value`, the list as a choice or the range
on a step of 1 in the variable's unit; one of another kind, or of several ranges, isn't offered,
and the journal says so once.
They come once upsd has said what the station offers, and stay while it can't be reached; until
then the widgets that run them are left out, and so is a toggle whose `on` or `off` isn't among
them.

Each asks before it runs. One that cuts this computer's power has to be typed in on the phone,
whatever the profile's `confirm_actions`: a command on an outlet with `critical = true` but the one
that turns it on (`outlet.1.load.off`), and a command upst-core counts as dangerous that isn't
about one outlet (`load.off`, `shutdown.*`, `calibrate.start`…). Turning any other outlet off is
asked as other actions are. The service refuses one that wasn't typed in, so an app before 0.7
can't run it.

A source's actions run one at a time, each answered once the station says it did it (or why not),
25 seconds at most: an upsd that can't say whether the station did it counts as a failure. The
activity log has them as `nut eco outlet.2.load.off` and `nut eco experimental.battery.charge.max 90`.

## Profiles

How a client looks and behaves. The app saves none of this: it takes its profile from the computer
with the dashboards. Each paired phone has one of its own in [phones.toml](#phones), which `use`s
another (`elamond pair --profile ID`, or `default_profile`); `elamond clients profile ID PROFILE`
gives it any other.

```toml
[profiles.owner]
use = "default"
theme = "dark"
pure_black = true
dashboards = ["overview", "power", "processes", "control"]
unlock = { for = ["power", "signal", "units", "actions"], grace = "1m" }

[profiles.owner.android]
dynamic_color = false
```

The part every client reads:

| key | type | default (`default` profile) | |
|---|---|---|---|
| `use` | profile id | | start from that profile; the keys here override its own |
| `name` | string | the id | what `elamond clients` and the app's settings page call it |
| `theme` | `system` \| `light` \| `dark` | `system` | |
| `pure_black` | bool | `false` | black instead of dark grey in the dark theme |
| `animations` | bool | `true` | rolling numbers, moving gauges, transitions |
| `interval_ms` | integer, 250–60 000 | 1000 | how often the client asks for readings; never faster than the service's tick |
| `fahrenheit` | bool | `false` | temperatures in °F: the service sends them so |
| `network_bits` | bool | `false` | network rates in bits per second: the service sends them so |
| `confirm_actions` | bool | `true` | ask before actions that ask (`confirm`); off, only unlocking still asks |
| `unlock.for` | list of kinds | `["power", "nut", "signal", "actions", "units"]` | action kinds the client unlocks the phone for first: `power`, `nut` (a station's), `signal`, `actions`, `units`, `profile`, `media`; settings written before 0.7 list theirs without `nut`, which a station's actions then don't unlock for until it is added |
| `unlock.grace` | `every_time` \| `1m` \| `5m` | `every_time` | how long an unlock covers later actions of its kind or a lesser one |
| `dashboards` | list of dashboard ids | `["overview", "processes", "sensors", "control"]` | the client's tabs, in order; those the client's role can't see are left out |
| `header` | dashboard id | `header` | widgets shown as small chips under the computer's name on every tab; `""` for none |

`[profiles.<id>.android]`, for the app only (the service checks the keys, but doesn't act on them):

| key | type | default | |
|---|---|---|---|
| `dynamic_color` | bool | `true` | colours from the wallpaper (Android 12+) |
| `blur` | bool | `true` | frosted-glass bars and sheets (Android 12+) |
| `haptics` | bool | `true` | |
| `keep_screen_on` | bool | `false` | while a computer's screen is open |

`[profiles.<id>.web]` is for the web page (Elamon 0.8) and takes no keys yet.

A `use` chain is followed to its end (a loop is a mistake); `android` and `web` are inherited key by
key as well. A profile that names a dashboard that doesn't exist is a mistake.

## Dashboards

A dashboard is a page of widgets: a tab when a profile lists it, otherwise a page a widget opens
(`open`), or the chips under the computer's name (a profile's `header`). The standard ones, which
[the starter file](#the-starter-file) has, are `overview`, `ups`, `processes`, `sensors` and
`control`, the `header`, and the pages their tiles open: `cpu`, `memory`, `gpu.<key>`, `network`,
`storage`, `battery`, `activity`, `journal` and `unit_log.<link>`.

`ups` is one dashboard for each `nut` [source](#sources) (`for_each = "ups"`: `ups.eco`, titled
by the station's model), and none without one: the starter's `default` profile has it as its
second tab (`["overview", "ups", "processes", "sensors", "control"]`), so a source needs nothing
but its `[sources.<id>]`. It shows what the station has and leaves out what it hasn't, the same
for a UPS and an EcoFlow: the status, with how long it has been so and its flags; the charge, with
the time left; a timeline of the power; the power in and out, and the solar inputs when there
are; the battery's charge and discharge limits, health, capacity, voltage and current, with the
estimates to empty and to full; the outlets; the voltages, frequencies and temperatures; graphs of
the charge and the power; the model and firmware. Below, once the source has `commands` or
`settings`, a switch for each outlet and the beeper, and sliders and choices for the station's
settings (the charge limits, the charging power, X-Boost, timeouts, the display). To change it,
`elamond defaults dashboards.ups` gives the table to copy. With two sources, each one's switches
list the outlets of both. Settings started before 0.7 have no `ups` dashboard: copy it the same
way into `elamond.d/dashboards.toml`, and add `"ups"` to the profile's `dashboards`.

`journal` is the system journal from warnings up, which Control's *Journal* button opens.
`unit_log` is one dashboard for each [link](#links) with a unit (`for_each = "links"`:
`unit_log.jellyfin`, titled by the link's name), the journal of its unit, which a link's row in
Control's Links table opens (its row action `{ open = "unit_log", label = "Journal" }`); a link
without a unit has none. Both are for `monitor` and up, and neither is a tab of the starter's
profiles. Settings started before 0.8 have neither, nor the button and the row action: copy
`elamond defaults dashboards.journal` and `dashboards.unit_log` into `elamond.d/dashboards.toml`,
and the two widgets from `dashboards.control`.

```toml
[dashboards.power]
title = "Power"
icon = "battery"

[[dashboards.power.widgets]]
type = "status"
value = "eaton.ups.status"
label = "UPS"
map = { OL = "On mains", OB = "On battery", LB = "Battery low" }
levels = { OB = "warning", LB = "critical" }
```

| key | type | default | |
|---|---|---|---|
| `title` | string | required | |
| `icon` | [icon](#icons) | `overview` | |
| `role` | role | `glance` | the least role that sees it |
| `for_each` | [collection](#collections) | | one dashboard per item, its id `<id>.<item key>`; `{item}` and `{item.<field>}` in its strings |
| `widgets` | list of [widgets](#widgets) | `[]` | in order |

## Widgets

Widgets fill a grid of 4 columns, in order, each as wide as its `span`; on a narrow screen the grid
has 2 and each span is halved, rounding up. A widget that doesn't fit beside the one before starts
a row, and the last widget of a row takes the columns left free, so no row ends in a gap (a lone
tile under two others is as wide as the screen).

Every widget takes:

| key | type | default | |
|---|---|---|---|
| `type` | one of the types below | required (or from `use`) | |
| `id` | id | its place in the list, from 0 | unique within the dashboard (or group) |
| `span` | 1–4 | 2, or 4 for `graph`, `table`, `list`, `group`, `media`, `text`, `timeline` and `log` | |
| `label` | string | | its title |
| `icon` | [icon](#icons) | | |
| `color` | [colour](#colours) | | its accent |
| `role` | role | the least role its values and action need | the least role that sees it |
| `when` | address | | shown only while that value exists on this computer; a source's value, once the source has had it ([below](#a-sources-values)) |
| `open` | dashboard id | | tapping it opens that dashboard |
| `caption` | string | | a line under it, which may hold values |
| `collapsed` | bool | | a `group` or `table` that can be folded, and starts folded; the client remembers it after |
| `use`, `with` | template id, table | | [templates](#templates) |
| `for_each` | collection or list of strings | | one widget per item |

Strings may hold `{address}` or `{address:format}`, filled in by the service with values as they
are now (`caption = "{mem.used:bytes} of {mem.total:bytes}"`); in a widget made by `for_each`,
`{item}` is the item's key (or the string) and `{item.<field>}` one of its fields; in one made from a
template, `{name}` is that key of `with`. Of a string in parts joined by ` · `, a part with a value
that is missing is left out, its separator with it (`"{cpu.temp:temp} · {cpu.freq_max:mhz}"` is just
the clock on a computer without a CPU temperature); a string left empty isn't shown.

### Value and thresholds

`value`, `gauge`, `meter`, `status`, `graph` and `list` rows show values:

| key | type | |
|---|---|---|
| `value` | address | the value shown |
| `unit` | string | in place of the address's own |
| `format` | [format](#formats) | in place of the address's own |
| `map` | table, value → text | text in place of a value (`OB` → `On battery`) |
| `empty` | string | shown when the value is missing (`Asleep`) |
| `warning_above`, `critical_above`, `warning_below`, `critical_below` | number or address | where the level turns `warning` and `critical`; the address's own limits otherwise ([levels](#levels)) |
| `soft` | number | how far below (above) `warning` the colour starts turning; the address's own otherwise |
| `levels` | table, value → `normal` \| `warning` \| `critical` | the level of a mapped value (`status`) |

### Types

| type | | its own keys |
|---|---|---|
| `value` | a number or text, large | `spark` (a small graph of the value under it) |
| `gauge` | a ring | `min` (0), `max` (number or address; the address's own) |
| `meter` | a bar | `min`, `max` |
| `status` | a state as a coloured word | `map`, `levels` |
| `graph` | lines over time, the newest values above them | `values` (list of addresses), `labels` (one per value), `colors` (one per value), `range` (seconds, at most `history_minutes`; all of it by default), `height` (`small`, `medium`, `large`), `min`, `max` (a fixed scale, a number or an address; it grows to fit otherwise), `floor` (the least the scale goes down to), `empty` (shown while the first value is missing), `ranges` (bool, `true` by default: a `medium` or `large` graph offers the [long history](#the-long-history), 1 hour to all of it, beside its own `range`; a `small` one never does) |
| `timeline` | when a source was on line, on battery, or nothing was known, as a bar | `source` (required: a `nut` source's id, or `{item}` in a dashboard with `for_each = "ups"`), `range` (seconds, 600–2 592 000; 86 400, a day, by default), `ranges` (bool, `true` by default, as a graph's), `legend` (`duration`, `percent` or `both`; `duration` by default: what the legend under the bar tells of each state over the range shown — how long it lasted, its share of the time something was known (on line and on battery; *No data* is told by its duration alone), or both: `On battery · 4% · 12 min`; an app before 0.8.1 tells durations) |
| `list` | rows of a label and a value | `items`: list of `{ label, value, text, format, unit, map, empty, when }`: `value` an address, or `text` a string that holds values (each may `for_each`); a row with nothing to show is left out |
| `bars` | one bar per item of a collection | `source`, `field`, `max` (number or address), `labels` (the field under each bar; none by default) |
| `table` | rows from a [collection](#collections) | `source`, `columns`, `sort`, `sorts`, `search`, `limit`, `filter`, `hide_empty`, `row_open`, `row_actions`, `expand` (below) |
| `button` | runs an action, or opens a dashboard | `action`, `params` (fixed values), `open` |
| `toggle` | a switch | `value` (the state now), `action`, `param` (the parameter it sets), `on`, `off` (its values; `true`/`false` by default), `map` (the state read through it: `{ on = "outlet.1.load.on", off = "outlet.1.load.off" }` for a switch whose values are commands) |
| `select` | one of an action's choices | `value` (the choice now), `action`, `param` |
| `slider` | a number in an action's range | `value`, `action`, `param` |
| `text` | words | `text` (may hold values), `level` |
| `group` | a card around widgets | `widgets`, `collapsed` (whether it starts closed; the client remembers it after); with `for_each`, its widgets are repeated for each item inside the one card, their ids `<id>.<item key>` |
| `media` | the players, with covers and seeking, and the volume | |
| `log` | lines of the systemd journal, the newest at the bottom | `scope`, `unit`, `level`, `lines`, `search` ([below](#the-journal-log)) |

A `table`:

| key | type | |
|---|---|---|
| `source` | collection | required |
| `columns` | list of `{ field, label, format, unit, bar, grow, map }` | `bar = { max = … }` draws the cell as a bar, full at a number, at a field of the row (`"crit"`), or at `"top"`, a quarter above the largest in the column; `grow` takes the room left; a column without `label` has no heading |
| `sort` | `{ field, desc }` | the order rows come in |
| `sorts` | list of fields | other orders a client may ask for |
| `sort_labels` | table, field → label | what the chips that pick one of `sorts` say (`{ cpu = "CPU", mem = "Memory" }`); the field's column label, or the field, otherwise |
| `search` | bool, or list of fields | a search field: `true` searches the text columns and the text fields of `expand`, a list those fields; the client searches the rows it has |
| `limit` | integer, at most 10 000 | rows sent at most (500) |
| `filter` | table, field → value | only rows whose fields have those values (a string, number or bool) |
| `hide_empty` | bool | hidden while it has no rows |
| `row_open` | field | a row opens that field's URL when tapped (`links`) |
| `row_actions` | list of `{ action, label, params }` or `{ open, label }` | `params` values may be `"{field}"`, filled from the row; `open` names a dashboard with `for_each` over the table's `source`, and a row opens its own of them (`unit_log` → `unit_log.<the row's key>`) |
| `expand` | list of `{ field, label, format, unit, map }` | what a row shows when opened |

A row action with `open` is on the rows whose dashboard the client has (as far as its role sees
it); `elamond check` finds one whose dashboard isn't `for_each` over the table's `source`.

A widget whose action the client's role may not run, or the computer can't do, is left out; with
control off they stay, disabled. So is a toggle whose `on` or `off` its action doesn't take (a
station's command it doesn't offer, or `commands` doesn't allow).

A `timeline` reaches an app before 0.7 as a `text`: the source's status and how long it has been
so (`On battery · 12 min`), coloured by the status. A `log` reaches an app before 0.8 as a `text`
of level `warning`, `Update the app to read the journal.`, and a row action with `open` doesn't
reach it.

### The journal: `log`

```toml
[[dashboards.journal.widgets]]
id = "system"
type = "log"
scope = "system"
level = "warning"
label = "System journal"
```

| key | type | default | |
|---|---|---|---|
| `scope` | `system` \| `user` \| `unit` | `system` | the system journal (as far as elamond's user may read it), that user's own, or one unit's |
| `unit` | string | | with `scope = "unit"`: the `unit` of one of the [links](#links) (`"{item.unit}"` in a dashboard `for_each = "links"`); the link's `scope` says whether it is a system or a user unit |
| `level` | `debug` \| `info` \| `notice` \| `warning` \| `error` \| `critical` | `info` | the least level shown at first; the client may pick another |
| `lines` | integer, 10–1000 | 200 | the lines a client keeps on screen |
| `search` | bool | `true` | a search field |

Its role is `monitor` at least, whatever its `role` says: `glance` never sees the journal. A `unit`
no link names leaves it out (`elamond check` finds one written out), and so does an empty one, of
a link without a unit; a dashboard `for_each` left without widgets isn't there. The levels are the
journal's priorities: 0–2 `critical`, 3 `error`, 4 `warning`, 5 `notice`, 6 `info`, 7 `debug`,
`info` for a line without one. A client picks another level and searches (the text, unit and name
of each line) through the service, and loads older lines as it is scrolled back
([protocol](protocol.md#get-apiv1log)).

The service reads the journal with systemd's `journalctl`, and only while a client shows a `log`.
The system journal needs elamond's user in the `systemd-journal` group (or `adm`, or `wheel`, where
the journal's ACL lets them read it): without, a `system` log, and one of a system unit, shows
what that user may read, under a note saying how to read the rest. `elamond setup --check` says
which it is (`journal: the system journal is readable`, or `journal: only this user's own journal:
add the user to systemd-journal …`), and when there is no `journalctl`.

### Templates

```toml
[templates.history]
type = "graph"
range = 600
span = 2
height = "small"

[[dashboards.power.widgets]]
use = "history"
for_each = ["battery.charge", "ups.load"]
values = ["eaton.{item}"]
```

A template is a widget without a place. `use = "<id>"` starts a widget from it; the widget's own
keys win; `with = { key = "value" }` fills `{key}` in the template's strings.

## Values

Every reading has an address. `elamond values` lists those of this computer with their values,
units and formats.

| address | unit | format | |
|---|---|---|---|
| `cpu.usage` | % | `percent` | all cores |
| `cpu.temp` | °C | `temp` | the package |
| `cpu.freq` | MHz | `mhz` | the mean of the cores |
| `cpu.load1`, `cpu.load5`, `cpu.load15` | | `load` | |
| `cpu.model`, `cpu.cores`, `cpu.threads` | | `text`, `integer` | |
| `mem.used`, `mem.available`, `mem.total` | B | `bytes` | |
| `mem.percent` | % | `percent` | |
| `swap.used`, `swap.total` | B | `bytes` | |
| `swap.percent` | % | `percent` | |
| `gpu.<key>.usage`, `.mem_percent` | % | `percent` | missing while the GPU sleeps |
| `gpu.<key>.temp` | °C | `temp` | |
| `gpu.<key>.power` | W | `watts` | |
| `gpu.<key>.mem_used`, `.mem_total` | B | `bytes` | |
| `gpu.<key>.freq`, `.freq_max`, `.mem_freq` | MHz | `mhz` | |
| `gpu.<key>.fan` | % | `percent` | |
| `gpu.<key>.state` | | `text` | `active`, `sleeping` or `unknown` |
| `gpu.<key>.name`, `.short_name`, `.vendor`, `.driver` | | `text` | `short_name` as the `gpus` collection has it |
| `temp.<key>` | °C | `temp` | `.high` and `.crit`, the sensor's limits |
| `fan.<key>` | RPM | `rpm` | |
| `disk.<key>.used`, `.available`, `.total` | B | `bytes` | |
| `disk.<key>.percent` | % | `percent` | |
| `disk.read`, `disk.write` | B/s | `rate` | all disks |
| `disk_io.<key>.read`, `.write` | B/s | `rate` | |
| `net.rx`, `net.tx` | B/s | `rate` | wired and wireless links |
| `net.<key>.rx`, `.tx` | B/s | `rate` | |
| `net.<key>.rx_total`, `.tx_total` | B | `bytes` | |
| `net.<key>.up` | | `bool` | |
| `net.<key>.speed` | Mbit/s | `integer` | |
| `battery.percent`, `battery.health` | % | `percent` | |
| `battery.power` | W | `watts` | |
| `battery.energy_now`, `.energy_full`, `.energy_design` | Wh | `wh` | |
| `battery.time_left` | s | `duration` | |
| `battery.status` | | `text` | `charging`, `discharging`, `full`, `not_charging`, `unknown` |
| `battery.cycles` | | `integer` | |
| `battery.model` | | `text` | |
| `power.on_ac` | | `bool` | |
| `power.profile` | | `text` | power-profiles-daemon's |
| `power.platform_profile`, `power.governor`, `power.epp` | | `text` | |
| `volume.level` | % | `percent` | the default output's |
| `volume.muted` | | `bool` | |
| `link.<id>.up` | | `bool` | |
| `link.<id>.unit_state` | | `text` | |
| `system.name`, `.os`, `.kernel`, `.arch`, `.version` | | `text` | |
| `system.uptime` | s | `duration` | |
| `system.boot_time` | s | `time` | |
| `cpu.freq_max` | MHz | `mhz` | the fastest core now |
| `gpu.<key>.summary` | | `text` | `Asleep`, `Idle`, the clock, `Active` or `No readings`: what a GPU is doing when it has no load to show |
| `battery.status_text` | | `text` | `Charging`, `Discharging`, `Full`, `Not charging`, `Unknown` |
| `battery.time_text` | | `text` | `2 h 10 min left`, `full in 40 min` |
| `fans.summary` | | `text` | `2 fans · 2400 RPM` (the fastest), `Fans stopped`, or `5 sensors` without fans |
| `net.up_names` | | `text` | the interfaces that are up, but virtual ones, joined by ` · ` |
| `displays.summary` | | `text` | `2 connected · 1 on` |

With `fahrenheit` in the client's profile, temperatures come in °F; with `network_bits`, network
rates in bit/s.

Keys name a thing by what it is, not by where it is in a list: lower case, `a-z`, `0-9` and `_`,
anything else becoming `_`.

| `<key>` of | from | e.g. |
|---|---|---|
| a GPU | its vendor, then a number from the second of a vendor | `nvidia`, `amd`, `amd2`, `intel`, `gpu` |
| a temperature, a fan | chip and label | `coretemp_package_id_0`, `nvme_composite`, `thinkpad_fan1` |
| a disk | mount point, `/` being `root` | `root`, `home`, `run_media_usb` |
| a disk's I/O, a network interface | its name | `nvme0n1`, `wlp2s0` |
| a link | its id | `jellyfin` |

### A source's values

A `nut` [source](#sources) has a value for each variable its station reports, named
`<id>.<variable>` as NUT names it (`eco.battery.charge`, `eco.outlet.2.realpower`,
`eco.experimental.input.dc.1.realpower`): whatever the station has, `elamond values eco` lists. A
number is a number, anything else text. The unit comes from the name, as upst-core reads it: `V`
for a `voltage`, `A` a `current`, `Hz` a `frequency`, `°C` a `temperature`, `W` a `realpower`, `VA`
another `power`, `%` a `charge`, `load`, `humidity` or `efficiency`, `s` a `runtime`, `delay`,
`timer` or `uptime`, `Ah` a battery's `capacity`; and the format from the unit: `percent`, `temp`,
`watts`, `duration`, `number` for other numbers (`230.5 V`), `text`. Their role is `glance`, but
`monitor` for the driver's (`driver.*`), serial numbers, versions, vendor and product ids, dates
and `.id`s.

Beside them, what the service makes of them:

| address | unit | format | |
|---|---|---|---|
| `<id>.status` | | `text` | `On line`, `On battery`, `Low battery`, `Overload`, `On bypass`, `Output off`, `Forced shutdown`, `No status`, `Unknown`; while upsd isn't read, `Connecting`, `No data` (upsd answers, the station doesn't) or `Unreachable`. It has a level of its own, by the worst of its flags as upst-core weighs them (on battery, on bypass, output off, discharging, replace battery: `warning`; low battery, overload, forced shutdown, alarm: `critical`), else `normal`; `No data` is `warning`, `Unreachable` `critical`, `Connecting` `normal` |
| `<id>.status.power` | | `text` | `line` (on line, or on bypass), `battery`, `none`: as the timeline has it |
| `<id>.status.flags` | | `text` | the status's other flags, joined by ` · ` (`Charging · Replace battery`); missing when there are none |
| `<id>.status.since` | s | `duration` | how long it has been on line, or on battery |
| `<id>.link` | | `text` | `connecting`, `up`, `stale` (upsd answers, without data), `down` |
| `<id>.link.error` | | `text` | why not `up` (role `monitor`) |
| `<id>.estimate.watts` | W | `watts` | the draw (or the charging power) from how fast the charge goes, with `capacity_wh` |
| `<id>.estimate.to_empty`, `.to_full`, `.to_low` | s | `duration` | from how fast the charge goes: to 0 %, to 100 %, to `battery.charge.low`; each while it can tell |

Its numbers carry limits as the computer's do: `<id>.battery.charge` the battery's, `<id>.ups.load`
a load's, temperatures a temperature's ([levels](#levels)).

A `command`, `http` or `file` source has values by its `format`:

| address | | when |
|---|---|---|
| `<id>.<path>` | `json`: each leaf of what it read, by the keys of objects and the places in arrays (from 0), joined by dots (`app.queue.length`, `app.workers.0.name`); a number is a number, a string text (1000 characters at most, cut with `…`), `true` and `false` a bool, `null` no value at all. JSON that is only a number, a string or a bool is `<id>.value`. 8 levels deep and 256 leaves at most: the rest is left out, and the journal says so once | after a good read |
| `<id>.text` | `text`: all it read, trimmed, its control characters but line breaks taken out, 1000 characters at most (`…`); a number when the whole of it is one (`45000`) | after a good read |
| `<id>.<key>` | `lines`: of each line `key = value` or `key: value` (split at its first `=` or `:`, both sides trimmed), the value: a number, `true` or `false` a bool, else text; other lines are left out, and of a key twice the last counts | after a good read |
| `<id>.exit` | a command's exit code | after a run that ended of itself (not killed at its `timeout`) |
| `<id>.age` | whole seconds since the last good read; unit `s`, format `duration` | from the first good read on |
| `<id>.error` | why the last read failed; role `monitor` at least | while it fails |

Keys and the parts of a path are written in `A-Z`, `a-z`, `0-9`, `_` and `-`, anything else
becoming `_` (`build info` → `build_info`; an empty one is `_`); `exit`, `age` and `error` are the
service's own, and a key or leaf of one of those names is left out. A value has no unit, and the
format `number`, `text` or `bool` as it is (`<id>.age` aside): the widget says the rest with its
own `unit` and `format` (`format = "time"` for a Unix time in seconds; `unit = "%"` for a
battery's charge read from /sys). The service guesses nothing from
a name: a key `temperature` gets no °C and no limits. Their role is the source's `role`.

A command that exits with another code than 0 reads well only when what it printed gives at least
one value (beside its `<id>.exit`); one that gives none fails, as `exited with 7: <the first line
of stderr>` whatever it printed. A read also fails when the command can't start or runs past its
`timeout`; when what it read is over 64 KiB, or, with `format = "json"`, isn't JSON; when the web
service can't be reached, answers other than 2xx, or redirects (none is followed); when the file
isn't a regular file or can't be read. Then the values of the last good read go (`<id>.age` stays,
and grows), and `<id>.error` says why: `timed out after 5 s`, `exited with 7: …`, `not JSON: …`,
`HTTP 404`, `redirect to elsewhere (301), not followed`, `couldn't connect: …`,
`output over 64 KiB`, `not a regular file`.

A source's values come and go: nutflow leaves out a reading it hasn't had for a while, an outlet
that is off has no voltage, and while upsd isn't read a `nut` source has only `<id>.status`,
`<id>.status.power` and the `link`; a web service leaves out a key, and a read that fails leaves
none. So that its dashboard keeps its shape, a `when` or a list row that names a source's value
holds once the source has had that value since elamond started, up to 256 values a source, for a
role that sees it; the widget then shows `—`. One the source has never had leaves the widget out,
as for the computer's.

### Collections

Lists of things, for `table` and `for_each`, each item with a `key` and fields:

| collection | role | fields |
|---|---|---|
| `processes` | monitor | `pid`, `ppid`, `name`, `cmd`, `user`, `cpu` (%), `mem` (B), `status`, `start` (s), `read`, `write`, `io` (their sum) (B/s), `gpu` (%), `gpu_mem` (B) |
| `cores` | glance | `key` (0, 1, …), `usage` (%), `freq` (MHz) |
| `gpus` | glance | as `gpu.<key>.*`: `name`, `short_name` (`RTX 3050 Ti` for `NVIDIA GeForce RTX 3050 Ti Laptop GPU`), `vendor`, `driver`, `state`, `summary`, `usage`, `temp`, `power`, `mem_used`, `mem_total`, `freq`, `freq_max`, `fan` |
| `temps` | glance | `chip`, `kind` (`cpu`, `gpu`, `storage`, `wireless`, `board`, `battery`, `other`), `device`, `label`, `value`, `high`, `crit` |
| `fans` | glance | `chip`, `label`, `rpm` |
| `disks` | glance | `device`, `mount`, `fs`, `used`, `available`, `total`, `percent`, `removable` |
| `disk_io` | glance | `name`, `model`, `read`, `write` |
| `net` | glance | `name`, `kind` (`wired`, `wireless`, `vpn`, `virtual`), `up`, `addrs`, `rx`, `tx`, `rx_total`, `tx_total`, `speed` |
| `displays` | monitor | `name` (vendor and model, or the connector), `state` (`On`, `Off`, `Standby`, `Not in use`, `Disconnected`), `connector`, `gpu`, `connected`, `active`, `dpms`, `internal`, `vendor`, `model`, `year`, `size` (`15.6″ · 34 × 19 cm`), `mode` (`1920 × 1080 @ 144 Hz`), `preferred`, `max_refresh`, `position` (`1920, 0`) |
| `links` | monitor | `id`, `name`, `url`, `unit`, `scope`, `port`, `up`, `local_only`, `unit_state` |
| `activity` | monitor | `t` (ms), `phone`, `what`, `result`, `message` |
| `actions` | operate | the actions the settings add (not the built-in ones) that the role may run: `key` (the id), `name`, `icon` |
| `ups` | glance | one for each `nut` [source](#sources), from the start, whether upsd answers or not: `key` (its id), `name` (`device.model`, else `ups.model`, else its id), `model`, `mfr`, `status`, `power`, `link` (as `<id>.status`, `.status.power`, `.link`), `ecoflow` (bool: an EcoFlow station) |
| `outlets` | glance | one for each outlet a source has had since elamond started (`outlet.<N>.*`), by source and number, kept while upsd isn't read: `key` (`<id>.<N>`), `source` (the id), `id` (N), `desc`, `status` (`on`, `off`), `realpower` (W), `voltage` (V), `current` (A), `switchable` (bool), `critical` (bool, from the settings) |

A row's `level`, where its collection has one (`temps` by value, `processes` by CPU, `disks` by
percent, `cores` and `gpus` by usage), comes with it. In a string, `{item.<field>}` is written by
the field's format.

### Levels

The service judges each value against its thresholds and sends `normal`, `warning` or
`critical` with it, and how far it has turned toward the next (`blend`, 0–1), so every client
colours it alike:

| values | warning | critical | soft |
|---|---|---|---|
| `cpu.usage`, `mem.percent`, `swap.percent`, `gpu.*.usage`, `gpu.*.mem_percent`, `disk.*.percent` | above 75 | above 90 | 0 |
| temperatures | the sensor's `high`, or 20 below critical, never under 40 | the sensor's `crit`, or `high`, or 95 | 20 |
| `battery.percent`, a source's `battery.charge` | below 30 | below 15 | 0 |
| a source's `ups.load` | above 75 | above 90 | 0 |
| `processes` rows by `cpu` (a quarter of the CPU is a lot already) | above 18.75 | above 22.5 | 0 |
| anything else | — | — | |

`soft`: the colour starts turning toward `warning` that far before it. Between `warning` and
`critical`, `blend` is how far across it is. A widget's own `warning_above`… replace the address's
limits one by one; an address given there is read now, in the client's units.

What has a level: a number with limits, judged by them; a number without is `normal`, but a
duration, a moment or a text has none, unless it has a level of its own (a source's `status`). A
widget with `levels` gives each mapped value its level there, `normal` for the rest.

### Formats

How a client writes a number. Named ones:

| format | e.g. |
|---|---|
| `number` | `12.5` (up to one decimal) |
| `integer` | `13` |
| `percent` | `42%` |
| `bytes` | `512 B`, `1.50 KiB`, `12.5 MiB`, `120 GiB` |
| `rate` | `1.50 MiB/s`; with `bit/s` as the unit, `12.0 Mbit/s` |
| `temp` | `54°C`, `129°F` |
| `mhz` | `800 MHz`, `3.20 GHz` |
| `watts` | `12.3 W` |
| `wh` | `48.0 Wh` |
| `rpm` | `2400 RPM` |
| `load` | `1.25` |
| `duration` | `3 d 4 h`, `4 h 12 min`, `12 min` |
| `time` | the date and time, the client's way; from seconds, or from milliseconds when the field is `t` |
| `bool` | `yes`, `no` |
| `text` | as it is |

And a pattern: `"{:.0}"`, `"{:.2}"` — that many decimals, then the unit.

Halves round away from zero (`12.5%` reads `13%`); a unit follows its number after a space, but
`%`, `°C` and `°F`, which follow it directly. `bytes` and `rate` scale by 1024 (B, KiB, MiB, GiB,
TiB), a rate in `bit/s` by 1000 (bit, kbit, Mbit, Gbit); the scaled number has two decimals under
10, one under 100 and none above, the rounded figure deciding (1023.7 KiB reads `1.00 MiB`). The
service writes values into strings by these rules and the clients write the rest by them;
[`formats.json`](../elamond/fixtures/formats.json) holds cases both sides are tested against.

### Icons

`battery`, `bolt`, `chip`, `clock`, `control`, `eye`, `fan`, `gauge`, `globe`, `gpu`, `help`,
`laptop`, `leaf`, `lock`, `memory`, `monitor`, `moon`, `music`, `network`, `next`, `overview`,
`pause`, `play`, `power`, `previous`, `processes`, `restart`, `rocket`, `storage`, `terminal`,
`thermometer`, `volume`. A client draws one it doesn't know as `overview`.

### Colours

`cpu`, `memory`, `swap`, `gpu`, `rx`, `tx`, `read`, `write`, `battery`, `good`, `warn`, `hot`,
`primary`, `secondary`, `tertiary`.
