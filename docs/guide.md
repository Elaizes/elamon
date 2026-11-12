# Elamon user guide

> **Test software in active development.** Features, settings and the protocol between the app
> and the service can change from one version to the next, and things may break. Use it where
> that's fine, and update the app and the service together.

Elamon lets you watch and control a Linux computer from an Android phone. It has two parts:

- **The app**, on an Android phone (Android 8 or newer).
- **elamond**, the Elamon service, on the computer. It reads the computer's own counters and serves
  them over HTTPS, only to phones you paired with it. It runs as your user and needs no root.

The phone talks to the computer directly: on the same network, or over a VPN both are on
(Tailscale, WireGuard and the like). Nothing goes through a server.

## Quick start

1. **Install the service.** Download `elamond` from the
   [latest release](https://github.com/Elaizes/elamon/releases/latest) and put it in
   `~/.local/bin` ([section 1](#1-install-the-service)). On NixOS, use the module instead.
2. **Set it up.** Run `~/.local/bin/elamond setup` as yourself, then run as root the commands it
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
- [7. Settings reference](#7-settings-reference)
- [8. Updating, moving and removing](#8-updating-moving-and-removing)
- [9. Troubleshooting](#9-troubleshooting)
- [10. Security](#10-security)

## 1. Install the service

This works the same way on every distribution that runs systemd: Debian, Ubuntu, Fedora, Arch,
openSUSE and the rest. On NixOS, the module does it ([below](#nixos-and-home-manager)).

### From a release

1. **Download and unpack it.** Take `x86_64` for most PCs and `aarch64` for ARM boards. It runs on
   any distribution with glibc 2.28 or newer (Debian 10, Ubuntu 20.04, RHEL 8 and later).

   ```sh
   sha256sum -c elamond-VERSION-x86_64-linux.tar.gz.sha256
   tar -xzf elamond-VERSION-x86_64-linux.tar.gz
   install -Dm755 elamond-VERSION-x86_64-linux/elamond ~/.local/bin/elamond
   ```

2. **Set it up**, as yourself, not as root:

   ```sh
   ~/.local/bin/elamond setup
   ```

   It installs a systemd user unit (`~/.config/systemd/user/elamond.service`) that runs this
   binary. It writes starter settings to `~/.config/elamon/elamond.toml` if there are none. Then
   it starts the service.

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

### Building from source

You need git, a C compiler and Rust 1.88 or newer.

| System | Build tools |
|---|---|
| Fedora | `sudo dnf install git cargo gcc` |
| Arch | `sudo pacman -S --needed git rust base-devel` |
| openSUSE | `sudo zypper install git cargo gcc` |
| Debian, Ubuntu | `sudo apt install git build-essential curl`, then Rust from [rustup](https://rustup.rs): theirs is older than 1.88 |

```sh
git clone https://github.com/Elaizes/elamon.git
cd elamon/elamond
cargo build --release
install -Dm755 target/release/elamond ~/.local/bin/elamond
~/.local/bin/elamond setup
```

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
[section 7](#nixos-module).

**Home Manager**, for one user: `imports = [ inputs.elamon.homeModules.default ];` and
`services.elamond.enable = true;`. The firewall stays the system's job. On NixOS,
`openFirewall` works without `enable`. On another distribution, `elamond setup` leaves Home
Manager's unit alone and prints what needs root.

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

**Only on some interfaces.** Name them in `listen` ([section 7](#where-the-service-listens)) and
run `elamond setup` again. It then opens the port on those interfaces only (with firewalld, in
their zones). UDP 5353 is opened only on interfaces not named like a VPN, since discovery doesn't
work over a VPN anyway. On NixOS, `services.elamond.firewallInterfaces = [ "eno1" "tailscale0" ];`
does the same.

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
and looks there ([section 7](#each-phones-own-profile)). Pairing the same phone again keeps it.

**Managing phones.** `elamond clients` lists the paired phones with their IDs, roles and profiles.
`elamond revoke ID` unpairs one. Up to 64 phones can be paired with one computer.

**Details:**

- *Wrong codes.* Five wrong tries from one address shut that address out for the rest of the
  code's life. Twenty wrong tries in all use the code up.
- *Scripts.* `elamond pair --yes` pairs whichever phone gives the code, without asking. Use it
  where nobody can answer, such as under systemd or cron. Without `--yes` and with no terminal to
  ask on, `elamond pair` says so and stops.
- *What the QR code carries.* The computer's addresses (those the service listens on, the local
  network first), then names that stay when addresses change (those in `names` and its Tailscale
  name, [section 7](#names-in-the-pairing-link)), and its certificate. The app keeps them all, and
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

**After updating from 0.5.** Once, the home screen shows a card with a profile that gives what was
set on this phone before: theme, update interval, what asked to unlock, and the rest. Copy it into
`~/.config/elamon/elamond.toml` on each computer and give it to this phone there with
`elamond clients profile ID NAME`. Until a computer gives the phone that profile, the app doesn't
ask to unlock the phone there for what it used to.

### A computer

The top shows the computer's name and how fresh the data is (*Live · every 1 s*). Chips below show
its system, uptime, power source and power profile, and the phone's role. Scroll, and they move
into the bar.

The tabs along the bottom are the computer's **dashboards**: those the phone's profile lists, as far
as its role may see them ([section 7](#dashboards-and-widgets)). The standard four are below.

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

**Charts.** Drag a finger across a chart to read the values at that moment and how long ago it was.
Charts are full the moment you open a computer, because the service keeps the last ten minutes
(`history_minutes`). Where the phone was away or its screen was off, the line has a gap.

**The ⋮ menu** has *Settings*, *Help* and *Forget computer*. Back closes a page first, then the
computer.

**Good to know:**

- A discrete graphics card that has powered itself down shows as *Asleep*. The service leaves it
  alone, since reading it would wake it.
- The app streams only while it's on screen, so it costs no battery or data in the background.
- *Nothing to show here* means the phone's role or profile shows none of the computer's dashboards.
  On the computer, `elamond clients role` or `elamond clients profile` changes that.

### Settings

How the app looks and behaves with a computer comes from that computer: from the phone's
**profile** there ([section 7](#profiles)). The Settings screen shows what applies and which profile
key sets it. To change something, change the profile on the computer.

| Setting | Profile key |
|---|---|
| Theme | `theme`: follow the phone (`system`), or always `light` or `dark` |
| Pure black | `pure_black`: black instead of dark grey in dark mode; saves power on OLED screens |
| Wallpaper colours | `android.dynamic_color`: colours from the wallpaper (Android 12+) |
| Blur | `android.blur`: frosted glass behind the bars and dialogs (Android 12+) |
| Animations | `animations`: rolling numbers and moving gauges |
| Update every | `interval_ms`: slower saves data on mobile networks |
| Temperature | `fahrenheit`: °C or °F |
| Network speed | `network_bits`: bytes per second, or bits per second as providers state it |
| Keep screen on | `android.keep_screen_on`: while a computer is open, to use the phone as a dashboard |
| Confirm actions | `confirm_actions`: ask before actions that ask to be confirmed |
| Ask to unlock before | `unlock.for`: the kinds of action the phone is unlocked for first |
| Ask again after | `unlock.grace`: how long one unlock holds |
| Haptic feedback | `android.haptics`: a tick on long presses |

Two things belong to the app itself, for every computer:

- **Export pairings** saves every paired computer to a file encrypted with a passphrase, for a new
  phone ([section 8](#a-new-phone)).
- **Import pairings** adds the computers from such a file.

Both always ask to unlock the phone. On a phone with no screen lock, the app asks you to confirm
instead.

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
  add its id to the phone's profile ([section 7](#dashboards-and-widgets)).
- **…turn it off?** Control → Power → *Shut down*. Only someone at the computer can turn it on
  again, unless it wakes on LAN.
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
- **…stop a phone from seeing a computer?** On the computer, `elamond revoke ID`. On the phone, ⋮ →
  *Forget computer*.
- **…move to a new phone?** Settings → *Export pairings* on the old phone, *Import pairings* on the
  new one ([section 8](#a-new-phone)).
- **…reinstall the computer without pairing the phones again?** Run `elamond backup` before and
  `elamond restore` after ([section 8](#a-new-computer-or-reinstalling)).

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
`~/.config/elamon/elamond.d/*.toml` ([section 7](#7-settings-reference)).

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
address, unit and port.

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

**Links from the 0.5 app.** Links are set on the computer only; the app no longer adds them. When
the 0.6 service first starts, it writes the links the 0.5 app added into
`~/.config/elamon/elamond.d/links-from-app.toml` (or `links-from-app-2.toml` if that exists), and
renames `links.json` to `links.json.migrated`. Change them there. A link keeps its unit where the 0.5
settings named it; any other unit is there commented out. Take the `#` off its `unit` and `scope`
to let phones start and stop it, and run `elamond setup` again for a system unit.

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
([section 7](#dashboards-and-widgets)). What the command prints comes back to the phone: its first
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
| `glance` | the basics: system, uptime, CPU, memory, graphics cards, temperatures, fans, storage, network, battery, power and the power profile, with their history | nothing |
| `monitor` | everything: also processes with their commands and users, monitors, links, media and the activity log | nothing |
| `operate` | everything | links' units, media and the volume, the power profile, the actions from the settings |
| `admin` | everything | all of that, plus shutting down, restarting, sleeping, locking, ending processes, and actions with `role = "admin"` |

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
(`android 0.6.1`) and the address it came from last. To change a role later:

```sh
elamond clients role ID glance                  # ID from elamond clients
elamond clients role all monitor                # every paired phone
elamond clients role ID glance --with-added     # and the phones that got a key with its token
```

It applies at once. The phone's open stream hears of it within a second or two and fetches what it
may see now. A phone turned into `glance`, for example, stops getting processes, monitors and
links.

Each phone has a profile of its own ([section 7](#each-phones-own-profile)), which starts from
another:

```sh
elamond pair --profile owner            # its own profile uses [profiles.owner] from the settings
elamond clients profile ID owner        # give it another later; all and --with-added work as for roles
```

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
token gave: `elamond revoke ID --with-added` ([section 8](#a-new-phone)).

### The activity log

Everything a phone asks the computer to do is written down, carried out or refused: when, which
phone, what (*power reboot*, *unit restart nginx.service*, *action Fan quiet*…) and how it ended.

- Phones with the `monitor` role or above see it in Control → Activity log.
- It's kept in `~/.local/share/elamon/activity.jsonl` (one JSON object per line, readable only by
  you) and survives restarts.
- It holds up to 500 entries. One phone keeps 100 at most (together with the keys its token gave),
  so a phone that keeps asking pushes out only its own.
- What was carried out is also in the journal: `journalctl --user -u elamond`.

**Rate limit.** A phone may ask the computer to do 10 things in a row, then one a second. More are
refused for a moment (*Too many requests*). A phone and the keys its token gave share that rate.
Watching is never limited.

## 7. Settings reference

### `elamond.toml`

The settings are files, and nothing else. The service reads:

1. `~/.config/elamon/elamond.toml`, or the file `ELAMON_CONFIG` names: the whole of the settings;
2. every `~/.config/elamon/elamond.d/*.toml`, in name order. One of them is `phones.toml`, with a
   profile for each paired phone ([below](#each-phones-own-profile)).

**The starter file.** Where there is none, `elamond setup` and the service's first start write
`elamond.toml` with everything elamond starts with: the top-level keys, each with a line on what it
does, two examples, the `default` profile and every standard dashboard. It begins with
`schema = 3`. Change what you like in it. What you take out is gone: a dashboard you delete isn't put
back from somewhere.

A table with an id (`[links.web]`, `[actions.fan]`, `[dashboards.power]`, `[profiles.owner]`) adds
to the one of the same id before it, key by key. A list (`widgets`, `listen`, a profile's
`dashboards`) replaces the one before it whole. So a drop-in can add a dashboard, change one key of
one in `elamond.toml`, or replace its widgets. Under NixOS or Home Manager, `elamond.toml` comes from
Nix and can't be edited, so your own additions go in drop-ins.

**Every key, widget and value is in [docs/config.md](config.md).**

The commands:

```sh
elamond check              # read the settings as the service would; names the file and line of each mistake
elamond check FILE         # the same, with FILE in place of elamond.toml, before you put it there
elamond edit               # edit elamond.toml, checked before it's saved
elamond edit ~/.config/elamon/elamond.d/phones.toml   # or a drop-in
elamond defaults           # the settings elamond starts with, as the starter file has them
elamond defaults --diff    # where yours differ from them, and what changed in them since the last version
elamond defaults dashboards.sensors   # one table of them, to copy into your settings
elamond values             # every value this computer has now, by address, and the collections
elamond schema > ~/.config/elamon/elamond.schema.json   # for editors that check TOML
```

`elamond edit` opens `$VISUAL`, else `$EDITOR`, else `vi`. An edit with a mistake doesn't replace
your settings: it's kept beside them as `elamond.toml.broken`, and you can open the editor again.

**The defaults beside yours.** `~/.config/elamon/defaults.toml` holds the settings elamond starts
with. It's read-only and isn't read as settings. Each new version of elamond writes its own and keeps
the one before as `defaults.toml.old`, so after an update `elamond defaults --diff` shows:

- what in your settings is other than in the defaults, by table: `added`, `changed` (and which keys),
  `removed`;
- what the new version changed in the defaults, such as a dashboard with a new widget. Your settings
  don't change by themselves: copy what you want with `elamond defaults TABLE`.

The top-level keys:

| Key | Default | |
|---|---|---|
| `schema` | | `3`: the file is the whole of the settings. Without it, the file is one of elamond 0.6.0 ([section 8](#updating)) |
| `port` | `47823` | TCP port |
| `listen` | `"::"` | addresses and interfaces to listen on, one or a list; `"::"` is every address, IPv4 and IPv6 ([below](#where-the-service-listens)) |
| `names` | `[]` | host names that reach this computer, added to the pairing link ([below](#names-in-the-pairing-link)) |
| `name` | host name | the name shown in the app |
| `mdns` | `true` | announce the service on the local network: only on wired and wireless interfaces it listens on, never over a VPN |
| `interval_ms` | `1000` | time between readings, 250–10 000 |
| `history_minutes` | `10` | how far back charts reach, 1–1 440 (at most 7 200 readings) |
| `control` | `false` | let phones act on the computer ([section 5](#5-the-control-panel)) |
| `default_role` | `"admin"` | the role `elamond pair` gives when not told ([section 6](#6-roles-who-may-do-what)) |
| `default_profile` | `"default"` | the profile a new phone's own one uses when `elamond pair` isn't given `--profile`, and the profile of a phone whose profile is gone |
| `[links.<id>]` | | `name`, `url`, `unit`, `scope`, `icon` ([section 5](#links)) |
| `[actions.<id>]` | | `name`, `command`, `params`, `role`, `confirm`, `unlock`, `timeout`, `icon` ([section 5](#actions)) |
| `[profiles.<id>]` | | how a phone looks and behaves ([below](#profiles)) |
| `[dashboards.<id>]` | | pages of widgets ([below](#dashboards-and-widgets)) |
| `[templates.<id>]` | | widgets to start others from |

**When changes apply.** The service rereads the files when one of them changes, or on
`systemctl --user reload elamond`.

- Links, actions, profiles, dashboards, `control`, `default_role`, `default_profile` and `names`
  apply at once. A phone's open stream fetches its new dashboards by itself.
- `port`, `listen`, `name`, `mdns`, `interval_ms` and `history_minutes` apply after a restart.

**Mistakes.** A file that doesn't parse, or settings with a mistake, are named in the log with the
file and line, and the settings read before stay. But control is off until they read again, so a
half-finished edit can't leave phones acting on old permissions. `elamond control status` shows the
mistake, `elamond check` all of them. At start, settings with a mistake stop the service.

**From 0.6.0.** A 0.6.0 `elamond.toml` (no `schema` in it) is moved over once, at the first start of
0.6.1 or later ([section 8](#updating)). **From 0.5.** The settings of 0.5 and before had
`[[links]]`, `[[actions]]` and `units`. The service refuses them and says what replaces each.

**Secrets.** A secret in the settings, such as a link's URL with a password in it or a token among
an action's arguments, belongs in a file only you can read: `chmod 600` it. Under NixOS or Home
Manager, never put one in `services.elamond.settings`, which goes into the Nix store where every
user can read it. Use a drop-in in `~/.config/elamon/elamond.d` instead.

### Profiles

A profile says how a phone looks and behaves: its tabs, the chips under the computer's name, its
theme, how often it asks for readings, its units, and which actions it confirms and unlocks for.
The starter file has `default`. A profile can start from another with `use`:

```toml
[profiles.owner]
use = "default"                 # start from it; the keys here win
name = "Owner"
theme = "dark"
pure_black = true
dashboards = ["overview", "power", "processes", "control"]   # the tabs, in order
unlock = { for = ["power", "signal", "units", "actions"], grace = "1m" }

[profiles.owner.android]
keep_screen_on = true
```

Every key is in [docs/config.md](config.md#profiles).

### Each phone's own profile

Every paired phone has a profile of its own, in `~/.config/elamon/elamond.d/phones.toml`. Pairing
adds it:

```toml
# Galaxy S25, paired 2026-11-12 (0123abcd)
[profiles.galaxy-s25]
name = "Galaxy S25"
use = "owner"
```

- Its id comes from the phone's name (`Galaxy S25` → `galaxy-s25`; a second one of that name gets
  `galaxy-s25-2`).
- It `use`s the profile `elamond pair --profile` names, or `default_profile`.
- **To change one phone,** add keys to its table: `theme = "light"`, `dashboards = [...]`. The phone
  follows within seconds, with its screen open. To change every phone, change the profile they use.
- The file is yours to edit like any drop-in (`elamond edit ~/.config/elamon/elamond.d/phones.toml`).
  The service only adds and removes tables there, keeping everything else.

| When | Its profile |
|---|---|
| the phone pairs again (a new QR code, another address), its key still good | the app proves it's the same phone; the new pairing keeps its table, and the old entry goes |
| the phone was reset, or unpaired, and pairs again | `elamond pair` asks: *A phone named "Galaxy S25" had the profile galaxy-s25. Give it to this one?* (`--yes` never gives it) |
| `elamond pair --profile galaxy-s25` | the phone takes that table, as it is |
| `elamond revoke ID` | its table goes if you never changed it; a changed one stays, with a note, for the phone to take when it pairs again |
| a new phone gets a key with Import pairings | a table of its own, using the old phone's |

`elamond clients profile ID owner` still gives a phone another profile, one shared by several
phones say. Its own table stays: `elamond clients profile ID galaxy-s25` gives it back.

Phones paired before 0.6.1 get their tables at the first start of 0.6.1, each using the profile it had.

In the app, Settings shows the phone's profile and where to change it.

### Dashboards and widgets

A dashboard is a page of widgets in a grid of four columns (two on a narrow screen). It can be:

- a tab, when a profile lists it;
- a page a widget opens;
- the chips under the computer's name (a profile's `header`).

The starter file has the standard ones: `overview`, `processes`, `sensors`, `control`, the
`header`, and the pages their tiles open. A dashboard of your own:

```toml
[dashboards.power]
title = "Power"
icon = "battery"

[[dashboards.power.widgets]]
type = "gauge"
value = "battery.percent"       # an address: elamond values lists this computer's
label = "Battery"

[[dashboards.power.widgets]]
type = "graph"
values = ["battery.power"]
labels = ["Draw"]
range = 600                     # seconds

[[dashboards.power.widgets]]
type = "select"                 # one of an action's choices
action = "fan"
param = "speed"
label = "Fan"
```

Then put `"power"` in a profile's `dashboards`.

- **Widget types:** `value`, `gauge`, `meter`, `status`, `graph`, `list`, `bars`, `table`, `button`,
  `toggle`, `select`, `slider`, `text`, `group` and `media`.
- **Tables** show a collection (processes, links, disks, the activity log…): sorted, searched,
  filtered, with what a row shows when opened and what it may run.
- **Text with values:** `caption = "{mem.used:bytes} of {mem.total:bytes}"`.
- **Repeats:** `for_each` repeats a widget or a dashboard for each GPU, disk or interface.
- **Templates:** `[templates.<id>]` and `use` save repeating yourself.
- **Colours and levels** (*warning* above 75 %, …) are decided by the service, the same for every
  phone.

All of it is in [docs/config.md](config.md): widgets, addresses, collections, formats, icons and
colours.

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
certificates before it with their signatures (`rotations.json`, [section 10](#a-new-certificate)),
the paired phones (only hashes of their keys, their roles and profiles, and the app each last
reported), a pending pairing code, the activity log and `elamond control off`'s mark. Deleting that
folder unpairs every phone and gives the service a new certificate, unless a backup of it is
restored ([section 8](#backing-up-the-service)).

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
| `services.elamond.settings` | `elamond.toml` for every user's service, as Nix (below). It holds the defaults already: set only what differs (a key you set wins over theirs; a list, like a dashboard's `widgets`, replaces theirs; a table of your own adds to them). `elamond pair` and the other commands read it too. It's checked as it is built, so a mistake fails `nixos-rebuild` with its line. A change restarts the services. It's in the Nix store, readable by every user: no secrets. A user's own `~/.config/elamon/elamond.toml` isn't read beside it; drop-ins in `elamond.d`, `phones.toml` among them, are |
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
| `services.elamond.settings` | written to `~/.config/elamon/elamond.toml`, as Nix as for NixOS, the defaults in it already. It's checked as it is built, so a mistake fails `home-manager switch`. A change restarts the service. Drop-ins in `elamond.d` still add to it. It's in the Nix store, readable by every user: no secrets |
| `services.elamond.defaults` | read-only: the settings elamond starts with |

The firewall stays the system's. If you set your own `port` here, give the system the same one
(`services.elamond.settings.port` beside `openFirewall`; without `enable`, that only opens the
port), or open it yourself.

## 8. Updating, moving and removing

### Updating

| Installed from | To update |
|---|---|
| a release | unpack the new one and install it over the old binary as in [section 1](#1-install-the-service), then run `elamond setup` |
| source | `git pull`, `cargo build --release` in `elamond/`, install the binary again, then run `elamond setup` |
| a Nix profile | `nix profile upgrade --all`, then `systemctl --user daemon-reload` and `systemctl --user restart elamond` |
| NixOS or Home Manager | `nix flake update elamon`, then rebuild; the rebuild restarts the service |

For a release or a source build, `elamond setup` sees the new binary and restarts the service.

**Update the app and the service together.** App 0.6 and service 0.6 speak API 2, which no earlier
app or service does. Each asks for the other to be updated when they don't match. Pairings stay:
the phones keep their keys and roles and don't pair again.

**From 0.6.0 to 0.6.1,** there's nothing to do by hand:

- At its first start, the service moves a 0.6.0 `elamond.toml` (one without `schema`) to a whole
  file: everything 0.6.0 had built in, with your keys in their places and your own tables after
  them. The old file stays beside it as `elamond.toml.0.6.0`. The phones see the same as before.
- Each paired phone gets its own profile in `elamond.d/phones.toml`, using the one it had
  ([section 7](#each-phones-own-profile)).
- Under NixOS and Home Manager, the module holds the defaults: `services.elamond.settings` work as
  they are.
- App 0.6.0 works with service 0.6.1 and the other way round, but update both: the phone learns the
  computer's addresses from 0.6.1 on, and finds it over a VPN after pairing on the local network.

**From 0.5 to 0.6,** the settings change once (settings schema 2). The 0.6 service doesn't start
with 0.5's `[[links]]`, `[[actions]]` and `units`. `elamond check` names each line and what replaces
it, and `elamond edit` lets you fix them with the check before saving:

```toml
# 0.5                                   # 0.6
units = ["syncthing.service"]           [links.syncthing]           # a unit is named on its link
                                        name = "Syncthing"
                                        url = "http://{host}:8384"
                                        unit = "syncthing.service"

[[links]]                               [links.jellyfin]
name = "Jellyfin"                       name = "Jellyfin"
url = "http://{host}:8096"              url = "http://{host}:8096"
unit = "jellyfin.service"               unit = "jellyfin.service"
scope = "system"                        scope = "system"

[[actions]]                             [actions.backup]
name = "Back up now"                    name = "Back up now"
command = ["/path/to/backup"]           command = ["/path/to/backup"]
```

- On NixOS and Home Manager, change the same in `services.elamond.settings`
  (`links.jellyfin = { … };`); the rebuild checks it.
- The links the 0.5 app added move into the settings by themselves ([section 5](#links)).
- The app's own display settings are now the computer's profiles ([section 7](#profiles)); the app
  shows the old ones once as a profile to copy ([section 4](#home)).
- From 0.4 to 0.5, each phone had to pair again (then `elamond revoke` its old entry).

### Backing up the service

What makes the service the same computer to its phones is in `~/.local/share/elamon`: its id, its
certificate and key with the rotations behind them, the paired phones, the activity log and
`elamond control off`'s mark. Lose it, and every phone has to pair again. Keep a copy:

```sh
elamond backup ~/elamond.backup
```

- It asks for a passphrase twice, at least 8 characters (a few words are best), and writes one file
  only you can read, encrypted with it ([section 10](#10-security) says how).
- Keep the file and the passphrase apart. Without the passphrase the backup can't be opened, and
  whoever has both can pose as this computer to its phones.
- The backup also carries the settings, `elamond.toml` and the drop-ins in `elamond.d`. A restore
  puts them back only when asked.

**For a script or a timer,** give the passphrase in a file only you can read (`chmod 600`):

```sh
elamond backup /backups/elamond.backup --passphrase-file ~/.local/share/keys/elamond-passphrase
```

Keep that file away from the backups: not beside them, not in a folder that is backed up or synced
with them, and not in `~/.config/elamon`, whose settings may travel with a backup of your home.

`--plain` writes the backup unencrypted, with a warning. Use it only for a copy that stays in a
private folder, never one that goes to cloud storage or another computer.

A backup holds the certificate's key as it was then. After `elamond rotate-cert`, older backups
still hold the old key. Delete those you don't need, and all of them if you rotated because the key
may have leaked ([section 10](#a-new-certificate)).

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
name, or one in `names` ([section 7](#names-in-the-pairing-link)). A phone that can't reach it any
other way has to pair again.

What `elamond restore` does:

- It checks everything in the backup before it writes anything. It refuses a wrong passphrase, a
  file that was changed or damaged, a backup from a newer elamond (update the service first) and,
  unless told `--force`, a running service.
- Each file it replaces or removes, it keeps beside as `NAME.before-restore-TIME`. Old keys are among
  them: delete those once all is well.
- It prints what it restored: the computer's id and certificate, the phones and the links.
- `--passphrase-file PATH` reads the passphrase from a file, as for `elamond backup`.
- `--with-settings` also puts back `elamond.toml` and the drop-ins in `elamond.d`, but never over a
  file NixOS or Home Manager make: set those in `services.elamond.settings`.
- On a computer that has phones paired already, a restore would bring back the backup's phones that
  aren't paired there now, including any you unpaired since, lost or stolen ones too. It lists them
  and stops. `--with-old-phones` restores them anyway.
- Control held off by `elamond control off` stays off, whatever the backup says.
- A backup whose certificate was since replaced by `elamond rotate-cert` is restored with a warning.
  If it was replaced because the key may have leaked, don't use that backup.

`elamond restore --force` writes under a running service. That service presents the restored
certificate within a few seconds and knows the restored phones at once, but keeps the id, links and
activity log it started with until it restarts. Restart it right after:
`systemctl --user restart elamond`.

### A new phone

1. **On the old phone:** Settings → *Export pairings*. Choose a passphrase (at least 8 characters;
   the app refuses easy ones, and *Generate* makes one of six random words), then where to save the
   file (`elamon-pairings-DATE.elamon`). The file holds every paired computer with the phone's key
   to it, encrypted.
2. **Copy the file** to the new phone and install Elamon there.
3. **On the new phone:** Settings → *Import pairings*, the file and its passphrase. New computers are
   added. For a computer the phone has already, with the same certificate and addresses, the file's
   copy replaces it. Where they differ, the app asks for each: *Replace* or *Keep this phone's*.

**Each computer gives the new phone a key of its own** the first time the app reaches it after the
import. It shows in `elamond clients` as a new entry with the new phone's name and the old phone's
role, *added … with the key of* the old phone's ID.

- One token gives one such key. So a file imported on several phones gives a key to the first of
  them only; pair the others.
- The old phone keeps working until it's revoked, and revoking it leaves the new one paired.
- Moving for good: forget the computers on the old phone, or reset it, then revoke its entry:

  ```sh
  elamond clients
  elamond revoke OLD_ID
  ```

**Families of keys.** `elamond revoke` names, loudly, the phones still paired that got a key with
the token it revoked: a phone you don't know there got its key from that one. To the computer they
are a family, which shares one rate of acting and one share of the activity log.
`elamond revoke OLD_ID --with-added` unpairs the whole family, even by an ID already gone. That's
the way to cut off a lost or stolen phone and whatever got a key with its token.

A computer that takes no more phones (64), or where the old phone's one key was already given to
another phone, can't give the new phone a key of its own. It then sees the new phone as the old
one: one entry for both, and revoking it cuts off both. Pair the new phone again there (scan the QR
code of `elamond pair`; the app replaces its entry) before revoking the old one.

### Removing the service

1. `systemctl --user disable --now elamond`.
2. Delete the binary and the unit file (`~/.config/systemd/user/elamond.service`).
3. Delete `~/.config/elamon` and `~/.local/share/elamon`.
4. As root: delete `/etc/polkit-1/rules.d/50-elamond.rules`, close the ports you opened
   ([section 2](#2-open-the-firewall)), and, if nothing else needs it,
   `sudo loginctl disable-linger YOUR_USER`.

In the app, ⋮ → *Forget computer*.

## 9. Troubleshooting

First, run `elamond setup --check`. It prints a line for each thing the service needs, with what to
do about those marked `!`. To see what the service reads without the app, run `elamond snapshot`.

| What happens | What to do |
|---|---|
| The computer isn't under *Found on this network* | Same network, and UDP 5353 open on the computer. VPNs and many guest Wi-Fi networks block discovery, and the service announces itself only on wired and wireless interfaces it listens on (`mdns: announced on …` in the journal). Add it by address or name instead: `elamond pair` prints them. |
| The app can't connect | Check `elamond setup --check`, `systemctl --user status elamond` and `journalctl --user -u elamond -e`. TCP 47823 must be open, and the phone must reach an address the service listens on (`listen: on …` in the journal). |
| It works only while you're logged in | `sudo loginctl enable-linger YOUR_USER`, as `elamond setup` prints it. |
| *The computer's certificate changed* | The service's data was deleted or the system reinstalled. Restore a backup ([section 8](#a-new-computer-or-reinstalling)), or forget the computer and pair again. If neither happened, check the network first. After `elamond rotate-cert` the app follows by itself; after `--no-statement`, every phone says this and pairs again ([section 10](#a-new-certificate)). |
| *Control is off* | Set `control = true` in `elamond.toml` or a drop-in; the service takes it up within a few seconds. If it's set, `elamond control status` tells whether `elamond control off` holds it off, or whether the settings stopped parsing. |
| Pairing ends with the phone turned away | Answer `y` at `elamond pair` within 90 seconds, and keep it running until the phone is paired ([section 3](#3-pair-the-phone)). |
| *This phone's role on the computer doesn't allow that* | `elamond clients` shows the phone's role, `elamond clients role ID ROLE` changes it ([section 6](#6-roles-who-may-do-what)). An action may ask for `admin` (`role` in its settings). |
| *Too many requests* | The phone asked for more than 10 things in a row; wait a few seconds. When pairing: five wrong codes came from this address; run `elamond pair` again for a new code. |
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

## 10. Security

**Connection and keys**

- Everything goes over TLS. The service's certificate is self-signed. The app pins it when pairing
  and refuses any other afterwards, except one the pinned certificate's key signed
  (`elamond rotate-cert`, [below](#a-new-certificate)).
- Each phone holds its own random 256-bit key; the service keeps only its SHA-256. Keys stay out of
  Android backups.
- A pairing code works for five minutes, one attempt at a time, each wrong one answered after a
  second. Five wrong ones shut that address out; twenty in all use the code up. The phone that gives
  the right code is let in only when you say so at `elamond pair` (unless it runs with `--yes`).
- Without a key, the service answers only which service it is (`hello`: name, id and version),
  pairing, and the statements that lead from one certificate to the next (`identity/rotations`),
  which prove themselves.
- It holds 256 connections at once, 16 from one address until one shows a phone's key, and drops a
  connection that says nothing within seconds, so a flood of them doesn't keep the phones out.
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
- It never signals itself, its parents or PID 1. With the start time the app sends, it never signals
  a process that took the PID of the one meant.
- A phone acts at most 10 times in a row, then once a second. Settings that stop parsing hold
  control off until they parse again.
- Every request to act is in the activity log, carried out or refused. What was carried out is also
  in the journal, with the name of the phone that asked: `journalctl --user -u elamond`.

**Backups and moved pairings**

- A backup (`elamond backup`) and its passphrase together are the computer's identity. Unless made
  with `--plain`, it's encrypted, and written for your eyes only. Argon2id makes the key from the
  passphrase (64 MiB of memory, 3 passes, 4 lanes); XChaCha20-Poly1305 seals the rest and
  authenticates the header.
- Exported pairings are encrypted with their own passphrase. PBKDF2-HMAC-SHA256 with 600,000
  iterations makes the key; AES-256-GCM seals the list and authenticates the header. The keys are
  taken out of Android's key store for this alone and never written anywhere unencrypted.
- A phone that imports pairings asks each computer for a key of its own at its first connection,
  beside the exporting phone's, which stays until revoked ([section 8](#a-new-phone)). Any phone
  paired with a code may get one such key, once, with its own role and no more. `elamond clients`
  and `elamond revoke` say whose key added it, the activity log has it, and
  `elamond revoke ID --with-added` cuts off both. At most 64 phones are paired at once.
- An old key lives on in backups and in files a restore set aside. `elamond restore` never lets go
  of control held off, and brings back phones unpaired since only with `--with-old-phones`.

### A new certificate

```sh
elamond rotate-cert
```

This gives the service a new certificate and key; do it from time to time.

- Before the old key is deleted, it signs the new certificate's fingerprint. The service shows that
  signature to any phone that asks.
- A phone that meets the new certificate checks the signature against the certificate it pinned and
  moves to the new one by itself, saying *The computer's certificate was renewed*. Nothing needs
  pairing again.
- The running service presents the new certificate within a few seconds (at once with
  `systemctl --user reload elamond`). Connections already open keep the old one until they end.
- The last five rotations are kept, so a phone that was away for one or two still follows. One that
  missed more says *The computer's certificate changed*: pair it again.
- Deleting `~/.local/share/elamon` is not a rotation: it unpairs every phone.

That is routine renewal, and only that. **If the key may have been copied off the computer,** a
phone mustn't follow what it signs: whoever has it could sign their own certificate. Then:

```sh
elamond rotate-cert --no-statement   # a new certificate nothing leads to; the old key deleted unused
elamond revoke all                   # the keys phones sent to a computer that may not have been this one
elamond pair                         # each phone again; the app replaces the computer it had
```

Every phone then says the certificate changed. The old key lives on in backups made before (with
`elamond backup`) and in `key.der.before-restore-*` files a restore left in `~/.local/share/elamon`.
Delete them, as `elamond rotate-cert` reminds you.
