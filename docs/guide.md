# Elamon user guide

> **A personal project.** Elamon is made for its author's own use, on NixOS, and grows with what
> that needs. It isn't tied to one computer, though: every change is built and tested
> on NixOS and on Debian, Ubuntu, Fedora, Arch and openSUSE Tumbleweed, so it should work for you
> too. This guide is all the support there is; settings and the protocol may still change between
> versions, so update the app and the service together.

Elamon lets you watch and control a Linux computer from an Android phone. It has two parts:

- **The app**, on an Android phone (Android 8 or newer).
- **elamond**, the Elamon service, on the computer. It reads the computer's own counters and serves
  them over HTTPS, only to phones you paired with it. It runs as your user and needs no root.

The phone talks to the computer directly: on the same network, or over a VPN both are on
(Tailscale, WireGuard and the like). Nothing goes through a server.

## Quick start

1. **Install the service.** On NixOS, turn on the module ([section 1](#nixos-and-home-manager)).
   Elsewhere, as yourself, run
   `curl -fsSL https://raw.githubusercontent.com/Elaizes/elamon/main/install.sh | sh`
   ([section 1](#with-one-command)).
2. **Set it up.** Off NixOS, the install ends with `elamond setup`: run as root the commands it
   prints.
3. **Pair the phone.** Run `elamond pair`, scan the QR code with the phone, and answer `y` on the
   computer ([section 3](#3-pair-the-phone)).
4. **Watch.** The app shows the computer's load, temperatures, memory, processes and more.
5. **Control, if you want it.** Set `control = true` in `~/.config/elamon/elamond.toml`. The phone
   can then use power, media, links and your own actions ([section 5](#5-the-control-panel)).

## Contents

- [1. Install the service](#1-install-the-service)
- [2. Open the firewall](#2-open-the-firewall)
- [3. Pair the phone](#3-pair-the-phone)
- [4. The app](#4-the-app)
- [5. The control panel](#5-the-control-panel)
- [6. Roles: who may do what](#6-roles-who-may-do-what)
- [7. UPSes and power stations](#7-upses-and-power-stations)
- [8. Your own readings](#8-your-own-readings)
- [9. The journal](#9-the-journal)
- [10. Events and notifications](#10-events-and-notifications)
- [11. Settings reference](#11-settings-reference)
- [12. Updating, moving and removing](#12-updating-moving-and-removing)
- [13. Troubleshooting](#13-troubleshooting)
- [14. Security](#14-security)

## 1. Install the service

On NixOS, the module (or Home Manager's) does it all: first below. On any other distribution
that runs systemd, one command does it, the same way everywhere: Debian, Ubuntu, Fedora, Arch and
openSUSE Tumbleweed are the ones it is tested on with every change, and others with systemd
should work alike.

### NixOS and Home Manager

Add the flake to your inputs:

```nix
inputs.elamon = {
  url = "github:Elaizes/elamon";
  inputs.nixpkgs.follows = "nixpkgs";
};
```

**NixOS.** Import the module, turn the service on, then run `sudo nixos-rebuild switch`:

```nix
imports = [ inputs.elamon.nixosModules.default ];

services.elamond = {
  enable = true;        # runs for everyone who logs in
  openFirewall = true;  # TCP 47823, and UDP 5353 for discovery
};
users.users.YOUR_USER.linger = true;  # keeps it running while you're logged out
```

The polkit rule comes from `services.elamond.control`
([section 5](#power-the-profile-and-system-units-polkit)). All options:
[section 11](#nixos-module).

**Home Manager**, for one user: `imports = [ inputs.elamon.homeModules.default ];` and
`services.elamond.enable = true;`. The firewall stays the system's job. On NixOS,
`openFirewall` works without `enable`. On another distribution, `elamond setup` leaves Home
Manager's unit alone and prints what needs root.

### With one command

1. **Install it**, as yourself, not as root:

   ```sh
   curl -fsSL https://raw.githubusercontent.com/Elaizes/elamon/main/install.sh | sh
   ```

   It takes the newest release for this computer (`x86_64` for most PCs, `aarch64` for ARM boards;
   glibc 2.28 or newer: Debian 10, Ubuntu 20.04, RHEL 8 and later), checks its SHA-256, puts
   `elamond` in `~/.local/bin`, and runs `elamond setup` (step 2):

   ```
   Downloading elamond 0.6.2 for x86_64
     OK  SHA-256 matches
     OK  installed ~/.local/bin/elamond (elamond 0.6.2)
         release notes: https://github.com/Elaizes/elamon/releases/tag/v0.6.2
   ```

   To read the script before it runs, download it and run it yourself:

   ```sh
   curl -fsSLO https://raw.githubusercontent.com/Elaizes/elamon/main/install.sh
   less install.sh
   sh install.sh
   ```

   | Option | |
   |---|---|
   | `--version X.Y.Z` | that release instead of the newest |
   | `--no-setup` | only put `elamond` in place; run `~/.local/bin/elamond setup` yourself |
   | `--force` | install even when `elamond` is there already (otherwise it says so and points to `elamond update`) |
   | `--no-start`, `--print`, `--multiple-sessions` | passed on to `elamond setup` |
   | `ELAMON_BIN_DIR=DIR` | another folder than `~/.local/bin` |

   Piped, options go after `sh -s --`: `curl -fsSL … | sh -s -- --no-setup`. It needs `tar`,
   `gzip`, `sha256sum`, `awk`, and `curl` or `wget`, and names whichever is missing. It refuses
   root, musl, an older glibc and other architectures: build from source there
   ([below](#building-from-source)).

2. **`elamond setup`**, which the install ran, installs a systemd user unit
   (`~/.config/systemd/user/elamond.service`) that runs the binary. Where there are no settings, it
   writes those to start from: `~/.config/elamon/elamond.toml` and `elamond.d/dashboards.toml`
   ([section 11](#where-the-settings-live)). Then it starts the service.

   You can run it again at any time. It changes only what changed, and restarts the service when
   the binary was moved or updated. `--print` shows what it would do without doing it;
   `--no-start` leaves the service stopped.

3. **Run what it prints as root.** There are up to three steps:

   | Command | What it's for | Printed |
   |---|---|---|
   | `sudo loginctl enable-linger YOUR_USER` | keeps the service running while you're logged out | until lingering is on |
   | the firewall's commands | let phones reach the service ([section 2](#2-open-the-firewall)) | every time, for the firewall that runs |
   | `sudo install -m 0644 … /etc/polkit-1/rules.d/50-elamond.rules` | lets phones use power, the power profile and system units ([section 5](#power-the-profile-and-system-units-polkit)) | until the installed rule matches the settings |

4. **Pair the phone** with `elamond pair` ([section 3](#3-pair-the-phone)).

### Check that it works

`elamond setup --check` prints a line for each thing the service needs or uses:

| Mark | Meaning |
|---|---|
| `OK` | it works |
| `!` | something is wrong; the line says what to do |
| `?` | it can't tell, often because seeing it needs root |
| `-` | not on this computer, and not needed |

It exits with 1 when the service can't run or can't be reached.

### Good to know

- If your shell doesn't find `elamond`, `~/.local/bin` isn't on your `PATH`. Call it as
  `~/.local/bin/elamond`.
- `elamond setup` doesn't run as root: the service runs as your user, and setup prints the few
  commands that need root.
- It knows only systemd for now. Under another init system, have that run `elamond serve` as your
  user at login; nothing else is needed.
- On NixOS it only points at the module's options. Where the unit comes from Home Manager or a Nix
  profile, it leaves the unit and the settings alone and prints only what needs root.
- `elamond --version` prints the version.

### By hand, from a release

The same without the script: take the archive and its `.sha256` for your architecture from the
[releases](https://github.com/Elaizes/elamon/releases), then:

```sh
sha256sum -c elamond-VERSION-x86_64-linux.tar.gz.sha256
tar -xzf elamond-VERSION-x86_64-linux.tar.gz
install -Dm755 elamond-VERSION-x86_64-linux/elamond ~/.local/bin/elamond
~/.local/bin/elamond setup
```

### Building from source

You need git, a C compiler and Rust 1.89 or newer.

| System | Build tools |
|---|---|
| Fedora | `sudo dnf install git cargo gcc` |
| Arch | `sudo pacman -S --needed git rust base-devel` |
| openSUSE | `sudo zypper install git cargo gcc` |
| Debian, Ubuntu | `sudo apt install git build-essential curl`, then Rust from [rustup](https://rustup.rs): theirs is older than 1.89 |

```sh
git clone https://github.com/Elaizes/elamon.git
cd elamon/elamond
cargo build --release
install -Dm755 target/release/elamond ~/.local/bin/elamond
~/.local/bin/elamond setup
```

### Nix on another distribution

Without Home Manager:

```sh
nix profile install github:Elaizes/elamon
systemctl --user link ~/.nix-profile/lib/systemd/user/elamond.service
systemctl --user enable --now elamond
elamond setup    # prints what needs root: lingering, the firewall, the polkit rule
```

## 2. Open the firewall

| Port | What for |
|---|---|
| **TCP 47823** | the app talks to the service |
| **UDP 5353** | mDNS: the app finds the computer by itself on the local network |

Without UDP 5353, add the computer by its address instead. Over a VPN, only TCP 47823 matters. An
interface the firewall already trusts needs nothing.

`elamond setup` finds the firewall that runs (firewalld, ufw or nftables) and prints the commands
that open both ports, with the port your settings give. With an iptables service, it says what to
open and leaves the rules to you. Arch and Debian ship with no firewall running, so there is
nothing to open. `elamond setup --check` tells whether the ports are open, as far as it can see
without root.

By hand:

| Firewall | Commands |
|---|---|
| NixOS | `services.elamond.openFirewall = true;` |
| firewalld (Fedora, openSUSE) | `sudo firewall-cmd --permanent --add-port=47823/tcp`<br>`sudo firewall-cmd --permanent --add-service=mdns`<br>`sudo firewall-cmd --reload` |
| ufw (Ubuntu, when it's on) | `sudo ufw allow 47823/tcp`<br>`sudo ufw allow 5353/udp` |
| nftables (when its service runs) | `sudo nft insert rule inet filter input tcp dport 47823 accept`<br>`sudo nft insert rule inet filter input udp dport 5353 accept`<br>Add the same rules to `/etc/nftables.conf`, or they're gone after a reboot. |

**Only on some interfaces.** The commands open the ports for the whole computer. To open them on
some interfaces only, adapt them by hand, and name those interfaces in `listen`
([section 11](#where-the-service-listens)). On NixOS,
`services.elamond.firewallInterfaces = [ "eno1" "tailscale0" ];` does it.

## 3. Pair the phone

1. On the computer, run:

   ```sh
   elamond pair
   ```

   It prints a QR code, a six-digit code that works for five minutes, and the computer's
   certificate digits (`Certificate:`).

2. On the phone, do one of these:

   - **Scan the QR code** with the camera and open the link. Pairing then takes one tap.
   - **Enter the code.** In the app, tap the computer under *Found on this network*, or tap *Add
     computer* and type its address or host name. The app first shows the certificate digits:
     check that they match the `Certificate:` line. Then enter the code.

3. Back on the computer, `elamond pair` asks:

   ```
     Phone "Pixel 9" from 192.0.2.23 asks to pair as admin. Allow? [y/N]
   ```

   Answer `y` if it's your phone. Meanwhile the app says to confirm on the computer. Keep
   `elamond pair` running until the phone is paired: any other answer, no answer within 90
   seconds, or stopping `elamond pair` turns the phone away, and the code stops working.

**Roles.** Each phone gets its own key and a **role** that says what it may see and do. By default
it's `admin`, which allows everything. For someone else's phone, give less, for example
`elamond pair --role glance` ([section 6](#6-roles-who-may-do-what)).

**Its own profile.** Pairing adds a profile for the phone to `~/.config/elamon/elamond.d/phones.toml`,
which starts from `default` (`elamond pair --profile NAME` names another): change that phone's tabs
and looks there ([section 11](#each-phones-own-profile)). Pairing the same phone again keeps it.

**Managing phones.** `elamond clients` lists the paired phones with their IDs, roles and profiles.
`elamond revoke ID` unpairs one. Up to 64 phones can be paired with one computer.

**Details:**

- *Wrong codes.* Each wrong try is answered after a second. Twenty wrong tries in all use the code
  up.
- *Scripts.* `elamond pair --yes` pairs whichever phone gives the code, without asking. Use it
  where nobody can answer, such as under systemd or cron. Without `--yes` and with no terminal to
  ask on, `elamond pair` says so and stops.
- *What the QR code carries.* The computer's addresses (those the service listens on, the local
  network first), then names that stay when addresses change (those in `names` and its Tailscale
  name, [section 11](#names-in-the-pairing-link)), and its certificate. The app keeps them all, and
  later tries the one that worked last first.
- *Addresses after pairing.* The computer tells a paired phone where it is now, whenever it connects:
  its addresses on the local network and on VPNs, and its names. So a phone paired by code over
  Wi-Fi also finds it over Tailscale later. When the phone's network changes, the app tries all the
  addresses at once.

## 4. The app

### Home

Each paired computer is a card. The card shows the first three figures of the phone's first tab:
with the standard tabs, CPU, memory, then the first graphics card (or the temperature without
one). They refresh every few seconds while the screen is open. The card also shows the computer's
state:

| State | Meaning |
|---|---|
| *Online* | it answers |
| *Offline* | it doesn't answer: off, asleep, the service stopped, or another network |
| *Not paired* | the computer no longer accepts this phone, for example after `elamond revoke` |
| *Needs updating* | the app and the service are from versions that don't work together |

Below the cards, *Found on this network* lists nearby computers that run the service and aren't
paired with this phone. **Add computer** pairs another one. **?** opens the help, the gear the
settings. With no computer paired yet, the home screen shows the steps to set one up.

### A computer

The top shows the computer's name and how fresh the data is (*Live · every 1 s*). Chips below show
its system, uptime, power source and power profile, and the phone's role. Scroll, and they move
into the bar.

The tabs along the bottom are the computer's **dashboards**: those the phone's profile lists, as far
as its role may see them ([section 11](#dashboards-and-widgets)). The standard four are below, and a
tab for each UPS or power station the computer has, second after Overview.

**Overview** has tiles for the CPU, memory, each graphics card, temperature, network, storage and
battery. Tap a tile for its page:

| Tile | What its page shows |
|---|---|
| CPU | load chart, each core's load, details, top processes |
| Memory | RAM and swap chart, details, top processes |
| A graphics card | load and temperature charts, details, top processes |
| Temperature | opens the Sensors tab |
| Network | traffic chart, the interfaces |
| Storage | throughput chart, devices, file systems, top processes |
| Battery | power draw chart, details |

**Processes** lists the heaviest processes first.

- Sort with the chips: CPU, Memory, GPU, Disk.
- Search by name, command, user or PID.
- Tap a process for its details: command, user, PID and parent, state, start time, disk and GPU
  use. With control on and the `admin` role, the details also offer **Quit** and **Force kill**.

**Sensors** has a table for each kind of sensor: Processor, Graphics, Storage, Wireless, Battery,
Board and Other. Each row's bar fills toward that sensor's critical limit. Tap a row for its chip
and its *High* and *Critical* limits. A *Fans* table follows.

**Control** acts on the computer; see [section 5](#5-the-control-panel).

**A UPS or a power station**, once the computer's settings have one, has a tab of its own, named by
its model: its status and charge, a timeline of when it was on battery, the power in and out, each
outlet, and its switches and settings ([section 7](#7-upses-and-power-stations)).

**Charts.** Drag a finger across a chart to read the values at that moment and how long ago it was.
Charts are full the moment you open a computer, because the service keeps the last ten minutes
(`history_minutes`), and they reach back past a restart of the service. Where the phone was away
or its screen was off, the line has a gap. A medium or large chart also offers longer ranges,
from 1 hour to all of it: the service keeps what charts show every 10 seconds for a day, every
5 minutes for 30 days and every hour from then on
([section 7](#graphs-that-reach-back-the-long-history)).

**The ⋮ menu** has *Settings*, *Help* and *Forget computer*. Back closes a page first, then the
computer.

**Forget computer** removes it from the phone and asks the computer to unpair the phone too. To
see it again, pair again: `elamond pair` offers the phone its profile back. When the computer can't
be told, the app says so: remove the phone there with `elamond revoke` ([section 13](#13-troubleshooting)).

**Good to know:**

- A discrete graphics card that has powered itself down shows as *Asleep*. The service leaves it
  alone, since reading it would wake it.
- The app streams only while it's on screen, so it costs no battery or data in the background.
- *Nothing to show here* means the phone's role or profile shows none of the computer's dashboards.
  On the computer, `elamond clients role` or `elamond clients profile` changes that.

### Settings

How the app looks and behaves with a computer comes from that computer: from the phone's
**profile** there ([section 11](#profiles)). The Settings screen names the profile each computer
gives this phone and where it is changed: the file and line, for an admin phone, or "Set in Nix".
To change something, change the profile on the computer. Two things are the phone's own, under
**Display**, the same with every computer and on the home screen:

- **Scale**, 80 to 140%: everything in the app, text, spacing, widgets and charts, dialogs and
  menus, over the phone's own display size and font size. On a large scale the dashboards have
  fewer columns: two where they had four, and one on a phone held upright from about 120%.
- **Own theme**, off at first: on, the theme is set here (System, Light or Dark; pure black;
  wallpaper colours on Android 12 and later) and stays so whatever the computers' profiles say. It
  starts as the theme you had. Off, each computer's profile sets it again.

The rest of the screen is help, the source and licences.

The app opens on the computer you opened last; Back goes to the list of computers. A link to pair
opens the pairing instead, and a computer you forget isn't opened again.

### How do I…

- **…see what slows the computer down?** Overview → tap the CPU or Memory tile. The bottom of the
  page lists the processes using the most. Or open Processes, sorted by CPU or memory.
- **…close a program that hangs?** Processes → find it → tap → *Quit*. If it doesn't go, *Force
  kill*. This needs control, and a phone paired as `admin`.
- **…check how hot it runs?** Overview → tap the Temperature tile, or open Sensors for every sensor
  and the fans.
- **…open a web page the computer hosts?** Control → Links → tap the link. Over a VPN, the link uses
  the VPN address by itself.
- **…restart a web app that stopped answering?** Control → Links → hold the link → *Restart*.
- **…add a link?** On the computer, add a `[links.<id>]` table to the settings, with its systemd
  unit if you want to start and stop it from the phone ([section 5](#links)).
- **…pause the music?** Control → Media: play, pause, skip, seek, and the computer's volume.
- **…make it quieter or faster?** Control → Power profile.
- **…run my own script from the phone?** Add it as an `[actions.<id>]` table on the computer
  ([section 5](#actions)). It shows up as a button under Control → Actions.
- **…see something the standard tabs don't show?** Make a dashboard of your own on the computer and
  add its id to the phone's profile ([section 11](#dashboards-and-widgets)).
- **…turn it off?** Control → Power → *Shut down*. Only someone at the computer can turn it on
  again, unless it wakes on LAN.
- **…see whether the power went out while I was away?** On the station's tab, the *Power*
  timeline shows when it was on battery; *7 days* or *All* over it reach back
  ([section 7](#7-upses-and-power-stations)).
- **…switch an outlet of the station, or change its charge limit?** At the bottom of its tab,
  *Outlets* and *Station*, once the computer allows it
  ([section 7](#switching-outlets-and-changing-settings)).
- **…watch several computers?** Pair each one. The home screen shows them all.
- **…keep the phone on as a dashboard?** Set `keep_screen_on = true` under
  `[profiles.<id>.android]` on the computer.
- **…let someone watch without touching anything?** Pair their phone with
  `elamond pair --role glance` (the basics) or `--role monitor` (everything to see, nothing to
  change). See [section 6](#6-roles-who-may-do-what).
- **…stop every phone from acting, right now?** Run `elamond control off` on the computer.
  `elamond control on` lets them act again. They still see the computer meanwhile.
- **…cut off a lost or stolen phone?** On the computer, run `elamond clients`, then
  `elamond revoke ID --with-added`.
- **…stop a phone from seeing a computer?** On the phone, ⋮ → *Forget computer*, which unpairs it
  on the computer too. On the computer, `elamond revoke ID`.
- **…move to a new phone?** Pair it with `elamond pair` on each computer
  ([section 12](#a-new-phone)).
- **…reinstall the computer without pairing the phones again?** Run `elamond backup` before and
  `elamond restore` after ([section 12](#a-new-computer-or-reinstalling)).

## 5. The control panel

The **Control** tab acts on the computer. It's off until the computer allows it, so a phone that
can watch can't also act by default.

### Turning it on

Add this to `~/.config/elamon/elamond.toml`:

```toml
control = true
```

The service notices the change within a few seconds by itself, or at once with
`systemctl --user reload elamond`.

On NixOS, set it through the module instead: `services.elamond.settings.control = true;`. The
service then reads its settings from a read-only file the module builds, and a rebuild that changes
them restarts every user's service. Settings you add by hand go in a drop-in beside it,
`~/.config/elamon/elamond.d/*.toml` ([section 11](#11-settings-reference)).

With control on, each phone does as much as its role allows: `admin` (the default) everything below,
other roles less ([section 6](#6-roles-who-may-do-what)). `elamond control off` stops them all at
once, whatever the settings say.

### What's on the Control tab

From top to bottom:

| Section | What it does | What it needs |
|---|---|---|
| **Links** | opens web pages the computer hosts; starts, stops and restarts their units | links in the settings ([below](#links)) |
| **Media** | play, pause, skip and seek any MPRIS player that runs; the computer's volume | a player; `wpctl` or `pactl` for the volume |
| **Power profile** | switches between the profiles of power-profiles-daemon | power-profiles-daemon; the polkit rule |
| **Actions** | a button for each action in the settings | actions in the settings ([below](#actions)) |
| **Power** | *Lock*, *Sleep*, *Restart*, *Shut down* | what logind allows; the polkit rule for all but *Lock* |
| **Displays** | the connected monitors, folded to one line; tap one for its port, graphics card, size, best mode, fastest refresh rate, position and year made | shown even with control off |
| **Journal** | the computer's system journal, warnings first, with search ([section 9](#9-the-journal)) | the `monitor` role or above; for all of it, your user in `systemd-journal` |
| **Activity log** | what phones asked the computer to do ([section 6](#the-activity-log)) | the `monitor` role or above |

The service leaves out what the computer can't do, and what the phone's role may not run. Ending
processes is on the Processes tab.

Each of these is an *action* with an id. There are built-in ones (`power.reboot`, `profile.set`,
`process.signal`, `unit.restart`, `media.play_pause`, `volume.set` and the rest, listed in
[docs/config.md](config.md#built-in-actions)), and those the settings add. The phone asks before an
action that asks to be confirmed, and unlocks first for those its profile says.

### Links

A link opens a web page the computer hosts:

```toml
[links.media]                 # the link's id: a-z, 0-9, _ and -
name = "Media server"
url = "http://{host}:8096"    # {host}: the address the phone reached the computer at
unit = "SERVICE.service"      # optional: lets the app start, stop and restart it
scope = "system"              # or "user" (the default)
icon = "music"                # optional; "globe" by default
```

Each link is a row with its name, its unit's state (*Unit*: `active`, `inactive`, `failed`, or
`missing` when there is no such unit) and *Up* or *Down*: whether something answers on its port.
Tap a row to open the page in the browser. Hold it for *Start*, *Stop* and *Restart*, and for its
address, unit and port. A link with a unit also has a *Journal* row action: that unit's journal
([section 9](#9-the-journal)); it needs the `monitor` role or above.

- `{host}` makes the link work on the local network and over a VPN alike. `localhost` and
  `127.0.0.1` in a URL are replaced the same way.
- A URL starts with `http://` or `https://`, or is a bare host and port (`{host}:3000`), opened as
  `http://`. Any other kind (`javascript:`, `file:`, `intent:`…) is a mistake in the settings.
- Phones with the `monitor` role or above see links. Those with `operate` or `admin` start, stop and
  restart their units.
- A phone can start or stop only the unit a link names. Only service units (`NAME.service`) are
  allowed, each in its own scope, and a template's instance (`podman-kube@….service`) only exactly
  as written. The service never starts or stops its own unit; a link naming it is a mistake.
- System units also need the polkit rule ([below](#power-the-profile-and-system-units-polkit)).

### Actions

An action runs a command of your choice. A phone can only press the button; it never writes the
command.

```toml
[actions.backup]
name = "Back up now"
command = ["/path/to/backup-script", "--now"]   # run without a shell
confirm = true      # ask in the app first (the default)
timeout = 600       # seconds before it's stopped (default 60)

[actions.fan]
name = "Fan"
command = ["/usr/local/bin/fanctl", "set", "{speed}"]
params.speed = { choices = ["quiet", "auto", "full"], labels = { quiet = "Quiet", auto = "Auto", full = "Full" }, default = "auto" }
role = "admin"      # who may run it: "operate" (the default) or "admin"
```

Each action becomes a button under Control → Actions. A dashboard can also put it anywhere: as a
button, or as a `select`, `toggle` or `slider` that sets one of its parameters
([section 11](#dashboards-and-widgets)). What the command prints comes back to the phone: its first
4 KiB and last 12 KiB.

**Parameters.** A phone can only choose among the values a parameter allows:

| Kind | Written as |
|---|---|
| a choice | `choices = ["a", "b"]` |
| a number in a range, on a step | `range = [0, 100]`, `step = 5` |
| yes or no | `type = "bool"` |

A parameter without `default` must be given. An argument that is exactly `"{speed}"` is replaced
by the value as a whole: a value never becomes part of another argument or an option. The service
checks each value before anything runs and refuses one it doesn't allow.

**Paths.** Commands run as your user, with the service's environment. Its `PATH` is the service
unit's and may be shorter than your shell's, so give the program's full path (`/usr/bin/…`; on
NixOS `/run/current-system/sw/bin/…`, `/etc/profiles/per-user/YOUR_USER/bin/…` or
`/run/wrappers/bin/sudo`). On NixOS, the module gives the unit a `PATH` with those directories.

### Power, the profile and system units: polkit

The service runs outside a login session. So for powering off, restarting, sleeping, switching the
power profile and managing **system** units, polkit would ask for a password the service can't
give. A rule lets your user do these without a password. Without it, the phone says *The computer
didn't allow that*. Locking and user units need no rule.

> **Note:** polkit can't tell the Elamon service from other programs you run. What the rule allows
> without a password, it allows every process of your user. Allow only what you want phones to do.

**On most distributions,** `elamond setup` writes the rule. It lets your user power off, restart
and sleep the computer, switch the power profile, and start, stop and restart the system units
that links name (`unit` with `scope = "system"`). It saves the rule as
`~/.local/share/elamon/setup/50-elamond.rules` and prints the command that installs it:

```sh
sudo install -m 0644 ~/.local/share/elamon/setup/50-elamond.rules /etc/polkit-1/rules.d/50-elamond.rules
```

- After changing which system units the links name, run `elamond setup` again and install the new
  rule the same way. `elamond setup --check` says when the installed rule no longer matches.
- While the settings don't load (`elamond check` says why), setup writes no rule, since it can't
  tell which units they name.
- `elamond setup --multiple-sessions` also allows powering off, restarting and sleeping while
  someone else is logged in too, which ends their sessions.

**On NixOS,** the module writes it:

```nix
services.elamond.control = {
  users = [ "YOUR_USER" ];            # power and the profile without a password
  units = [ "SERVICE.service" ];      # system units these users may start, stop and restart
  # multipleSessions = true;          # also while other users are logged in, ending their sessions
};
```

**By hand,** put this in `/etc/polkit-1/rules.d/50-elamond.rules`, with your user name and units:

```js
polkit.addRule(function (action, subject) {
  if (subject.user != "YOUR_USER") return polkit.Result.NOT_HANDLED;
  var power = [
    "org.freedesktop.login1.power-off",
    "org.freedesktop.login1.reboot",
    "org.freedesktop.login1.suspend",
    "org.freedesktop.UPower.PowerProfiles.switch-profile",
    "net.hadess.PowerProfiles.switch-profile"
  ];
  if (power.indexOf(action.id) >= 0) return polkit.Result.YES;
  if (action.id == "org.freedesktop.systemd1.manage-units" &&
      ["SERVICE.service"].indexOf(action.lookup("unit")) >= 0 &&
      ["start", "stop", "restart"].indexOf(action.lookup("verb")) >= 0)
    return polkit.Result.YES;
  return polkit.Result.NOT_HANDLED;
});
```

**Lock** locks every session of your user through logind. Your desktop's locker has to respond to
that. GNOME and KDE do; on wlroots compositors, set it up, for example with hypridle's `lock_cmd`.

## 6. Roles: who may do what

Every paired phone has a role. Each role allows what the one before it does, and more:

| Role | Sees | Does (with control on) |
|---|---|---|
| `glance` | the basics: system, uptime, CPU, memory, graphics cards, temperatures, fans, storage, network, battery, power and the power profile, UPSes and power stations, with their history | nothing |
| `monitor` | everything: also processes with their commands and users, monitors, links, media and the activity log, a UPS's driver and serial numbers | nothing |
| `operate` | everything | links' units, media and the volume, the power profile, the actions from the settings, and a station's commands and settings when its source says `role = "operate"` |
| `admin` | everything | all of that, plus shutting down, restarting, sleeping, locking, ending processes, actions with `role = "admin"`, and a station's commands and settings |

The role filters on the computer. A widget, a table or a whole dashboard that a role may not see
isn't sent to that phone at all, and neither are actions it may not run. A dashboard's or widget's
own `role` in the settings can only ask for more. A phone with no role in the list of paired phones
is `admin`.

### Choosing a role

`elamond pair` pairs as `admin` and says so. Give another role when you pair:

```sh
elamond pair --role glance     # someone else's phone: how the computer is doing, nothing more
elamond pair --role monitor    # sees what runs and what you host, changes nothing
elamond pair --role operate    # everyday control, without power or ending processes
```

To make another role the default, set `default_role = "monitor"` (or any role) in `elamond.toml`.

`elamond clients` lists each phone with its role, its profile, the app it last reported
(`android 0.6.2`) and the address it came from last. To change a role later:

```sh
elamond clients role ID glance                  # ID from elamond clients
elamond clients role all monitor                # every paired phone
elamond clients role ID glance --with-added     # and the phones that got a key with its token
```

It applies at once. The phone's open stream hears of it within a second or two and fetches what it
may see now. A phone turned into `glance`, for example, stops getting processes, monitors and
links.

How a phone looks, within its role, is its profile: `elamond pair --profile` and
`elamond clients profile` set it ([section 11](#each-phones-own-profile)).

### Stopping every phone at once

```sh
elamond control off     # no phone acts on the computer, whatever its role and the settings
elamond control status  # whether control is on, and why (plain `elamond control` does the same)
elamond control on      # back to what the settings say
```

`control off` takes effect at the next request. Phones still see what their roles show. It
survives restarts of the service and of the computer, and a restore, until `elamond control on`.
The settings' `control = true` stays as it was.

A lost or stolen phone still sees everything while control is off. Unpair it, with the keys its
token gave: `elamond revoke ID --with-added` ([section 12](#a-new-phone)).

### The activity log

Everything a phone asks the computer to do is written down, carried out or refused: when, which
phone, what (*power reboot*, *unit restart nginx.service*, *action Fan quiet*…) and how it ended.

- Phones with the `monitor` role or above see it in Control → Activity log.
- It's kept in `~/.local/share/elamon/activity.jsonl` (one JSON object per line, readable only by
  you) and survives restarts.
- It holds up to 500 entries. One phone keeps 100 at most (together with the keys its token gave),
  so a phone that keeps asking pushes out only its own.
- What was carried out is also in the journal: `journalctl --user -u elamond`.

## 7. UPSes and power stations

The service can watch a UPS or a power station beside the computer and, once you allow it, switch
its outlets and change its settings from the phone. It reads it from a NUT server, over NUT's own
network protocol, through upst-core (the core of the upst project), so it needs no NUT client:

- **a UPS** that Network UPS Tools has a driver for, served by NUT's `upsd`;
- **an EcoFlow power station**, served by nutflow, a NUT server of its own that talks to the
  station over Bluetooth.

Each one is a **source** in the settings, `[sources.<id>]`. Its id (`eco` below) names its values
(`eco.battery.charge`) and its tab. A source needs nothing more: the standard settings have a
dashboard for each one, the `default` profile's second tab, after Overview. It shows what the
station has and leaves out what it hasn't, the same for a UPS and an EcoFlow:

- the status and how long it has been so, the charge and the time left;
- a timeline of the power: on line, on battery, or *No data* where nothing is known;
- the power in and out and the solar inputs, the battery (its limits, health and the estimates to
  empty and to full), each outlet with its watts, the voltages, frequencies and temperatures;
- graphs of the charge and the power, reaching back as far as the service has kept
  ([below](#graphs-that-reach-back-the-long-history)), and the model and firmware;
- folded at the bottom, once you allow it, a switch for each outlet and the beeper, and the
  station's settings: the charge and discharge limits, the charging power, X-Boost, timeouts, the
  screen ([below](#switching-outlets-and-changing-settings)).

The service asks the station every second while a phone shows one of its values, and every
10 seconds otherwise, for the history (`interval_ms`, `background_ms`). Every key of a source is in
[docs/config.md](config.md#sources).

### On NixOS, with nutflow

Everything on one computer: nutflow serves the station, `upst record` keeps its journal
([below](#history-from-before-elamond-upst-record)), and elamond shows it. With nutflow's and
upst's flakes among your inputs, beside Elamon's (their documentation gives them), and Elamon's
module on ([section 1](#nixos-and-home-manager)):

```nix
{ inputs, ... }:
let
  # What phones may run on the station: elamond offers these, and nutflow lets its user do them.
  stationCommands = [ "outlet.*.load.*" "beeper.*" ];
  stationSettings = [
    "experimental.battery.charge.max" "experimental.battery.charge.minimum"
    "experimental.charger.realpower.max" "experimental.output.xboost"
  ];
in
{
  imports = [ inputs.nutflow.nixosModules.default inputs.upst.nixosModules.default ];

  services.nutflow = {
    enable = true;
    stations = [ { name = "ups"; kind = "mock"; } ];   # a simulated station, to try it out
    # A real one, over Bluetooth (nutflow's documentation has the rest):
    # stations = [ { name = "ups"; address = "00:00:5E:00:53:01"; } ];
    # userIdFile = "/run/secrets/ecoflow-user-id";
    users.elamon = {
      passwordFile = "/run/secrets/nut-elamon";
      allow = stationCommands ++ stationSettings;
    };
  };
  # hardware.bluetooth.enable = true;                 # for a real station

  services.upst.enable = true;                         # upst record: /run/upst/upst.sock

  services.elamond.settings = {
    control = true;                                    # for the switches (section 5)
    sources.eco = {
      kind = "nut";
      ups = "ups";                                     # its name on nutflow: upsc ups@localhost
      capacity_wh = 2048;                              # the battery's energy, for the estimated draw
      recorder = "/run/upst/upst.sock";                # leave it out without upst record
      user = "elamon";
      password_file = "/run/secrets/elamon-nut";
      commands = stationCommands;
      settings = stationSettings;
      outlets."1".critical = true;                     # the outlet this computer runs from
    };
  };
}
```

The NUT user's password is in two files, outside the Nix store (sops-nix, agenix, or written by
hand), the same password on the first line of each:

| File | Read by | Owner and mode |
|---|---|---|
| `/run/secrets/nut-elamon` | nutflow, which systemd hands it to as it starts | root's alone will do (`0400`) |
| `/run/secrets/elamon-nut` | elamond, as you, each time a phone runs something | yours (`YOUR_USER`), `0600` |

After `sudo nixos-rebuild switch`, the phone has the station's tab. `elamond values eco` lists what
the station has ([below](#what-a-station-has-elamond-values)).

**With upsd instead,** NixOS's `power.ups` runs it. The source's `ups` is the UPS's name there, and
the user is `power.ups.users.elamon`, with `passwordFile`, `instcmds` naming the commands
`commands` allows (or `[ "ALL" ]`), and `actions = [ "SET" ]` for the settings.

### Without Nix

nutflow reads `/etc/nutflow/nutflow.toml`. The station, and a NUT user for elamond:

```toml
[[station]]
name = "ups"
kind = "mock"            # or, for a real one, address = "00:00:5E:00:53:01" and [ble] user_id_file

[nut.users.elamon]
password_file = "/etc/nutflow/elamon.password"   # readable by nutflow's user alone
allow = ["outlet.*.load.*", "beeper.*", "experimental.battery.charge.max", "experimental.output.xboost"]
```

With upsd, the user goes in `upsd.users`, with `instcmds` for the commands (or `ALL`) and
`actions = SET` for the settings.

Then the source, in a drop-in of your own such as `~/.config/elamon/elamond.d/ups.toml`, with
`control = true` in `elamond.toml` for the switches:

```toml
[sources.eco]
kind = "nut"
ups = "ups"                         # its name on the NUT server: upsc ups@localhost
# host = "nas.example.net"          # a NUT server on another computer (port 3493 unless given)
capacity_wh = 2048
recorder = "/run/upst/upst.sock"
user = "elamon"
password_file = "/home/YOUR_USER/.config/elamon/nut.password"
commands = ["outlet.*.load.*", "beeper.*"]
settings = ["experimental.battery.charge.max", "experimental.output.xboost"]
outlets."1".critical = true
```

and the password, the same as nutflow's, in a file only you can read:

```sh
(umask 077; cat > ~/.config/elamon/nut.password)   # type the password, Enter, then Ctrl-D
elamond check                                       # warns of a password file missing or open to others
```

The service takes the source up within seconds. Keep the password file out of `elamond.d/`.

### What a station has: `elamond values`

`elamond values eco` asks the NUT server there and then (5 seconds at most) and prints each of the
source's values as the service names it, with its unit, format and the least role that sees it;
then the station's commands and writable settings, with their values or ranges and whether
`commands` and `settings` allow them:

```
$ elamond values eco
address                                    value    unit  format   role
eco.battery.charge                         99%      %     percent  glance
eco.battery.charger.status                 resting        text     glance
eco.driver.name                            nutflow        text     monitor
eco.experimental.battery.cell.voltage.max  3.3 V    V     number   glance
eco.input.realpower                        206 W    W     watts    glance
eco.outlet.1.realpower                     200 W    W     watts    glance
eco.status                                 On line        text     glance
…

Commands on this station:
  beeper.disable     Turn the beeper off           allowed
  outlet.1.load.off  Turn off AC outlets, group 1  allowed
  …
Writable settings:
  experimental.battery.charge.max  RANGE 50–100            allowed
  experimental.output.memory       ENUM enabled, disabled  not in settings
  …

How long it has been so, the estimates and the timeline come from the running service.
```

`elamond values` alone ends with the sources: `Sources: eco (elamond values <id>)`, of every kind.

**A widget of your own.** The station's tab shows what most stations have. Anything else it lists,
a widget can show: on a page of your own, in a drop-in such as `elamond.d/station.toml`,

```toml
[dashboards.station]
title = "Station"
icon = "battery"

[[dashboards.station.widgets]]
type = "status"
value = "eco.battery.charger.status"
label = "Charger"
map = { charging = "Charging", discharging = "Discharging", resting = "Resting" }

[[dashboards.station.widgets]]
type = "graph"
values = ["eco.experimental.battery.cell.voltage.max", "eco.experimental.battery.cell.voltage.min"]
labels = ["Highest cell", "Lowest cell"]
label = "Cells"
icon = "battery"

[[dashboards.station.widgets]]
type = "timeline"
source = "eco"
label = "Power"
legend = "both"     # On battery · 4% · 12 min
```

with `"station"` in your profile's `dashboards`. A timeline's legend tells how long each state
lasted; `legend = "percent"` tells instead its share of the time anything was known (*No data* left
out of it, so a stopped service doesn't lower the share on battery), and `"both"` tells both, for
whichever range is shown. On the station's tab itself, it goes under
`[dashboards.ups]` in `elamond.d/dashboards.toml`, with `{item}` for the source's id and a `when`,
so a station without it leaves it out (`value = "{item}.battery.charger.status"`,
`when = "{item}.battery.charger.status"`); under Nix, after the standard widgets with `++`
([section 11](#dashboards-and-widgets)).

### Graphs that reach back: the long history

This is for every graph, the computer's as much as a station's. Beside the last ten minutes it keeps
in memory (`history_minutes`), the service keeps on disk every value a graph or a spark of the
settings shows, and every number of each source (256 a source at most; `history = false` on a source
keeps only those its graphs show), each point the mean of the readings in it:

| Every | Kept | For the ranges |
|---|---|---|
| 10 seconds | a day | 1 h, 24 h |
| 5 minutes | 30 days | 7 days, 30 days |
| hour | for good, or `keep` days | Year, All |

A medium or large graph, and a timeline, offers them as chips over it, beside its own range
(*10 min* on most graphs): tap one to see that far back, and drag a finger across it to read a
moment. A small graph never does, and `ranges = false` in a widget's settings leaves them out.

- **It survives restarts.** The service writes it every 5 minutes and as it stops, so a crash loses
  those minutes at most; after a restart, the graphs start from it.
- **Its size.** About 0.3 MB a value the first year and 70 KB a year after: a computer's graphs
  some 4 MB, a power station's ninety-odd numbers some 25 MB the first year and 6 MB a year after.
  `elamond check` says how much there is:

  ```
  history: 12.3 MiB in 274 files (oldest 2026-11-16), not in backups
  ```
- **It isn't in backups.** It's in `~/.local/share/elamon/history/`, which `elamond backup` leaves
  out: copy the folder to keep it. Deleting it loses the long ranges and nothing else.
- **How long the hourly points stay** is the settings' `[history]`:

  ```toml
  [history]
  keep = 365      # days, 30 or more; "all", the default, keeps them for good
  ```

### History from before elamond: `upst record`

On its own, the service knows a station from when it first read it, and the timeline shows
*No data* for the time the service didn't run. `upst record`, of the upst project, runs as a
service of its own and keeps a journal of every change of each UPS it polls, whether elamond runs
or not. With `recorder` naming it, each time the source starts it fills in its history and its
timeline from where it left off (the whole journal the first time), and starts its estimates from
the last hour of it. An outage while the computer was off, or elamond stopped, is then on the
timeline too.

- On NixOS, upst's module runs it, as in the example [above](#on-nixos-with-nutflow):
  `services.upst.enable = true;`. It records every UPS on `localhost`, serves
  `/run/upst/upst.sock`, and keeps a week of journal unless its `settings.record.keep` says more
  (`"365d"`).
- A recorder on another computer is `recorder = "nas.example.net"`, on port 3494 unless given
  (`"nas.example.net:4000"`); one run as a user service serves `/run/user/<your uid>/upst/upst.sock`.
- The journal (`journalctl --user -u elamond`) says what came of it, or, in one line, why not. A
  recorder that can't be reached, or doesn't know the UPS, leaves the source working without it:

  ```
  nut eco: history filled in from the recorder's ups@localhost: all it had before this start
  nut eco: recorder /run/upst/upst.sock: …; no history from before elamond
  ```

### Switching outlets and changing settings

Phones only watch a station until the settings allow more. A phone may switch an outlet or change
a setting when:

1. control is on: `control = true`, and no `elamond control off` ([section 5](#turning-it-on));
2. the source has a NUT user: `user` and `password_file`;
3. its `commands` (the station's commands) or `settings` (its writable variables) list it, as
   patterns where `*` stands for any run of characters (`"outlet.*.load.*"`), and the NUT user may
   do it: nutflow's `allow`, upsd's `instcmds` and `actions = SET`;
4. the station offers it;
5. the phone's role allows it: `admin`, or `operate` too when the source says `role = "operate"`.

What the phone gets is what the station offers, as far as the lists allow (`elamond values eco`
shows both): a switch for each outlet and the beeper; a slider for a setting with a range (the
charge limits, the charging power), on a step of 1 in its unit; a choice for one with a list of
values (X-Boost, the timeouts). A setting of any other kind isn't offered, and the journal says so
once. They come once the NUT server has said what the station has, and stay while it can't be
reached. The limits are the station's: the service checks every value against them and against
the lists before the NUT server hears of it, and a phone sends only the command's name or the
value, never anything else.

**Asking first.** Each asks before it runs, and unlocks the phone first (`nut` in the profile's
`unlock.for`, as the standard profile has it). One that cuts this computer's power has to be
**typed in** on the phone, whatever `confirm_actions` says: on an outlet with `critical = true`,
every command but the one that turns it on (`outlet.1.load.off`); and those upst-core counts as
dangerous that aren't about one outlet (`load.off`, `shutdown.*`, `calibrate.start`…). The phone
asks *Type “outlet.1.load.off” to run it.* The service checks the text, so an app older than 0.7,
which can't ask, can't run them. Turning off another outlet is asked as any action is.

**The station answers.** A source's actions run one at a time. The phone says *done* once the
station says it did it, and otherwise what it said (`invalid value`, `access denied`…), within
25 seconds: a NUT server that can't tell whether the station did it counts as a failure. nutflow
tells, and so does upsd from NUT 2.8 on. The activity log has each (`nut eco outlet.2.load.off`,
`nut eco experimental.battery.charge.max 90`), and `elamond control off` stops them with the rest.

**The password** stays in its file. The service reads its first line each time it runs something,
and gives it only to the NUT server: never to phones, the journal, errors, `elamond values` or
backups. NUT sends it unencrypted, so keep a NUT server on another computer on a network you trust.

### Settings from before 0.7

Settings started before 0.7 have no `ups` dashboard, and a profile that sets its `unlock.for` lacks
`nut` there. Under Nix, the module's defaults have both, unless you set a profile's `dashboards` or
`unlock` yourself. Otherwise, copy the dashboard in:

```sh
elamond defaults dashboards.ups      # into elamond.d/dashboards.toml (or a 0.6.1 elamond.toml)
```

then add `"ups"` to the profile's `dashboards` (second, after `"overview"`, as the standard one
has it), and `"nut"` to its `unlock.for`. `elamond defaults --diff` shows what else changed.

## 8. Your own readings

Besides the computer's own counters, the service can show what a program prints, what a web
service answers or what a file holds, each as a *source*. A source has an id, and its values are
named after it (`up.text`, `app.queue.length`). You write it in the settings, then a widget in a
dashboard of your own shows the value. All of it is in the settings only: a phone never names a
command, a URL or a path.

### Three kinds

```toml
[sources.up]                   # what a program prints
kind = "command"
command = ["uptime", "--pretty"]
format = "text"

[sources.app]                  # what a web service answers
kind = "http"
url = "http://localhost:8080/status"
format = "json"                # the default

[sources.charge]               # what a file holds
kind = "file"
path = "/sys/class/power_supply/BAT0/capacity"
format = "text"
interval = 5                   # seconds between reads (10 by default)
```

- **`command`** runs the program as your user, with nothing on its input. There is no shell: the
  first item is the program and the rest are its arguments, so `|`, `>` and `$VARIABLE` mean
  nothing in it. For a pipe, put it in a script of your own and name the script.
- **`http`** asks the URL with `GET`. A redirect isn't followed.
- **`file`** reads a regular file (`/proc` and `/sys` files are).

`format` says how what it read is taken: `json` (each value in it gets a name, such as
`app.queue.length` for `{"queue": {"length": 3}}`), `text` (all of it, as `charge.text`; a number
when it is one) or `lines` (each `key = value` or `key: value` line).

### See what a source gives

Before writing a widget, ask the service what the source has:

```
$ elamond values charge
```

It reads the source there and then and prints each value with the name a widget uses, or says why
it couldn't. `elamond values` alone ends with `Sources: app, charge, up (elamond values <id>)`.
`elamond check` finds mistakes in the settings (a missing `command`, a bad `url`) without running
anything.

### Show it

A value has no unit and no look of its own: the widget says the rest.

```toml
[dashboards.mine]
title = "Mine"
icon = "gauge"

[[dashboards.mine.widgets]]
type = "value"
value = "charge.text"
unit = "%"
format = "number"
label = "Laptop battery"

[[dashboards.mine.widgets]]
type = "value"
value = "app.queue.length"
label = "Queue"
when = "app.queue.length"
```

Add `"mine"` to your profile's `dashboards` for a tab ([section 11](#dashboards-and-widgets)).
The formats and the other widget types are in [docs/config.md](config.md#widgets).

### When a source is read

A source is read only while it is needed: while a phone shows a widget with one of its values, a
graph or a spark shows it, or `history = true` keeps its numbers for the
[long graphs](#graphs-that-reach-back-the-long-history). With nobody looking and no history, it
isn't read at all, so a slow script costs nothing. At the start it is read once, then every
`interval` seconds. A read that takes more than `timeout` (5 seconds) fails, and a command is
ended then, whatever it started with it.

### Keys and secrets

A web service that wants a token reads it from a file of yours:

```toml
[sources.app]
kind = "http"
url = "http://localhost:8080/status"
token_file = "/home/YOUR_USER/.config/elamon/app.token"   # first line: the token
```

- Make it readable by you alone: `chmod 600`. A token file others can read fails the read, and
  `elamond check` warns of it.
- Never put a user or a password in the URL (`http://user:pass@…`): it is a mistake, and the
  token has to come from `token_file`. It goes only in the request's header, to no phone, and not
  into the journal, `elamond values`, `elamond check` or a backup. Another header than
  `Authorization` is `header = "X-Api-Key"`.
- Keep the token file out of `elamond.d/`, where `.toml` files are read as settings.

### When it fails

A source that can't be read has two more values:

- **`<id>.error`** says why (`timed out after 5 s`, `exited with 7: …`, `HTTP 404`, `not JSON: …`).
  Show it in a `text` widget with a `when`, and it appears only while something is wrong.
- **`<id>.age`** is how many seconds ago the last good read was. It keeps growing while the source
  fails, so a widget with `unit = "s"`, `format = "duration"` shows how stale the numbers are.

The values of the last good read go while a read fails. The widgets that show them say `—` once
the source has had the value, or are left out when it never had it. The journal
(`journalctl --user -u elamond`) says when a source starts to fail and when it reads again, once
each.

A dashboard whose widgets all have a `when` on values of one source stays hidden until that
source has read well once: with nothing to show, the phone gets no tab for it. Keep one widget
without a `when` in it, such as a `text` of fixed words, and the tab is always there.

## 9. The journal

The Control tab has a **Journal** button. It opens the computer's system journal: the lines
systemd collects from the kernel and every service, the newest at the bottom.

- It starts with warnings and above; when the newest lines hold only a few, the page reaches
  back for older ones at once. The chips along the top (*Debug*, *Info*, *Notice*, *Warning*,
  *Error*) pick the least level shown (a log that starts at `critical` has a *Critical* chip too).
- The search field filters by text, unit and program name. The service searches the whole journal
  you can reach, not only the lines on the screen.
- **Load older** at the top goes further back. New lines arrive while the page is open, and
  *Newest* takes you back down to them after you have scrolled up.
- Tap a line to show all of it; hold it to copy it.

**One service's journal.** A link with a unit (see [Links](#links)) has a *Journal* row
action: hold its row in Links and pick *Journal* to see that unit's lines alone.

**As a tab.** Add `"journal"` to a profile's `dashboards` and the journal is a tab of its own
([section 11](#profiles)); the button on Control then opens the same page.

**Who sees it.** The `monitor` role and above. A phone with `glance` has no journal button, and
an app older than 0.8 shows a page that asks to update.

### Reading the system journal

The service reads the journal with `journalctl` as your user, and your user may read only its own
journal unless it is in a group the system's allows. `elamond setup --check` says which it is:

```
  OK  journal: the system journal is readable
  !   journal: only this user's own journal: add the user to systemd-journal …
```

In the second case the page says so under a note, and shows only your user's lines. Add the user
to the `systemd-journal` group and log in again (then restart the service):

```sh
sudo usermod -aG systemd-journal YOUR_USER
```

On NixOS:

```nix
users.users.YOUR_USER.extraGroups = [ "systemd-journal" ];
```

Members of `wheel` and `adm` can usually read it too, on systems whose journal grants them access.

### A log in your own dashboards

The `log` widget draws the same page's lines anywhere:

```toml
[[dashboards.mine.widgets]]
type = "log"
scope = "unit"
unit = "example.service"   # the unit of one of your links
level = "notice"
lines = 100
label = "Example"
```

| key | |
|---|---|
| `scope` | `system` (the default), `user` (your user's own journal) or `unit` |
| `unit` | with `scope = "unit"`: the `unit` of one of your [links](#links); no other is allowed |
| `level` | the least level shown at first: `debug`, `info` (the default), `notice`, `warning`, `error`, `critical`; the phone may pick another |
| `lines` | how many lines the phone keeps on screen, 10 to 1000 (200) |
| `search` | `false` leaves out the search field |

The journal is read only while a phone shows a log. Settings made before 0.8 have no *Journal*
button or row action: `elamond defaults dashboards.journal` and `dashboards.unit_log` give the
tables to copy ([docs/config.md](config.md#the-journal-log)).

## 10. Events and notifications

The service can tell you when something goes wrong, whether or not the app is open: a disk
nearly full, a unit that failed, the laptop's battery running out, a UPS gone to battery. It
works in three parts, all of them settings on the computer:

- **Rules** watch the computer's values and what a UPS says. A rule that turns makes an **event**,
  once: when the trouble begins, and again when it ends.
- Events are **kept** on the computer and **shown** in the app: the Control tab's **Events**
  button opens them, a list like the journal's with the same level chips, search and *Load older*.
- **Targets** send events on to [ntfy](https://ntfy.sh), and the ntfy app on the phone shows
  them as notifications. Elamon itself sends none and asks for no permission.

Nothing is built in: the rules you start with are in the settings like everything else, and you
change, turn off or remove any of them ([docs/config.md](config.md#events-and-notifications) has
every key).

### Getting notifications on the phone

1. Install the **ntfy** app on the phone (Play Store, F-Droid or its releases). In its settings
   for the topic, or in Android's battery settings for ntfy, let it run unrestricted, or
   notifications come late.
2. Think of a long random topic, like a password (`openssl rand -hex 16` makes one), and subscribe
   to it in the app. On the public server, the topic is all that keeps others from reading or
   sending under it.
3. On the computer, put the topic in a file only you can read, and name the file in the settings:

   ```sh
   umask 077; echo 'the-topic-you-made-up' > ~/.config/elamon/ntfy.topic
   ```

   ```toml
   # ~/.config/elamon/elamond.d/notify.toml
   [notify.phone]
   topic_file = "/home/you/.config/elamon/ntfy.topic"
   ```

   On NixOS: `services.elamond.settings.notify.phone.topic_file = "/home/you/.config/elamon/ntfy.topic";`
   (the file itself stays out of the Nix store).
4. Try it: `elamond notify --level warning "Hello from the computer"`. `elamond check` says
   `Events: 7 rules on; sent on to phone.`, and `elamond events` lists what was told, each with
   how sending it went.

**What the server sees.** A notification passes through the ntfy server in the clear, so a target
starts with `names = false`: it sends `A disk is 93% full`, `A unit failed`, `The UPS is on
battery`, under the title `Elamon`, without the disk's, the unit's or the computer's name. The
app's Events page always has the names. Set `names = true` to send them too, or run an ntfy
server of your own and point `url` at it (`token_file` for one that wants a token).

A target sends events from `warning` up, and the end of a trouble it told of. More than 20 an
hour are told as one (`… and 7 more events`); with the network or the server down, an event waits
and is tried again for half an hour. All of these are keys of the target.

### The rules you start with

| rule | tells | on |
|---|---|---|
| `disk` | a filesystem above 90% for a minute; calm again at 88% | yes |
| `cpu-temp` | the CPU as hot as its sensor calls critical, for 30 seconds | yes |
| `unit` | a link's unit that failed ([Links](#links)) | yes |
| `battery` | the laptop's battery below 15% and not charging | yes |
| `ups-battery`, `ups-low`, `ups-link` | a UPS on battery and the power back (with how long it was gone), its battery low, upsd not answering for a minute | yes |
| `gpu-temp` | a GPU as hot as critical | no: reading an NVIDIA card's values wakes it, every half minute |
| `power-saver` | the power profile turned to power-saver | no |
| `boot` | the computer started | no |

A new install has them in `~/.config/elamon/elamond.d/events.toml`; under Nix they are the
module's defaults. Settings from before 0.9 have none: `elamond defaults rules` and `elamond
defaults events` print them to copy, with `elamond defaults dashboards.events` and the Events
button of `dashboards.control`.

Change one by naming its key, in `elamond.d/events.toml` itself or in a file of elamond.d/ whose
name comes after it (files are read by name, the later one winning; the same lines in elamond.toml
would lose to events.toml):

```toml
# ~/.config/elamon/elamond.d/mine.toml
[rules.disk]
above = 85          # sooner
level = "critical"

[rules.gpu-temp]
enabled = true

[rules.boot]
enabled = true
```

```nix
services.elamond.settings.rules.disk.above = 85;
```

### Rules of your own

A rule looks at any value the computer has (`elamond values` lists them), a source's among them
([section 8](#8-your-own-readings)):

```toml
[sources.queue]
kind = "command"
command = ["queue-status", "--json"]
interval = 60

[rules.backlog]
value = "queue.length"
above = 100
for = 300                                  # for five minutes
text = "The queue on {computer} is {value} long"
plain = "The queue is {value} long"        # what a target without names sends
```

- The test is one of `above`, `below`, `equals`, `not`, or `when = "warning"` / `"critical"`: the
  level the dashboard already colours the value by, so the limit is in one place.
- `for` is how long the trouble must last, `clear_for` how long the calm; `clear` how far back
  past the limit a number must go. `again` keeps a thing that flaps quiet for a while.
- A `*` makes it a rule of each thing: `disk.*.percent`, `link.*.unit_state`; `{name}` is the
  thing's key.
- A source a rule looks at is read at its own `interval`, whether or not a phone shows it; a UPS
  a rule hears is asked on the same way.
- For a UPS: `kind = "ups"` with `codes`, what upst-core calls what happened (`power.lost`,
  `battery.low`, `load.overload`…), and `ends`, what ends it.

For anything a rule can't say, a script can: **`elamond notify "text"`** makes an event of its
words (`--level warning`, `--title Backup`), kept, shown and sent like a rule's. It goes as it is,
names and all, to every target.

```sh
borg create … || elamond notify --level critical --title Backup "The backup failed"
```

**Who sees events.** The `monitor` role and above (`[events] role`). An app of 0.8 shows the
Events page too; an older one asks to update.

## 11. Settings reference

Everything a phone shows comes from the computer it is paired with. The app has no editor and
keeps no display settings of its own: it draws what the service describes. Three things decide
what that is:

| Piece | Answers | Lives in | Changed with |
|---|---|---|---|
| **Dashboards** | *what* can be shown: pages of widgets | the settings | your editor, `elamond edit` |
| **Profiles** | *which* dashboards a phone has as tabs, and *how* the app looks and behaves | the settings; each phone's own in `elamond.d/phones.toml` | your editor, `elamond edit`, `elamond clients profile` |
| **Roles** | what a phone is *allowed* to see and do | the list of paired phones, in `~/.local/share/elamon` | `elamond pair --role`, `elamond clients role` |

The role always wins. A profile can put the Control tab on a friend's phone, but if the phone's
role is `monitor`, the service sends it no buttons that act on the computer. So a mistake in a
profile can make a phone look wrong, but never lets it do more than its role allows.

This section is how the pieces fit together. **Every key, widget and value is in
[docs/config.md](config.md).**

### Where the settings live

| File in `~/.config/elamon` | What is in it | Who writes it |
|---|---|---|
| `elamond.toml` | the top-level keys, each with a line on what it does; links, actions, the `default` profile and your shared ones | `elamond setup` or the service's first start, once; then you |
| `elamond.d/dashboards.toml` | the standard dashboards and the templates they use | the same, once; then you |
| `elamond.d/*.toml` | your own additions, one topic per file if you like | you |
| `elamond.d/phones.toml` | a small profile for each paired phone | the service adds and takes out tables as phones pair and unpair; you edit them |
| `defaults.toml`, `elamond-schema.json` | the settings this version of elamond starts with, and their JSON Schema | the service, at each new version; **not read as settings** |

- **The files are all there is.** Nothing is built in under them: delete the `sensors` dashboard,
  and it's gone.
- **Reading order.** `elamond.toml` first, then the files in `elamond.d/` by name. A table with an
  id (`[dashboards.overview]`, `[profiles.pixel-9]`, `[links.media]`) that comes again later is
  merged into the earlier one key by key. A list (`widgets`, a profile's `dashboards`, `listen`)
  replaces the earlier one whole.
- **For editors.** The two starter files begin with `#:schema elamond-schema.json`
  (`../elamond-schema.json` in `elamond.d/`). Editors that read it (taplo, Even Better TOML)
  suggest keys and mark mistakes.
- **NixOS and Home Manager.** `services.elamond.settings` takes the place of both starter files.
  The module holds the standard setup as defaults, and what you set replaces just that key
  ([below](#nixos-module)). `elamond.d/` stays yours, `phones.toml` with it, so each phone has its
  own profile under Nix too.

The top-level keys (`control`, `listen`, `port`, `names`, `default_role`…) are in
[docs/config.md](config.md#top-level). The commands:

```sh
elamond check              # read the settings as the service would; names the file and line of each mistake
elamond check FILE         # the same, with FILE in place of elamond.toml, before you put it there
elamond edit               # edit elamond.toml, checked before it's saved
elamond edit ~/.config/elamon/elamond.d/dashboards.toml   # or a drop-in
elamond values             # every value this computer has now, by address, and the collections
```

`elamond edit` opens `$VISUAL`, else `$EDITOR`, else `vi`. An edit with a mistake doesn't replace
your settings: it's kept beside them as `FILE.broken`, and you can open the editor again.

**Mistakes.** A file with a mistake doesn't take the running service down. It keeps the settings
it read before, but control is off until they read again, so a half-finished edit can't leave
phones acting on old permissions. `elamond control status` shows the mistake, `elamond check` all
of them. At start, settings with a mistake stop the service.

**Secrets.** A secret in the settings, such as a link's URL with a password in it or a token among
an action's arguments, belongs in a file only you can read: `chmod 600` it. Never put one in
`services.elamond.settings`, which goes into the Nix store where every user can read it. Use a
drop-in instead.

### Dashboards and widgets

A dashboard is a page of widgets in a grid of four columns (two on a narrow screen), filled in the
order they are listed. It is a tab when a profile lists it, a page a widget opens (`open`), or the
chips under the computer's name (a profile's `header`). The standard ones are in
`elamond.d/dashboards.toml`: `overview`, `ups` (one for each UPS source), `processes`, `sensors`,
`control`, the `header`, and the pages their tiles open.

A dashboard of your own, in a drop-in such as `elamond.d/power.toml`:

```toml
[dashboards.power]
title = "Power"
icon = "battery"

[[dashboards.power.widgets]]
type = "gauge"
value = "battery.percent"       # an address: elamond values lists this computer's
label = "Battery"

[[dashboards.power.widgets]]
type = "select"                 # one of an action's choices
action = "fan"
param = "speed"
label = "Fan"
```

Then put `"power"` in a profile's `dashboards`.

- **A dashboard is shared.** Every phone whose profile lists it sees the same widgets, minus what
  its role leaves out. For another layout on one phone, make another dashboard
  (`[dashboards.overview-wall]`) and give it to that phone's profile.
- **Change a standard dashboard where it is,** in `dashboards.toml` (or in a 0.6.1 `elamond.toml`).
  Adding, moving or deleting a widget is an edit there.
- **In a later file or in Nix, a dashboard's `widgets` is written whole.** It replaces the list
  before; it doesn't add to it. Start from a copy: `elamond defaults dashboards.overview` prints
  it. In Nix, build on the original:

  ```nix
  services.elamond.settings.dashboards.overview.widgets =
    config.services.elamond.defaults.dashboards.overview.widgets
    ++ [ { type = "value"; value = "system.uptime"; label = "Uptime"; } ];
  ```

Widget types, tables, values in text, repeats (`for_each`), templates, formats, icons and colours
are in [docs/config.md](config.md#the-journal-log).

#### One dashboard per file

```
~/.config/elamon/elamond.d/
├── dashboards.toml   # the standard ones: change them here
├── power.toml        # [dashboards.power]
├── wall.toml         # [dashboards.overview-wall], for a phone used as a wall display
└── phones.toml       # each phone's profile, written by the service
```

- Keep each dashboard in **one** file: a later file's `widgets` replaces the earlier list.
- Files are read by name. One that changes a key of a standard dashboard has to come after
  `dashboards.toml`.
- Templates work across files: everything is merged before it is checked.

With Home Manager, the standard dashboards are in the module's settings. Files of your own can live
in your configuration, linked in one by one:

```nix
xdg.configFile."elamon/elamond.d/power.toml".source = ./elamon/power.toml;
```

- Link files, not the whole `elamond.d` folder: the service has to write `phones.toml` there.
- Linked files are read-only, and `elamond edit` refuses them. They change with
  `home-manager switch`, and the service, not the build, checks them.

### Profiles

A profile says which dashboards a phone has as tabs (`dashboards`), which chips sit under the
computer's name (`header`), and how the app looks and behaves: its theme, animations, how often it
asks for readings, its units, which actions it confirms, and before which it asks to unlock the
phone. `[profiles.<id>.android]` holds what only the app does, such as `keep_screen_on`. Every key
is in [docs/config.md](config.md#profiles).

`use` means "start from that profile; the keys here win". Shared profiles go in `elamond.toml` (or
Nix), each phone's own in `phones.toml`:

```toml
# elamond.toml: profiles shared by several phones
[profiles.owner]
use = "default"
pure_black = true
dashboards = ["overview", "power", "processes", "sensors", "control"]

[profiles.guest]
use = "default"
dashboards = ["overview", "sensors"]

# elamond.d/phones.toml: one table per phone
[profiles.pixel-9]                # your phone
name = "Pixel 9"
use = "owner"
theme = "dark"                    # this phone only

[profiles.friends-phone]          # someone else's
name = "Friend's phone"
use = "guest"
```

The service follows `use` to the end of the chain; the closest key wins (`android` too, key by
key). A look you want on several phones, or want to keep in your dotfiles, goes in a shared
profile. Only what belongs to one phone goes in its table: phone tables come and go with pairings.

#### Each phone's own profile

When a phone pairs, the service adds a table for it to `elamond.d/phones.toml`, named after the
phone. The line above it says which phone has it, with the ID `elamond clients` shows:

```toml
# Galaxy S25, paired 2026-11-12 (0123abcd)
[profiles.galaxy-s25]
name = "Galaxy S25"
use = "default"
```

- Its id comes from the phone's name (`Galaxy S25` → `galaxy-s25`; a second one of that name gets
  `galaxy-s25-2`).
- **To change one phone,** add keys to its table: `theme = "light"`, `dashboards = [...]`. The phone
  follows within seconds, with its screen open. To change every phone, change the profile they use.
- The service only adds tables, takes them out and writes the lines above them. Your keys and
  comments stay.

| When | Its table |
|---|---|
| `elamond pair` | a new one, using `default_profile` |
| `elamond pair --profile owner` | a new one, with `use = "owner"` |
| `elamond pair --profile galaxy-s25`, a table no paired phone has | the phone takes that table as it is |
| the same phone pairs again (a new QR code, another address), its key still good | kept: the app proves it's the same phone, and the old entry goes. `elamond pair` says so when it asks |
| a phone pairs whose key is gone (the phone was reset, or unpaired), and a phone of its name left a table | `elamond pair` asks: *A phone named "Galaxy S25" had the profile galaxy-s25. Give it to this one?* (not with `--yes` or `--profile`) |
| `elamond revoke` | taken out if it's as pairing wrote it and nothing else uses it; otherwise kept, with an `# Unpaired` note |
| *Forget computer* in the app | always kept, with a note: `# Unpaired 2026-11-15; kept for a phone paired again (elamond pair offers it to one named "Galaxy S25").` |

`elamond check` names each table no paired phone has: when it was unpaired, and that `elamond pair`
offers it to a phone of that name, or that you can take it out. The exact rules are in
[docs/config.md](config.md#phones).

#### A shared profile instead: `elamond clients profile`

| | The phone gets | Its own table |
|---|---|---|
| `use = "guest"` in its table | `guest`, then its own keys over it | applies |
| `elamond clients profile ID guest` | `guest`, as it is | doesn't apply, but stays: `elamond clients profile ID friends-phone` gives it back |

Use `use` to change what one phone starts from, and `elamond clients profile` to put several
phones on one profile at once (`all`, `--with-added`). `elamond clients` shows each phone's
profile, with `(shared)` after one that isn't its own table.

In the app, Settings names the profile each computer gives the phone and where to change it
([section 4](#settings)).

### Roles are separate

A phone's role ([section 6](#6-roles-who-may-do-what)) is kept with the paired phones, not in a
profile, and a profile can't raise it. A dashboard's or widget's own `role` can only ask for more.
Unlocking the phone before an action (`unlock` in a profile) is the app's doing: it protects a phone
left lying around, but a copied key works without it.

### Keeping up with new versions

The service never rewrites your settings. Each new version writes its own `defaults.toml` and keeps
the one before as `defaults.toml.old`, so you can see what changed:

```sh
elamond defaults                         # the settings elamond starts with
elamond defaults --diff                  # where yours differ from them, and what the update changed in them
elamond defaults dashboards.sensors      # one table, to copy into your settings
```

Take what you like from an update by copying it in, and leave the rest. What `--diff` prints is in
[docs/config.md](config.md#the-defaults-beside-defaultstoml). Under Nix, the module's defaults
follow the version by themselves; only what you set in `settings` stays as it was.

### When changes apply

- Dashboards, profiles (phone tables too), links, actions, sources, `[history]`, `control`,
  `default_role`, `default_profile` and `names`: as soon as the service rereads the files. It does when one of them
  changes, or at once on `systemctl --user reload elamond`. An open phone fetches its new tabs by
  itself.
- Roles (`elamond clients role`) and `elamond clients profile`: at once.
- `port`, `listen`, `name`, `mdns`, `interval_ms` and `history_minutes`: after a restart of the
  service.
- With Nix: after the rebuild (`nixos-rebuild switch`, `home-manager switch`), which also checks
  the settings and fails on a mistake.

### Recipes

- **Pure black on my phone only:** in `phones.toml`, under your phone's table, `pure_black = true`.
- **Pure black on all my phones:** the same key in a shared `[profiles.owner]`, and `use = "owner"`
  in each phone's table (or `elamond pair --profile owner` when pairing).
- **Friends see only Overview and Sensors:** `[profiles.guest]` with
  `dashboards = ["overview", "sensors"]`, and their phones paired with
  `elamond pair --profile guest --role glance`. The role is what keeps them from acting.
- **A widget on Overview for everyone:** `elamond edit ~/.config/elamon/elamond.d/dashboards.toml`,
  and add it where it should sit under `[dashboards.overview]`. Under Nix, the `++` above.
- **An old phone as a wall display:** a dashboard `[dashboards.overview-wall]` with big widgets. In
  that phone's table, `dashboards = ["overview-wall"]`, and under `[profiles.<its id>.android]`,
  `keep_screen_on = true`.
- **Buttons for my own scripts:** actions in `[actions.<id>]` ([section 5](#actions)), a dashboard
  with `button`, `select` or `toggle` widgets for them, and its id in your profile's `dashboards`.
- **I broke something:** `elamond check` says where. `elamond defaults TABLE` prints the original of
  anything standard.

### Limits

- **Settings belong to a computer, not to a phone.** A phone paired with three computers has a table
  on each, and may look different on each. To look the same everywhere, give the computers the same
  shared profile (one Nix module for all of them does it), and `use` it in the phone's tables; or,
  for the theme alone, turn on *Own theme* in the app's settings. The app's scale is the phone's
  too.
- **A dashboard's widgets are one list.** A later file or Nix can't change one widget of a dashboard
  by its `id`: it gives the whole list.
- **No profile switching by situation** (other tabs over a VPN, at night…).
- **No editing on the phone.** One place to change things, one place to back up. The scale and the
  app's own theme are the exceptions: they are how the phone shows things, and stay on it.

### Where the service listens

By default the service listens on every address, IPv4 and IPv6. `listen` narrows that to addresses
and network interfaces, as one entry or a list:

```toml
listen = ["tailscale0"]                    # over Tailscale only
listen = ["eno1", "tailscale0"]            # the cable and Tailscale, not Wi-Fi or containers
listen = ["192.0.2.10", "fd7a:115c::1"]    # these addresses
```

- An interface stands for the addresses it has. One that has none yet, like a VPN that comes up
  after the service, is listened on within seconds of getting one. An address that goes away is let
  go; connections on it stay until they end. The journal says each. The same goes for an address
  the computer doesn't have yet.
- `"0.0.0.0"` is every IPv4 address.
- A drop-in's `listen` replaces the one before rather than adding to it.
- `elamond pair` puts the addresses the service listens on in its QR code, the local network first,
  and looks for the running service at one of them.
- Discovery (`mdns`) announces the service only on the wired and wireless interfaces among them,
  never over a VPN or on a bridge for containers. Over Tailscale alone, add the computer by
  address, name or QR code.

**Where the service keeps its state:** `~/.local/share/elamon`. That's its id and certificate, the
paired phones (only hashes of their keys, their roles and profiles, and the app each last
reported), a pending pairing code, the activity log, `elamond control off`'s mark, and the graphs'
long history (`history/`). Deleting that folder unpairs every phone and gives the service a new
certificate, unless a backup of it is restored ([section 12](#backing-up-the-service)).

**Environment variables.** The folders follow `XDG_CONFIG_HOME` and `XDG_DATA_HOME`.

| Variable | Names |
|---|---|
| `ELAMON_CONFIG_DIR` | the settings folder |
| `ELAMON_DATA_DIR` | the state folder |
| `ELAMON_CONFIG` | the settings file alone |
| `ELAMON_PCI_IDS` | the PCI database graphics card names come from |
| `ELAMON_NVML_LIB` | NVIDIA's management library, when it isn't found by itself |

### Names in the pairing link

Addresses change with networks and leases; a name stays. After its addresses, `elamond pair` puts
host names in the QR code, which the app keeps and tries as well:

1. those you give in `names`, in their order:

   ```toml
   names = ["deskpc.example.net", "deskpc.lan"]
   ```

2. then the computer's Tailscale name (like `deskpc.tail1234.ts.net`), when Tailscale runs with
   MagicDNS on and the service listens on the computer's Tailscale address (`listen` is `"::"` or
   names `tailscale0`). `elamond pair` asks `tailscale status` for it, and goes on without it when
   Tailscale isn't there or doesn't answer within a second and a half.

- The computer's `.local` name isn't added by itself, as Android looks such names up poorly. Put it
  in `names` if your phone finds it.
- `elamond pair` prints the names on a `Names:` line.
- The QR code carries 8 addresses and names at most, addresses first. What doesn't fit is printed as
  left out.
- A name that leads to another computer does no harm: the app accepts only the computer with the id
  and certificate it paired with.
- A drop-in's `names` replaces the one before.

### NixOS module

| Option | |
|---|---|
| `services.elamond.enable` | run the service for the people who log in (never for system users such as a display manager's greeter) |
| `services.elamond.users` | only these users run it; empty means every regular user. One user's service holds the port at a time |
| `services.elamond.package` | the service's package |
| `services.elamond.settings` | the settings of every user's service, as Nix (below), the defaults in them already: set only what differs ([docs/config.md](config.md#nix)). `elamond pair` and the other commands read them too. A mistake fails `nixos-rebuild` with its line; a change restarts the services. They're in the Nix store, readable by every user: no secrets. A user's own `~/.config/elamon/elamond.toml` is read right after them, then the drop-ins in `elamond.d` |
| `services.elamond.defaults` | read-only: the settings elamond starts with, as `settings` holds them before yours |
| `services.elamond.openFirewall` | open the service's port (`settings.port`, 47823 by default) and UDP 5353 (unless `settings.mdns = false`); works without `enable` |
| `services.elamond.firewallInterfaces` | with `openFirewall`, open them on these interfaces only, and UDP 5353 only on those not named like a VPN (`tailscale*`, `wg*`, `nordlynx*`, `tun*`, `tap*`, `zt*`, `ppp*`; a guess by the name). Empty means every interface |
| `services.elamond.control.users` | users who may use power and the profile without a password, so their service can. polkit allows it every program of theirs, not only the service |
| `services.elamond.control.units` | system units those users may start, stop and restart without a password. Links in `settings` with `scope = "system"` are in the rule already; list here the units of links in a user's drop-in |
| `services.elamond.control.multipleSessions` | `false` by default; also power off, restart and sleep while other users are logged in |

The module's user unit has a `PATH` with `/run/wrappers/bin` (sudo, pkexec), your profile
(`/etc/profiles/per-user/YOUR_USER/bin`, `~/.nix-profile/bin`) and the system's
(`/run/current-system/sw/bin`), for `wpctl` or `pactl` and for actions.

The settings in Nix are the TOML of [docs/config.md](config.md) as attribute sets.
`[dashboards.power]` becomes `dashboards.power`, and `[[dashboards.power.widgets]]` a list of
attribute sets:

```nix
services.elamond.settings = {
  control = true;
  links.jellyfin = { name = "Jellyfin"; url = "http://{host}:8096"; unit = "jellyfin.service"; scope = "system"; };
  actions.fan = {
    name = "Fan";
    command = [ "${pkgs.coreutils}/bin/echo" "fan" "{speed}" ];
    params.speed = { choices = [ "quiet" "fast" ]; default = "quiet"; };
  };
  dashboards.power = {
    title = "Power";
    widgets = [
      { type = "gauge"; value = "battery.percent"; label = "Battery"; }
      { type = "select"; action = "fan"; param = "speed"; label = "Fan"; }
    ];
  };
  profiles.owner = { theme = "dark"; dashboards = [ "overview" "power" "processes" "control" ]; };
};
```

### Home Manager module

`inputs.elamon.homeModules.default` (`homeManagerModules.default` is the same):

| Option | |
|---|---|
| `services.elamond.enable` | run the service for this user |
| `services.elamond.package` | the service's package |
| `services.elamond.settings` | written to `~/.config/elamon/elamond.toml` (a link into the Nix store), as for NixOS. A mistake fails `home-manager switch`; a change restarts the service. Drop-ins in `elamond.d` still add to it. A file of your own in its place stops `home-manager switch`: move it into `elamond.d/`. No secrets |
| `services.elamond.defaults` | read-only: the settings elamond starts with |

The firewall stays the system's. If you set your own `port` here, give the system the same one
(`services.elamond.settings.port` beside `openFirewall`; without `enable`, that only opens the
port), or open it yourself.

## 12. Updating, moving and removing

### Updating

| Installed from | To update |
|---|---|
| a release (`install.sh`, or by hand) | `elamond update` (below) |
| source | `git pull`, `cargo build --release` in `elamond/`, install the binary again, then run `elamond setup` |
| a Nix profile | `nix profile upgrade --all`, then `systemctl --user daemon-reload` and `systemctl --user restart elamond` |
| NixOS or Home Manager | `nix flake update elamon`, then rebuild; the rebuild restarts the service |

**`elamond update`** looks for a newer release, only when you run it:

```sh
elamond update --check     # only tell whether there is one
elamond update             # show it, ask, install it and start the service again on it
elamond update --yes       # the same without asking
```

```
elamond 0.6.2 is out; this is 0.6.1.
  Release notes: https://github.com/Elaizes/elamon/releases/tag/v0.6.2
Install it over ~/.local/bin/elamond? [y/N] y
  OK  downloaded elamond-0.6.2-x86_64-linux.tar.gz (4.0 MB); its SHA-256 matches
  OK  installed elamond 0.6.2 at ~/.local/bin/elamond
  OK  started the service again on it
```

- It asks GitHub for the list of releases, anonymously, and takes the newest one with a build for
  this computer. It checks the archive's SHA-256 and that the new binary says it's that version,
  then puts it in place of the old one in one step. Anything wrong leaves the old one as it was.
- A new minor version (0.6 → 0.7) gets a warning: update the app with it.
- It restarts the service only when it runs and runs this binary; otherwise it says what to do.
- It refuses a binary from the Nix store (update it through Nix, as in the table), one in a folder
  you can't write, and root.
- A release whose notes say the unit changed needs `elamond setup` after the update.
- GitHub answers 60 such requests an hour to one address without signing in; past that, it says
  to try again later.

**Update the app and the service together.** Each asks for the other to be updated when they don't
match. Pairings stay: the phones keep their keys, roles and profiles and don't pair again.

**From 0.6.1 to 0.6.2,** the settings and pairings carry over as they are. Elamon 0.6.2 updates only
from 0.6.1: from an older version, update to 0.6.1 first, as its guide says, since 0.6.2 no longer
moves the settings of 0.6.0 and before. What changed:

- A 0.6.1 `elamond.toml` stays as it is. Only a new start gets the short `elamond.toml` and
  `elamond.d/dashboards.toml` ([section 11](#where-the-settings-live)). Under Nix, nothing changes.
- Backups are no longer encrypted. Make a new one after the update: 0.6.2 doesn't read those 0.6.1
  encrypted ([below](#backing-up-the-service)).
- After `elamond rotate-cert`, phones no longer follow the new certificate: each pairs again
  ([section 14](#a-new-certificate)). `--no-statement` is still accepted and changes nothing.
- The app no longer exports and imports pairings: a new phone pairs with `elamond pair`
  ([below](#a-new-phone)).
- *Forget computer* unpairs the phone on the computer too. With a service older than 0.6.2, or one
  it can't reach, the app says to run `elamond revoke` there.

### Backing up the service

What makes the service the same computer to its phones is in `~/.local/share/elamon`: its id, its
certificate and key, the paired phones, the activity log and `elamond control off`'s mark. Lose it,
and every phone has to pair again. Keep a copy:

```sh
elamond backup ~/elamond.backup
```

- It writes one file that only you can read (`0600`). It isn't encrypted: whoever reads it can pose
  as this computer to its phones. Keep it out of shared and cloud folders.
- The backup also carries the settings, `elamond.toml` and the drop-ins in `elamond.d`. A restore
  puts them back only when asked.
- It leaves out the charts' long history (`history/` in the same folder): it is large, and only
  the long ranges of the charts need it. Copy the folder to keep it.
- It asks nothing, so a script or a timer runs it as it is. `--plain`, which elamond 0.6.1 wanted
  for that, is still accepted and changes nothing.
- elamond 0.6.2 doesn't read the backups that 0.6.1 and older encrypted with a passphrase (those
  made without `--plain`). Restore one of those with elamond 0.6.1, or make a new backup.

A backup holds the certificate's key as it was then. After `elamond rotate-cert`, older backups
still hold the old key: delete them ([section 14](#a-new-certificate)).

### A new computer, or reinstalling

Back up first. On the new or reinstalled system, install the service
([section 1](#1-install-the-service)), then:

```sh
systemctl --user stop elamond
elamond restore ~/elamond.backup
systemctl --user start elamond
```

The phones connect as before, without pairing again: the service has its old id and certificate and
knows their keys. They look for it at the addresses and names they already know, so give the new
computer the old one's address (a DHCP reservation does it) or keep a name they know: its Tailscale
name, or one in `names` ([section 11](#names-in-the-pairing-link)). A phone that can't reach it any
other way has to pair again.

What `elamond restore` does:

- It checks everything in the backup before it writes anything. It refuses a damaged file, a backup
  encrypted by elamond 0.6.1 or older, a backup from a newer elamond (update the service first) and,
  unless told `--force`, a running service.
- It writes each file in one step, and removes the state files the backup doesn't have. It keeps no
  copy of what it replaces: restore from the backup again if a write fails.
- It prints what it restored: the computer's id and certificate and the phones.
- `--with-settings` also puts back `elamond.toml` and the drop-ins in `elamond.d`, but never over a
  file NixOS or Home Manager make: set those in `services.elamond.settings`.
- On a computer that has phones paired already, a restore would bring back the backup's phones that
  aren't paired there now, including any you unpaired since, lost or stolen ones too. It lists them
  and stops. `--with-old-phones` restores them anyway.
- Control held off by `elamond control off` stays off, whatever the backup says.

`elamond restore --force` writes under a running service. That service presents the restored
certificate within a few seconds and knows the restored phones at once, but keeps the id and
activity log it started with until it restarts. Restart it right after:
`systemctl --user restart elamond`.

### A new phone

Pair it as you did the first: run `elamond pair` on each computer and scan its QR code with the new
phone ([section 3](#3-pair-the-phone)).

Moving for good: first forget the computers on the old phone. That unpairs it on each computer it
reaches and keeps its profile there, which `elamond pair` then offers to a new phone of the same
name. Where it couldn't, or the old phone is reset already, revoke its entry on the computer:

```sh
elamond clients
elamond revoke OLD_ID
```

**Families of keys.** A phone that imported pairings with Elamon 0.6.1 got a key of its own on each
computer, with the old phone's token. To the computer the two are a family, which shares the
activity log. `elamond revoke` of the old one leaves the new one paired and names it loudly: a phone
you don't know there got its key from that one. `elamond revoke OLD_ID --with-added` unpairs the
whole family, even by an ID already gone. That's the way to cut off a lost or stolen phone and
whatever got a key with its token.

A phone that imported pairings with 0.6.1 but never connected since still holds the old phone's
token, and the computer can't tell the two apart: *Forget computer* on either unpairs both. Pair
such a phone with `elamond pair` before forgetting the computer on the old one.

### Removing the service

1. `systemctl --user disable --now elamond`.
2. Delete the binary and the unit file (`~/.config/systemd/user/elamond.service`).
3. Delete `~/.config/elamon` and `~/.local/share/elamon`.
4. As root: delete `/etc/polkit-1/rules.d/50-elamond.rules`, close the ports you opened
   ([section 2](#2-open-the-firewall)), and, if nothing else needs it,
   `sudo loginctl disable-linger YOUR_USER`.

In the app, ⋮ → *Forget computer*. With the service gone it can't tell the computer, and there is
nothing left to revoke.

## 13. Troubleshooting

First, run `elamond setup --check`. It prints a line for each thing the service needs, with what to
do about those marked `!`. To see what the service reads without the app, run `elamond snapshot`.

| What happens | What to do |
|---|---|
| The computer isn't under *Found on this network* | Same network, and UDP 5353 open on the computer. VPNs and many guest Wi-Fi networks block discovery, and the service announces itself only on wired and wireless interfaces it listens on (`mdns: announced on …` in the journal). Add it by address or name instead: `elamond pair` prints them. |
| The app can't connect | Check `elamond setup --check`, `systemctl --user status elamond` and `journalctl --user -u elamond -e`. TCP 47823 must be open, and the phone must reach an address the service listens on (`listen: on …` in the journal). |
| It works only while you're logged in | `sudo loginctl enable-linger YOUR_USER`, as `elamond setup` prints it. |
| *The computer's certificate changed* | The service's data was deleted or the system reinstalled. Restore a backup ([section 12](#a-new-computer-or-reinstalling)), or forget the computer and pair again. If neither happened, check the network first. After `elamond rotate-cert` every phone says this: pair it again ([section 14](#a-new-certificate)). |
| *Control is off* | Set `control = true` in `elamond.toml` or a drop-in; the service takes it up within a few seconds. If it's set, `elamond control status` tells whether `elamond control off` holds it off, or whether the settings stopped parsing. |
| Pairing ends with the phone turned away | Answer `y` at `elamond pair` within 90 seconds, and keep it running until the phone is paired ([section 3](#3-pair-the-phone)). |
| *… was forgotten here* after *Forget computer* | The computer couldn't be reached, or its service is older than 0.6.2, so the phone is still paired there. On the computer, `elamond clients` lists the phones, `elamond revoke ID` removes this one. |
| *This phone's role on the computer doesn't allow that* | `elamond clients` shows the phone's role, `elamond clients role ID ROLE` changes it ([section 6](#6-roles-who-may-do-what)). An action may ask for `admin` (`role` in its settings). |
| *Nothing to show here* | The phone's role or profile shows none of the computer's dashboards: `elamond clients role` or `elamond clients profile`. |
| *The computer didn't allow that* | polkit refused. Install the rule `elamond setup` prints; `elamond setup --check` tells whether it's there ([section 5](#power-the-profile-and-system-units-polkit)). |
| The phone keeps dropping off | The journal has a line for each stream a phone opens and closes, with its address, how long it lasted and why it ended: `journalctl --user -u elamond -e`. |
| `elamond setup` says the user manager doesn't answer | Run it in a session of your own, logged in at the computer or over ssh, not through `su` or `sudo`. |
| `elamond setup` says systemd isn't running | Another init system runs the computer: have it run `elamond serve` as your user at login. |
| A link opens but the page doesn't load | What it points to may listen only on 127.0.0.1, which other devices can't reach. Make it listen on all addresses, or reach it through an SSH tunnel. |
| A link's unit shows `missing` | The `unit` name is wrong, or the unit isn't installed: `systemctl list-units` (add `--user` for user units). |
| No temperatures or fans | `sudo sensors-detect` (lm-sensors), then restart the service. |
| A graphics card shows no load | NVIDIA needs its own driver (the service reads its management library; `ELAMON_NVML_LIB` names it if it isn't found). Intel doesn't report load without extra privileges; the app shows the clock. |
| A monitor shows no resolution | It's off or blanked, or the service can't open the graphics device: be logged in at the computer, or in the `video` group. |
| *Lock* does nothing | Your desktop's locker doesn't react to logind's lock signal ([section 5](#power-the-profile-and-system-units-polkit)). |
| No tab for the UPS or power station | Settings started before 0.7 have no `ups` dashboard, or the profile's `dashboards` lacks `"ups"` ([section 7](#settings-from-before-07)). `elamond values` ends with the sources the settings have. |
| A source of your own shows nothing, or `—` | `elamond values ID` says what it reads, or why it can't. A widget shows `ID.error` (with a `when`) while it fails: `HTTP 404`, `timed out after 5 s`, `exited with 7: …`, `token_file may be read by others: chmod 600 it`. A command has no shell, so a pipe belongs in a script of your own; a dashboard of only `when` widgets is hidden until the source has read well once ([section 8](#8-your-own-readings)). |
| The journal shows *only this user's own journal* | Your user isn't in the `systemd-journal` group (`elamond setup --check` says so): add it and log in again ([section 9](#reading-the-system-journal)). A phone with `glance` has no journal. |
| The station says *Unreachable* | The service can't reach the NUT server at the source's `host` and `port`. Is nutflow (or upsd) running: `systemctl status nutflow`? `upsc ups@localhost` reads the station as the service would. The journal says why, in lines that start `nut eco:`: `journalctl --user -u elamond -e`. |
| The station says *No data* | The NUT server answers, but has nothing from the station: nutflow lost it (out of Bluetooth range, or the EcoFlow app on a phone holds the one connection the station takes), or upsd's driver stopped. `journalctl -u nutflow -e`. Meanwhile the tab keeps its shape, with `—` for each value. |
| The station's switches or sliders aren't there | They need `control = true`, the source's `user` and `password_file`, the command or setting in its `commands` or `settings`, a station that offers it, and a phone paired as `admin` (or `operate`, with the source's `role = "operate"`). `elamond values eco` shows what the station offers and which the lists allow (`allowed`, `not in commands`) ([section 7](#switching-outlets-and-changing-settings)). |
| `elamond check` warns of a `password_file` | *can't be read*: the file isn't there, or isn't yours; *may be read by others*: `chmod 600` it; *read as settings*: it's a `.toml` in `elamond.d/`, move it. Until then the station's actions fail. |
| A station's action fails with *access denied* | The NUT user may not do it: its `allow` in nutflow, or `instcmds` and `actions = SET` in upsd's users, has to cover what `commands` and `settings` list. Or the password in the service's file isn't the NUT server's. |
| *The computer asks for this command to be typed in* (`confirm_required`) | The command cuts this computer's power, or the station's whole load, and has to be typed in ([section 7](#switching-outlets-and-changing-settings)). An app older than 0.7 can't ask for it: update the app. |

## 14. Security

**Connection and keys**

- Everything goes over TLS. The service's certificate is self-signed. The app pins it when pairing
  and refuses any other afterwards ([a new certificate](#a-new-certificate) means pairing again).
- Each phone holds its own random 256-bit key; the service keeps only its SHA-256. Keys stay out of
  Android backups.
- A pairing code works for five minutes, one attempt at a time, each wrong one answered after a
  second. Twenty wrong ones, from anywhere, use the code up. The phone that gives
  the right code is let in only when you say so at `elamond pair` (unless it runs with `--yes`).
- Without a key, the service answers only which service it is (`hello`: name, id and version) and
  pairing.
- It drops a connection that says nothing, or says it a byte at a time, within seconds.
- The key is all the computer knows a phone by. Unlocking the phone before an action is enforced by
  the app only: a key copied off a phone works without it. That's why `elamond revoke` matters: it's
  the one thing that stops a key, open streams included. For a lost or stolen phone:
  `elamond revoke ID --with-added`.

**What phones may do**

- Each phone sees and does only what its role allows. Give phones that aren't yours `glance` or
  `monitor` ([section 6](#6-roles-who-may-do-what)).
- Unless `control = true`, and no `elamond control off`, the service doesn't act on the computer:
  phones only read.
- With control on, it acts as your user. Commands come only from the settings files, never from a
  phone, and so do the units a phone may start and stop. Links open only web pages.
- A UPS or power station runs only the commands and settings its source lists and the station
  offers, with values within the station's limits, checked by the service. One that cuts this
  computer's power is typed in on the phone, and the service checks the text. The NUT password
  stays in its file: it never reaches a phone, the journal or a backup.
- A source runs only the command, asks only the URL and reads only the file its settings name, as your
  user; a phone never writes one. A token for a web service stays in its file (`chmod 600`) and goes
  only in the request to that service: to no phone, the journal or a backup. The journal on the
  phone is for `monitor` and above, and a phone never names a unit in it: only a link's.
- It never signals itself, its parents or PID 1. With the start time the app sends, it never signals
  a process that took the PID of the one meant.
- Settings that stop parsing hold control off until they parse again.
- Every request to act is in the activity log, carried out or refused. What was carried out is also
  in the journal, with the name of the phone that asked: `journalctl --user -u elamond`.

**Installing and updating**

- `install.sh` and `elamond update` take releases from GitHub over HTTPS, anonymously, and check
  each archive against the SHA-256 published with it. That catches a damaged or cut-off download,
  not a forged release: whoever could change the release could change its checksum too. Signed
  releases are planned for 1.0. `curl … | sh` also trusts whoever can change `install.sh` on
  `main`; download it and read it first if that matters to you.
- A paired phone learns the computer's addresses and names (`endpoints`), for it to find the
  computer again. Only paired phones get them; whatever answers at one of them is accepted only
  with the paired id and certificate.

**Backups and keys**

- A backup (`elamond backup`) is the computer's identity. It isn't encrypted: it is written for your
  eyes only (`0600`), and is only as safe as the folder you keep it in. Never put it in a shared or
  cloud folder.
- A phone paired with a code may ask the computer once for a second key, for another phone, with
  its own role and no more: app 0.6.1 did so after Import pairings ([section 12](#a-new-phone)).
  `elamond clients` and `elamond revoke` say whose key added one, the activity log has it, and
  `elamond revoke ID --with-added` cuts off both. At most 64 phones are paired at once.
- An old key lives on in backups. `elamond restore` never lets go of control held off, and brings
  back phones unpaired since only with `--with-old-phones`.

### A new certificate

The certificate is good until the year 4096 and pinned by its fingerprint: it needs no renewing.
If its key may have been copied off the computer, give the service a new one:

```sh
elamond rotate-cert   # a new certificate and key; the old key deleted, the id kept
elamond revoke all    # the keys phones sent to a computer that may not have been this one
elamond pair          # each phone again; the app replaces the computer it had
```

- The running service presents the new certificate within a few seconds (at once with
  `systemctl --user reload elamond`). Connections already open keep the old one until they end.
- Every phone says *The computer's certificate changed* until it is paired again.
- The old key lives on in backups made before (with `elamond backup`). Delete them, as
  `elamond rotate-cert` reminds you.
