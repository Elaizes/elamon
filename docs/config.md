# Elamon settings reference (settings schema 2)

Everything the app and the web page show, and everything a phone may run, is set on the computer,
in elamond's settings: the dashboards and their widgets, the actions, how each phone looks and
behaves (its *profile*). The app keeps nothing of it; it draws what the service describes. These
settings are TOML on every distribution; on NixOS the module's `settings` mirror them one to one.

Files, read in this order, a later one adding to and overriding an earlier one:

1. what is built into elamond: the standard dashboards (`elamond defaults` prints them), the built-in
   actions and the `default` profile;
2. `~/.config/elamon/elamond.toml` (or the file `ELAMON_CONFIG` names);
3. `~/.config/elamon/elamond.d/*.toml`, by name.

A table with an id (`[dashboards.power]`, `[actions.mode]`, `[links.jellyfin]`,
`[profiles.owner]`, `[templates.history]`) merges with the one of the same id before it, key by
key; a list (`widgets`, `listen`, `dashboards` in a profile) replaces the one before it whole.
So a drop-in can add a dashboard, change one key of a built-in one, or replace its widgets.

`elamond check [FILE]` reads the settings as the service would and names the file and line of each
mistake; `elamond edit [FILE]` opens the file in `$EDITOR` and doesn't keep a broken one;
`elamond schema` prints the JSON Schema of these settings, for editors that check TOML against one
(taplo, Even Better TOML: `#:schema` at the top of the file); `elamond defaults` prints the built-in
dashboards and profile as a starting point; `elamond values` lists the values this computer has now.

Ids — of dashboards, actions, links, profiles, templates, widgets and action parameters — are 1 to
32 characters `a-z`, `0-9`, `_` and `-`, starting with a letter or a digit. Ids with a dot are the
service's own (built-in actions, dashboards made by `for_each`).

## Top level

| key | type | default | |
|---|---|---|---|
| `listen` | list of addresses and interface names | `["::"]` | where the service listens ([guide](guide.md#where-the-service-listens)) |
| `port` | integer | 47823 | |
| `names` | list of host names | `[]` | names offered in the pairing link after the addresses |
| `name` | string | the host name | the computer's name in the app |
| `mdns` | bool | `true` | announce the service on the local network |
| `interval_ms` | integer, 250–10 000 | 1000 | the service's tick: how often it reads the computer |
| `history_minutes` | integer | 10 | how far back graphs reach |
| `control` | bool | `false` | let paired clients act on the computer, as far as their roles allow |
| `default_role` | `glance` \| `monitor` \| `operate` \| `admin` | `admin` | the role of a client paired without `--role` |
| `default_profile` | profile id | `default` | the profile of a client paired without `--profile`, and of one whose profile is gone |
| `links` | table of [links](#links) | | |
| `actions` | table of [actions](#actions) | | |
| `profiles` | table of [profiles](#profiles) | | |
| `dashboards` | table of [dashboards](#dashboards) | | |
| `templates` | table of [templates](#templates) | | |

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
| `{ range = [0, 100], step = 5, unit = "%", default = 50 }` | `range` | a number within the range, on a step from its start; an integer when `step` and the start are |
| `{ type = "bool", default = false }` | `bool` | `true` or `false` |

Each may have a `label` (what a form calls it; the parameter's id by default). A parameter
without `default` must be given.

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

Actions the settings add are of kind `actions`. The kind is what a profile's `unlock.for` names.
Every action also needs control: `control = true`, and no `elamond control off`.

## Profiles

How a client looks and behaves. The app saves none of this: it takes its profile from the computer
with the dashboards. Each paired client has one (`elamond pair --profile ID`,
`elamond clients profile ID PROFILE`); one without is in `default_profile`.

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
| `unlock.for` | list of kinds | `["power", "signal", "actions", "units"]` | action kinds the client unlocks the phone for first: `power`, `signal`, `actions`, `units`, `profile`, `media` |
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
(`open`), or the chips under the computer's name (a profile's `header`). The standard ones are
`overview`, `processes`, `sensors` and `control`, the `header`, and the pages their tiles open:
`cpu`, `memory`, `gpu.<key>`, `network`, `storage`, `battery`, `activity`.

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
a row.

Every widget takes:

| key | type | default | |
|---|---|---|---|
| `type` | one of the types below | required (or from `use`) | |
| `id` | id | its place in the list, from 0 | unique within the dashboard (or group) |
| `span` | 1–4 | 2, or 4 for `graph`, `table`, `list`, `group`, `media` and `text` | |
| `label` | string | | its title |
| `icon` | [icon](#icons) | | |
| `color` | [colour](#colours) | | its accent |
| `role` | role | the least role its values and action need | the least role that sees it |
| `when` | address | | shown only while that value exists on this computer |
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
| `graph` | lines over time, the newest values above them | `values` (list of addresses), `labels` (one per value), `colors` (one per value), `range` (seconds, at most `history_minutes`; all of it by default), `height` (`small`, `medium`, `large`), `min`, `max` (a fixed scale, a number or an address; it grows to fit otherwise), `floor` (the least the scale goes down to), `empty` (shown while the first value is missing) |
| `list` | rows of a label and a value | `items`: list of `{ label, value, text, format, unit, map, empty, when }`: `value` an address, or `text` a string that holds values (each may `for_each`); a row with nothing to show is left out |
| `bars` | one bar per item of a collection | `source`, `field`, `max` (number or address), `labels` (the field under each bar; none by default) |
| `table` | rows from a [collection](#collections) | `source`, `columns`, `sort`, `sorts`, `search`, `limit`, `filter`, `hide_empty`, `row_open`, `row_actions`, `expand` (below) |
| `button` | runs an action, or opens a dashboard | `action`, `params` (fixed values), `open` |
| `toggle` | a switch | `value` (the state now), `action`, `param` (the parameter it sets), `on`, `off` (its values; `true`/`false` by default) |
| `select` | one of an action's choices | `value` (the choice now), `action`, `param` |
| `slider` | a number in an action's range | `value`, `action`, `param` |
| `text` | words | `text` (may hold values), `level` |
| `group` | a card around widgets | `widgets`, `collapsed` (whether it starts closed; the client remembers it after); with `for_each`, its widgets are repeated for each item inside the one card, their ids `<id>.<item key>` |
| `media` | the players, with covers and seeking, and the volume | |

A `table`:

| key | type | |
|---|---|---|
| `source` | collection | required |
| `columns` | list of `{ field, label, format, unit, bar, grow, map }` | `bar = { max = … }` draws the cell as a bar, full at a number, at a field of the row (`"crit"`), or at `"top"`, a quarter above the largest in the column; `grow` takes the room left; a column without `label` has no heading |
| `sort` | `{ field, desc }` | the order rows come in |
| `sorts` | list of fields | other orders a client may ask for |
| `search` | bool | a search field, over the text columns (the client's own) |
| `limit` | integer, at most 10 000 | rows sent at most (500) |
| `filter` | table, field → value | only rows whose fields have those values (a string, number or bool) |
| `hide_empty` | bool | hidden while it has no rows |
| `row_open` | field | a row opens that field's URL when tapped (`links`) |
| `row_actions` | list of `{ action, label, params }` | `params` values may be `"{field}"`, filled from the row |
| `expand` | list of `{ field, label, format, unit, map }` | what a row shows when opened |

A widget whose action the client's role may not run, or the computer can't do, is left out; with
control off they stay, disabled.

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
| `battery.percent` | below 30 | below 15 | 0 |
| `processes` rows by `cpu` (a quarter of the CPU is a lot already) | above 18.75 | above 22.5 | 0 |
| anything else | — | — | |

`soft`: the colour starts turning toward `warning` that far before it. Between `warning` and
`critical`, `blend` is how far across it is. A widget's own `warning_above`… replace the address's
limits one by one; an address given there is read now, in the client's units.

What has a level: a number with limits, judged by them; a number without is `normal`, but a
duration, a moment or a text has none. A widget with `levels` gives each mapped value its level
there, `normal` for the rest.

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
