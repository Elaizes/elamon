# Elamon user guide

> **Test software in active development.** Features, settings and the protocol between the app
> and the service can change from one version to the next, and things may break. Use it where
> that's fine, and update the app and the service together.

Elamon has two parts:

- **the app** on an Android phone (Android 8 or newer);
- **elamond**, the Elamon service, a small program on the Linux computer you want to watch. It
  reads the computer's own counters and serves them over HTTPS, only to phones you paired with it.
  It runs as your user and needs no root.

The phone talks to the computer directly: on the same network, or over a VPN both are on
(Tailscale, WireGuard and the like). Nothing goes through a server.

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

The same way on every distribution that runs systemd: Debian, Ubuntu, Fedora, Arch, openSUSE and
the rest. On NixOS, the module does it ([below](#nixos-and-home-manager)).

1. **Get elamond.** Download it from the [latest release](https://github.com/Elaizes/elamon/releases/latest):
   `x86_64` for most PCs, `aarch64` for ARM boards. It runs on any distribution with glibc 2.28 or
   newer (Debian 10, Ubuntu 20.04, RHEL 8 and everything since). Check it, unpack it and put it
   where it stays:

   ```sh
   sha256sum -c elamond-VERSION-x86_64-linux.tar.gz.sha256
   tar -xzf elamond-VERSION-x86_64-linux.tar.gz
   install -Dm755 elamond-VERSION-x86_64-linux/elamond ~/.local/bin/elamond
   ```

   Or [build it from source](#building-from-source).

2. **Set it up**, as yourself, not root:

   ```sh
   ~/.local/bin/elamond setup
   ```

   It installs the service as a systemd user unit (`~/.config/systemd/user/elamond.service`,
   running the binary you ran it from), writes settings to start from in
   `~/.config/elamon/elamond.toml` when there are none, and starts the service. Run again, it
   changes only what changed, and restarts the service when its binary was moved or updated.
   `--print` shows what it would do without doing it; `--no-start` leaves the service stopped.

3. **Run what it prints as root.** Each step is printed only while it is needed:

   - `sudo loginctl enable-linger YOUR_USER`, to keep the service running while you're logged out;
   - the commands that open the firewall, for the firewall that runs ([section 2](#2-open-the-firewall));
   - `sudo install -m 0644 … /etc/polkit-1/rules.d/50-elamond.rules`, the polkit rule the control
     panel needs for power, the power profile and system units
     ([section 5](#power-the-profile-and-system-units-polkit)).

4. **Pair the phone**: `elamond pair` ([section 3](#3-pair-the-phone)).

`elamond setup --check` tells whether it all works: a line for each thing the service needs or
uses, marked `OK`, `!` (wrong, with what to do), `?` (can't tell, often without root) or `-`
(not on this computer, and not needed). It exits with 1 when the service can't run or be reached.

If your shell doesn't find `elamond`, `~/.local/bin` isn't on your `PATH`; call it as
`~/.local/bin/elamond`. `elamond setup` knows systemd only for now; under another init system,
have it run `elamond serve` as your user at login, which needs nothing else.

### Building from source

It needs git, a C compiler and Rust 1.88 or newer.

| System | Build tools |
|---|---|
| Fedora | `sudo dnf install git cargo gcc` |
| Arch | `sudo pacman -S --needed git rust base-devel` |
| openSUSE | `sudo zypper install git cargo gcc` |
| Debian, Ubuntu | `sudo apt install git build-essential curl`, then Rust from [rustup](https://rustup.rs), since their own is older than 1.88 |

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

On NixOS, import the module and turn the service on, then `sudo nixos-rebuild switch`:

```nix
imports = [ inputs.elamon.nixosModules.default ];

services.elamond = {
  enable = true;        # runs for everyone who logs in
  openFirewall = true;  # TCP 47823, and UDP 5353 for discovery
};
users.users.YOUR_USER.linger = true;  # keeps it running while you're logged out
```

The polkit rule is `services.elamond.control` ([section 5](#power-the-profile-and-system-units-polkit)).
There, `elamond setup` only points at these options; `elamond setup --check` works as anywhere.

With Home Manager, for one user, `imports = [ inputs.elamon.homeModules.default ];` and
`services.elamond.enable = true;`. The firewall stays the system's: on NixOS `openFirewall`
works without `enable`; on another distribution, `elamond setup` leaves Home Manager's unit alone
and prints what needs root.

Nix on another distribution, without Home Manager:

```sh
nix profile install github:Elaizes/elamon
systemctl --user link ~/.nix-profile/lib/systemd/user/elamond.service
systemctl --user enable --now elamond
elamond setup    # what needs root: lingering, the firewall, the polkit rule
```

## 2. Open the firewall

The service listens on **TCP 47823**. **UDP 5353** is mDNS, which lets the app find the computer by
itself; without it, add the computer by address. Over a VPN only 47823 matters, and an interface
the firewall already trusts needs nothing.

`elamond setup` finds the firewall that runs (firewalld, ufw or nftables) and prints the commands
that open both for it, with the port your settings give; `elamond setup --check` tells whether
they are open, as far as it can see without root. By hand:

| Firewall | Commands |
|---|---|
| NixOS | `services.elamond.openFirewall = true;` |
| firewalld (Fedora, openSUSE) | `sudo firewall-cmd --permanent --add-port=47823/tcp`<br>`sudo firewall-cmd --permanent --add-service=mdns`<br>`sudo firewall-cmd --reload` |
| ufw (Ubuntu, when it's on) | `sudo ufw allow 47823/tcp`<br>`sudo ufw allow 5353/udp` |
| nftables (when its service runs) | `sudo nft insert rule inet filter input tcp dport 47823 accept`<br>`sudo nft insert rule inet filter input udp dport 5353 accept`<br>and the same rules in `/etc/nftables.conf`, or they're gone after a reboot |

Arch and Debian ship with no firewall running; then there's nothing to open.

To open the port on some interfaces only, name them in `listen`
([section 7](#where-the-service-listens)) and run `elamond setup` again: it opens the port there
only (with firewalld, in their zones), and UDP 5353 only on those not named like a VPN, where
discovery doesn't reach anyway. On NixOS,
`services.elamond.firewallInterfaces = [ "eno1" "tailscale0" ];` does the same.

## 3. Pair the phone

On the computer:

```sh
elamond pair
```

It prints a QR code and a six-digit code, good for five minutes. Five wrong tries from one address
shut that address out for the rest of the code's life; twenty in all use the code up.

When a phone gives the right code, `elamond pair` asks you first:

```
  Phone "Pixel 9" from 192.0.2.23 asks to pair as admin. Allow? [y/N]
```

Answer `y` if that is your phone, and it is paired; anything else, or no answer within 90 seconds,
turns it away, and the code no longer works. Meanwhile the app says to confirm on the computer.
So keep `elamond pair` open until the phone is paired: stopped, it turns the phone away too. For
a script, or where nobody can answer (under systemd or cron), `elamond pair --yes` pairs whichever
phone gives the code without asking; without `--yes` and with nothing to answer on, `elamond pair`
says so and stops.

- **QR code:** point the phone's camera at it and open the link. The link carries the computer's
  addresses (those the service listens on, the local network first), then names that stay when
  addresses change (those in `names` and its Tailscale name,
  [section 7](#names-in-the-pairing-link)), and its certificate, so pairing takes one tap. The app
  keeps them all, and later tries the one that worked last first.
- **Code:** in the app, tap the computer under *Found on this network*, or *Add computer* and its
  address or host name. The app shows the computer's certificate digits first: they must match the
  `Certificate:` line `elamond pair` printed. Then enter the code.

Each phone gets its own key, and a role that says what it may see and do: by default `admin`,
everything. Pairing someone else's phone, give it less, for example
`elamond pair --role glance` ([section 6](#6-roles-who-may-do-what)). `elamond clients` lists the
paired phones with their roles and `elamond revoke ID` unpairs one. Up to 64 phones can be paired
with one computer.

## 4. The app

### Home

Every paired computer is a card with its CPU, memory and temperature, refreshed every few seconds
while the screen is open, and a dot for online or offline. Below them, *Found on this network*
lists computers on the local network that run the service and that this phone isn't paired with
yet. Tap a card to open the computer; **Add computer** pairs another one. The **?** opens the
help, the gear the settings.

### A computer

The name, how fresh the data is (*Live · every 1 s*) and the computer's state sit in the big title
at the top; scroll and they move into the bar. What the tabs are, and what is on them, the
computer decides: they are its dashboards, as its settings describe them, and as far as the phone's
role sees them ([section 7](#dashboards-and-widgets)). The standard four along the bottom:

- **Overview** — tiles for the CPU, memory, each graphics card, temperatures, network, storage and
  battery. **Tap a tile** for its page: a large chart of the last ten minutes, details, and the
  processes using the most of it (on the CPU page also every core with its clock).
- **Processes** — the heaviest first. **Sort** with the chips (CPU, memory, GPU, disk), **search**
  by name, command, user or PID, **By app** adds up the processes of one program. **Tap a
  process** for its details: user, state, start time, disk and GPU use, the full command, and —
  with control on and the `admin` role — **Quit** and **Force kill**.
- **Sensors** — every temperature the computer reports, grouped by device, a chart of the CPU and
  GPU temperatures, and the fans. A limit appears next to a sensor when it gets close to it.
- **Control** — links; the monitors, shown even when control is off: folded to one line with how
  many are connected and on, then a line per monitor with its mode, and a tap on one for its
  details (port, size, native mode, highest refresh rate, position) and the arrangement below;
  media, power profile, actions and power; see [section 5](#5-the-control-panel).

On every chart, **drag a finger** sideways to read the values at that moment and how long ago it
was. The charts are full the moment you open a computer: the service keeps the last ten minutes
(`history_minutes`). Where the phone was away or the screen off, the line has a gap instead of
joining the two ends.

The **⋮** menu has *Settings*, *Help* and *Forget computer*. Back closes a detail page first, then
the computer.

A discrete graphics card that has powered itself down shows as *Asleep* and is left alone: reading
it would wake it. The app only streams while it's on screen, so it costs no battery or data in the
background.

### Settings

How the app looks and behaves with a computer comes from that computer: the phone's *profile*,
which the computer's settings define and `elamond pair --profile` or `elamond clients profile`
choose ([section 7](#profiles)). The app shows which one it has; to change one, change the
profile on the computer.

| | Profile key |
|---|---|
| Theme | `theme`: follow the phone (`system`), or always `light` or `dark` |
| Pure black | `pure_black`: black instead of dark grey in dark mode; saves power on OLED screens |
| Wallpaper colours | `android.dynamic_color`: take the colours from the wallpaper (Android 12+) |
| Blur | `android.blur`: frosted glass behind the bars and dialogs (Android 12+) |
| Animations | `animations`: rolling numbers and moving gauges |
| Update every | `interval_ms`: slower saves data on mobile networks |
| Temperature | `fahrenheit`: °C or °F |
| Network speed | `network_bits`: bytes per second, or bits per second as providers state it |
| Keep screen on | `android.keep_screen_on`: while a computer is open, to use the phone as a dashboard |
| Confirm actions | `confirm_actions`: ask before actions that ask |
| Unlock first | `unlock`: the action kinds the phone is unlocked for first, and for how long an unlock holds |
| Haptic feedback | `android.haptics`: a tick on long presses |

What stays the app's own, for every computer:

| | |
|---|---|
| Export pairings | save every paired computer to a file encrypted with a passphrase, for a new phone ([section 8](#a-new-phone)) |
| Import pairings | add the computers in such a file |

### How do I…

- **…see what slows the computer down?** Overview → tap the CPU or Memory tile: the bottom of the
  page lists the processes using the most. Or the Processes tab, sorted by CPU or memory.
- **…close a program that hangs?** Processes → find it → tap → *Quit*; if it doesn't go, *Force
  kill*. (Needs control, and a phone paired as `admin`.)
- **…check how hot it runs?** The Temperature tile, or Sensors for every sensor and the fans.
- **…open a web page the computer hosts?** Control → tap its tile. Over a VPN the link uses the
  VPN address by itself.
- **…restart a web app that stopped answering?** Control → hold its link → *Restart*.
- **…add a link?** On the computer, a `[links.<id>]` table in its settings, with its systemd unit
  to start and stop it from here ([section 5](#links)). The app no longer adds links.
- **…pause the music?** Control → Media: play, pause, skip, seek, and the computer's volume.
- **…make it quieter or faster?** Control → Power profile.
- **…run my own script from the phone?** Add it as an `[actions.<id>]` table on the computer, with
  the choices it may be given ([section 5](#actions)); it shows up as a button under Actions, or
  where a dashboard puts it.
- **…see something the standard tabs don't show?** A dashboard of your own on the computer
  ([section 7](#dashboards-and-widgets)), and its id in the phone's profile.
- **…turn it off?** Control → Power → *Shut down*. Only someone at the computer can turn it on
  again, unless it wakes on LAN.
- **…watch several computers?** Pair each one; the home screen shows them all.
- **…keep it on screen as a dashboard?** Settings → *Keep screen on*.
- **…let someone watch the computer without touching it?** Pair their phone with
  `elamond pair --role glance` (the basics) or `--role monitor` (everything to see, nothing to
  change); see [section 6](#6-roles-who-may-do-what). `--profile` gives it other tabs and looks.
- **…stop every phone from acting, right now?** `elamond control off` on the computer;
  `elamond control on` lets them again. They still see the computer.
- **…cut off a lost or stolen phone?** On the computer `elamond clients`, then
  `elamond revoke ID --with-added`: it can neither see nor act any more, and nor can a phone that got
  a key of its own with its token.
- **…stop a phone from seeing a computer?** On the computer `elamond clients`, then
  `elamond revoke ID`; on the phone, ⋮ → *Forget computer*.
- **…move to a new phone?** Settings → *Export pairings* on the old one, *Import pairings* on the
  new one ([section 8](#a-new-phone)).
- **…reinstall the computer without pairing the phones again?** `elamond backup` before,
  `elamond restore` after ([section 8](#a-new-computer-or-reinstalling)).

## 5. The control panel

The **Control** tab acts on the computer. It's off until the computer allows it, so a phone that
can watch can't also act by default.

### Turning it on

In `~/.config/elamon/elamond.toml`:

```toml
control = true
```

The service notices the change within a few seconds by itself (or at once with
`systemctl --user reload elamond`). On NixOS, set it through the module instead
(`services.elamond.settings.control = true;`): the service then reads its settings from a read-only
file the module builds, and restarts when they change (a rebuild that changes them restarts every
user's service). Anything you want to add by hand goes in a drop-in beside it,
`~/.config/elamon/elamond.d/*.toml` ([section 7](#7-settings-reference)).

With control on, each phone does as much as its role allows: a phone paired as `admin` (the
default) everything below, others less
([section 6](#6-roles-who-may-do-what)). `elamond control off` stops them all at once, whatever the
settings say.

### What comes from where

| | Found by the service | Set on the computer |
|---|---|---|
| Power: lock, sleep, restart, shut down | which of them the system allows | permission (polkit, below) |
| Power profile | profiles of power-profiles-daemon | permission (polkit) |
| Media and volume | any MPRIS player that's running; the default output through `wpctl` or `pactl` | — |
| Quitting and killing processes | — | — |
| Links | whether something answers on their port, their units' states | `[links.<id>]` in the settings |
| Actions | — | `[actions.<id>]` in the settings, and only there |

Each of these is an *action* to the app, with an id: the built-in `power.reboot`, `profile.set`,
`process.signal`, `unit.restart`, `media.play_pause`, `volume.set` and the rest
([docs/config.md](config.md#built-in-actions) lists them), and those the settings add. The service
leaves out the ones the computer can't do, and those the phone's role may not run. The phone asks
before an action that asks to be confirmed, and unlocks first for those its profile says.

### Links

A link opens a web page the computer hosts. Each is a row with its state: *Running*, *Stopped*,
*Failed*, *Local only* or *No such unit*. Tap to open it in the browser; hold it for start, stop
and restart.

```toml
[links.media]                 # the link's id: a-z, 0-9, _ and -
name = "Media server"
url = "http://{host}:8096"    # {host}: the address the phone reached the computer at
unit = "SERVICE.service"      # optional: lets the app start, stop and restart it
scope = "system"              # or "user" (the default)
icon = "music"                # optional; "globe" by default
```

- `{host}` makes the link work on the local network and over a VPN alike. `localhost` and
  `127.0.0.1` in a URL are replaced the same way.
- A link opens a web page: its URL starts with `http://` or `https://`, or is a bare host and port
  (`{host}:3000`), opened as `http://`. Another kind (`javascript:`, `file:`, `intent:`…) is a
  mistake in the settings.
- *Local only* means what the link points to listens on the computer's own address (127.0.0.1):
  other devices can't open it. The app warns before trying. Make it listen on all addresses, or
  reach it through an SSH tunnel.
- Links come from the settings alone; phones with the `monitor` role or above see them, those
  with `operate` or `admin` start, stop and restart their units. The app doesn't add or change
  links any more: the links the 0.5 app added are written into the settings once, as
  `~/.config/elamon/elamond.d/links-from-app.toml`, when the 0.6 service first starts
  (`links.json` beside its state becomes `links.json.migrated`). Change them there.

A phone can only start or stop the unit a link names, so it can never touch what you didn't
choose: only service units (`NAME.service`), each in its own scope; a template's instance
(`podman-kube@….service`) only exactly as written. The service never starts or stops its own unit,
and a link naming it is a mistake. System units also need polkit's leave (below).

### Actions

```toml
[actions.backup]
name = "Back up now"
command = ["/path/to/backup-script", "--now"]   # run without a shell
confirm = true      # ask in the app first (default)
timeout = 600       # seconds before it's stopped (default 60)

[actions.fan]
name = "Fan"
command = ["/usr/local/bin/fanctl", "set", "{speed}"]
params.speed = { choices = ["quiet", "auto", "full"], labels = { quiet = "Quiet", auto = "Auto", full = "Full" }, default = "auto" }
role = "admin"      # who may run it: "operate" (default) or "admin"
```

Each becomes a button under Actions on the Control tab, and a dashboard can put it anywhere: a
button, or a `select`, `toggle` or `slider` that sets one of its parameters
([section 7](#dashboards-and-widgets)). What it prints comes back to the phone, its first 4 KiB and
last 12 KiB.

A phone never writes a command: it can only choose among the values a parameter allows. A choice
(`choices`), a number in a range on a step (`range = [0, 100]`, `step = 5`) or a bool
(`type = "bool"`); one without `default` must be given. An argument that is exactly `"{speed}"` is
replaced by the value, whole: a value never becomes part of another argument or an option. The
service checks each value before anything runs, and refuses one it doesn't allow.

Commands run as your user, with the service's environment, whose `PATH` is the service unit's and
may be shorter than your shell's: give the program's full path (`/usr/bin/…`, on NixOS
`/run/current-system/sw/bin/…`, `/etc/profiles/per-user/YOUR_USER/bin/…`, or
`/run/wrappers/bin/sudo`). On NixOS the module gives the unit a `PATH` with those directories; the
volume's `wpctl` and `pactl` are looked for there in any case.

### Power, the profile and system units: polkit

The service runs outside a login session, so polkit asks for a password it can't give for powering
off, restarting, sleeping, switching the power profile and managing **system** units. The app
marks those *needs permission*. User units need no permission.

polkit can't tell the Elamon service from the other programs you run: what such a rule allows
without a password, it allows every process of your user, not only the service. Allow only what
you want phones to do.

`elamond setup` writes the rule: your user may power off, restart and sleep the computer and
switch the power profile, and start, stop and restart the system units the settings' links name
(`unit` with `scope = "system"`, [Links](#links)). It saves it as
`~/.local/share/elamon/setup/50-elamond.rules` and prints the command that installs it:

```sh
sudo install -m 0644 ~/.local/share/elamon/setup/50-elamond.rules /etc/polkit-1/rules.d/50-elamond.rules
```

After changing which system units the links name, run `elamond setup` again and install the new
rule the same way; `elamond setup --check` says when the one installed no longer matches.
`elamond setup --multiple-sessions` also allows powering off, restarting and sleeping while
someone else is logged in too, ending their sessions.

On NixOS the module writes it:

```nix
services.elamond.control = {
  users = [ "YOUR_USER" ];            # power and the profile without a password
  units = [ "SERVICE.service" ];      # system units these users may start, stop and restart
  # multipleSessions = true;          # also while other users are logged in, ending their sessions
};
```

By hand, the rule is this, with your user name and units, in `/etc/polkit-1/rules.d/50-elamond.rules`:

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

*Lock* locks every session of your user through logind; your desktop's locker has to respond to
that (GNOME and KDE do; on wlroots compositors, set up e.g. hypridle's `lock_cmd`).

## 6. Roles: who may do what

Every paired phone has a role. Each allows what the one before it does, and more:

| Role | Sees | Does (with control on) |
|---|---|---|
| `glance` | the basics: CPU, memory, graphics cards, temperatures, fans, storage, network, battery and power, with their history | nothing |
| `monitor` | everything: processes with their commands and users, monitors, links, media, the activity log | nothing |
| `operate` | everything | the links' units, media and the volume, the power profile, the actions from `[actions.<id>]` |
| `admin` | everything | all that, and shutting down, restarting, sleeping, locking, quitting and killing processes, and the actions with `role = "admin"` |

A phone with no role in the list of paired phones is `admin`. The role filters the values
themselves, on the computer: a widget, a table or a whole dashboard whose values a role doesn't
see isn't sent to that phone at all, nor are actions it may not run. A dashboard's or a widget's
own `role` in the settings can only ask for more.

### Choosing a role

`elamond pair` pairs as `admin` and says so; give another role when you pair:

```sh
elamond pair --role glance     # someone else's phone: how the computer is doing, nothing more
elamond pair --role monitor    # sees what runs and what you host, changes nothing
elamond pair --role operate    # everyday control without power or killing processes
```

To make another role the default for `elamond pair`, set `default_role = "monitor"` (or any role)
in `elamond.toml`.

`elamond clients` lists each phone with its role, its profile and the app it last said it is
(`android 0.6.0`). To change a role later:

```sh
elamond clients role ID glance    # ID from `elamond clients`
elamond clients role all monitor  # every paired phone
elamond clients role ID glance --with-added   # and the phones that got a key of their own with its token
```

It applies at once: the phone's next request, and the stream it has open hears of it within a
second or two and fetches what it may see now, so a phone turned into `glance` stops getting
processes, monitors and links. The app shows only what the phone's role allows.

A phone's profile (its tabs, theme and the rest, [section 7](#profiles)) changes the same way:

```sh
elamond pair --profile owner            # pair with the profile `[profiles.owner]` of the settings
elamond clients profile ID owner        # change it later; all, and --with-added, as for roles
```

### Stopping every phone at once

```sh
elamond control off     # no phone acts on the computer, whatever its role and the settings
elamond control status  # whether control is on, and why
elamond control on      # back to what the settings say
```

`control off` takes effect at the next request; phones still see what their roles show. It
survives restarts of the service and of the computer, and a restore, until `elamond control on`.
It's the quick way when something acts that shouldn't: the settings' `control = true` stays as it
was. A lost or stolen phone still sees everything with control off: unpair it, with the keys its
token gave, `elamond revoke ID --with-added` ([section 8](#a-new-phone)).

### The activity log

Everything a phone asks the computer to do is written down, carried out or refused, with how it
ended: when, which phone, what (*power reboot*, *unit restart nginx.service*, *action Fan quiet*,
the action's name and the values it was given…) and the result. Up to 500 are kept in `~/.local/share/elamon/activity.jsonl` (one JSON
object per line; only your user can read it) and survive restarts; of them, a phone keeps 100 at
most (with the keys of their own its token gave), so one that keeps asking pushes out only its own.
Phones with the `monitor` role or above can read it in the app (Control → Activity log). What was carried out is also in
the journal: `journalctl --user -u elamond`.

A phone may ask the computer to do 10 things in a row, then one a second; more are refused for a
moment (*Too many requests*). A phone and the keys of their own its token gave share that rate.
Seeing is never limited.

## 7. Settings reference

### `elamond.toml`

`~/.config/elamon/elamond.toml`, or the file `ELAMON_CONFIG` names, then every
`~/.config/elamon/elamond.d/*.toml` in name order, all after what is built into the service (the
standard dashboards and the `default` profile). A table with an id (`[links.web]`,
`[actions.fan]`, `[dashboards.power]`, `[profiles.owner]`) adds to the one of the same id before
it, key by key; a list (`widgets`, `listen`, a profile's `dashboards`) replaces the one before
whole. So a drop-in can add a dashboard, change one key of a built-in one, or replace its widgets.
Under NixOS or Home Manager, where `elamond.toml` comes from Nix and can't be edited, the drop-ins
are where your own additions go. All keys are optional. **The full reference, every key and
widget and value, is [docs/config.md](config.md)**; the commands:

```sh
elamond check              # read the settings as the service would, naming the file and line of each mistake
elamond check FILE         # the same with FILE in its place, before you put it there
elamond edit               # edit elamond.toml in $EDITOR; a broken one isn't kept
elamond edit ~/.config/elamon/elamond.d/power.toml   # or a drop-in
elamond defaults           # the built-in dashboards and profile, to copy from
elamond values             # every value this computer has now, by address, and the collections
elamond schema > ~/.config/elamon/elamond.schema.json   # for editors that check TOML
```

The top-level keys:

| Key | Default | |
|---|---|---|
| `port` | `47823` | TCP port |
| `listen` | `"::"` | addresses and interfaces to listen on, one or a list; `"::"` covers every address, IPv4 and IPv6 ([below](#where-the-service-listens)) |
| `names` | `[]` | host names that reach this computer, offered in the pairing link after its addresses ([below](#names-in-the-pairing-link)) |
| `name` | host name | the name shown in the app |
| `mdns` | `true` | announce the service on the local network: only on wired and wireless interfaces it listens on, never over a VPN |
| `interval_ms` | `1000` | time between readings, 250–10 000 |
| `history_minutes` | `10` | how far back graphs reach, 1–1 440 (at most 7 200 readings) |
| `control` | `false` | let phones act on the computer ([section 5](#5-the-control-panel)) |
| `default_role` | `"admin"` | the role `elamond pair` gives a phone when not told: `glance`, `monitor`, `operate` or `admin` ([section 6](#6-roles-who-may-do-what)) |
| `default_profile` | `"default"` | the profile of a phone paired without `--profile`, and of one whose profile is gone |
| `[links.<id>]` | | `name`, `url`, `unit`, `scope`, `icon` ([section 5](#links)) |
| `[actions.<id>]` | | `name`, `command`, `params`, `role`, `confirm`, `unlock`, `timeout`, `icon` ([section 5](#actions)) |
| `[profiles.<id>]` | | how a phone looks and behaves ([below](#profiles)) |
| `[dashboards.<id>]` | | pages of widgets ([below](#dashboards-and-widgets)) |
| `[templates.<id>]` | | widgets to start others from |

The settings of 0.5 and before (schema 1) had `[[links]]`, `[[actions]]` and `units`; the 0.6
service refuses them and says what replaces each: `[links.<id>]`, `[actions.<id>]`, and a unit named
on its link ([section 8](#updating)).

The service rereads the files when one of them changes, or on `systemctl --user reload elamond`. A
file that doesn't parse, or settings with a mistake, are named in the log with the file and line,
and the settings read before stay; but control is off until they read again, so a half-finished
edit can't leave phones acting on old permissions. `elamond control status` shows the mistake,
`elamond check` all of them. At start, settings with a mistake stop the service. Links, actions,
profiles, dashboards, `control`, `default_role`, `default_profile` and `names` apply at once (a
phone's open stream fetches its new dashboards by itself); `port`, `listen`, `name`, `mdns`,
`interval_ms` and `history_minutes` after a restart.

A secret in the settings (a link's URL with a password in it, a token among an action's arguments)
belongs in a file only you can read: `chmod 600` it. Under NixOS or Home Manager, never put one in
`services.elamond.settings`, which goes into the Nix store where every user can read it; use a
drop-in in `~/.config/elamon/elamond.d` instead.

### Profiles

How a phone looks and behaves: its tabs, the chips under the computer's name, its theme, how often
it asks for readings, its units, which actions it confirms and unlocks for. Each paired phone has
one; the built-in `default` is everyone's until you say otherwise.

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

`elamond pair --profile owner` pairs a phone with it, `elamond clients profile ID owner` gives it to
one paired already, `default_profile = "owner"` makes it the one phones get without saying
([section 6](#choosing-a-role)). Every key is in [docs/config.md](config.md#profiles).

### Dashboards and widgets

A dashboard is a page of widgets in a grid of four columns (two on a narrow screen): a tab when a
profile lists it, a page a widget opens, or the chips under the computer's name (a profile's
`header`). `elamond defaults` prints the standard ones: `overview`, `processes`, `sensors` and
`control`, the `header`, and the pages their tiles open.

```toml
[dashboards.power]
title = "Power"
icon = "battery"

[[dashboards.power.widgets]]
type = "gauge"
value = "battery.percent"       # an address: `elamond values` lists this computer's
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

Then put `"power"` in a profile's `dashboards`. The widget types are `value`, `gauge`, `meter`,
`status`, `graph`, `list`, `bars`, `table`, `button`, `toggle`, `select`, `slider`, `text`, `group`
and `media`; a `table` shows a collection (processes, links, disks, the activity log…), sorted,
searched, filtered, with what a row shows when opened and what it may run. Strings may hold values
(`caption = "{mem.used:bytes} of {mem.total:bytes}"`); `for_each` repeats a widget or a dashboard
for each GPU, disk or interface; templates (`[templates.<id>]`, `use`) save repeating yourself.
The service decides colours and levels (*warning* above 75 %, …) the same for every phone. All of
it is in [docs/config.md](config.md): widgets, addresses, collections, formats, icons, colours.

### Where the service listens

By default on every address, IPv4 and IPv6. `listen` narrows that to addresses and network
interfaces, as one entry or a list:

```toml
listen = ["tailscale0"]                    # over Tailscale only
listen = ["eno1", "tailscale0"]            # the cable and Tailscale, not Wi-Fi or containers
listen = ["192.0.2.10", "fd7a:115c::1"]    # these addresses
```

An interface stands for the addresses it has. One that has none yet, like a VPN that comes up
after the service, is listened on within seconds of getting one, and an address that goes away
is let go (connections on it stay until they end); the journal says each. So does an address
the computer doesn't have yet. `"0.0.0.0"` covers every IPv4 address. A drop-in's `listen`
replaces the one before rather than adding to it.

`elamond pair` puts the addresses the service listens on in its QR code, the local network first
(then names, [below](#names-in-the-pairing-link)), and looks for the running service at one of
them. Discovery (`mdns`) only announces the service on the wired and wireless interfaces among
them, never over a VPN or on a bridge for containers: over Tailscale alone, add the computer by
address, name or QR code.

Its state lives in `~/.local/share/elamon`: its id and certificate, the certificates before it
with their signatures (`rotations.json`, [section 10](#a-new-certificate)), the paired phones
(only hashes of their keys, their roles and profiles, and the app each last said it is), a pending
pairing code, the activity log and `elamond control off`'s mark. Deleting that folder unpairs every phone and gives
the service a new certificate, unless a backup of it is restored ([section 8](#backing-up-the-service)).

Where these live follows `XDG_CONFIG_HOME` and `XDG_DATA_HOME`. `ELAMON_CONFIG_DIR` and
`ELAMON_DATA_DIR` name the two folders outright, `ELAMON_CONFIG` the settings file alone, and
`ELAMON_PCI_IDS` the PCI database the service takes graphics card names from.

### Names in the pairing link

Addresses change with networks and leases; a name stays. After its addresses, `elamond pair` puts
host names in the QR code, which the app keeps and tries as well:

- those you give in `names`, in their order:

  ```toml
  names = ["deskpc.example.net", "deskpc.lan"]
  ```

- then the computer's Tailscale name (like `deskpc.tail1234.ts.net`), when Tailscale runs with
  MagicDNS on and the service listens on the computer's Tailscale address (`listen` is `"::"` or
  names `tailscale0`). `elamond pair` asks `tailscale status` for it, and goes on without it when
  Tailscale isn't there or doesn't answer within a second and a half.

The computer's `.local` name isn't added by itself, as Android looks such names up poorly; put it
in `names` if your phone finds it. `elamond pair` prints the names on a `Names:` line. The QR code
carries 8 addresses and names at most, addresses first; what doesn't fit is printed as left out.
A name that leads to another computer does no harm: the app accepts only the computer with the id
and the certificate it paired with. A drop-in's `names` replaces the one before.

### NixOS module

| Option | |
|---|---|
| `services.elamond.enable` | run the service for the people who log in (never for system users such as a display manager's greeter) |
| `services.elamond.users` | only these users run it; empty means every regular user. One user's service holds the port at a time |
| `services.elamond.package` | the service's package |
| `services.elamond.settings` | `elamond.toml` for every user's service, as Nix (below); `elamond pair` and the other commands read it too. `elamond check` reads it as it is built, so a mistake fails `nixos-rebuild` with its line. A change restarts the services. It is in the Nix store, readable by every user: no secrets |
| `services.elamond.openFirewall` | open the service's port (`settings.port`, 47823 by default) and UDP 5353 (unless `settings.mdns = false`); works without `enable` |
| `services.elamond.firewallInterfaces` | with `openFirewall`, open them on these interfaces only, UDP 5353 only on those not named like a VPN (`tailscale*`, `wg*`, `nordlynx*`, `tun*`, `tap*`, `zt*`, `ppp*`; a guess by the name, so a VPN named otherwise gets it too); empty means every interface |
| `services.elamond.control.users` | users who may use power and the profile without a password, so their service can; polkit allows it every program of theirs, not only the service |
| `services.elamond.control.units` | system units those users may start, stop and restart without a password, beside those `settings.links` name with `scope = "system"`, which the rule has already: list here the units of links in a user's drop-in |
| `services.elamond.control.multipleSessions` | `false` by default; also power off, restart and sleep while other users are logged in |

The module's user unit has a `PATH` with `/run/wrappers/bin` (sudo, pkexec), your profile
(`/etc/profiles/per-user/YOUR_USER/bin`, `~/.nix-profile/bin`) and the system's
(`/run/current-system/sw/bin`), for `wpctl` or `pactl` and for actions.

The settings in Nix are the TOML of docs/config.md as attribute sets: `[dashboards.power]` is
`dashboards.power`, `[[dashboards.power.widgets]]` a list of attribute sets:

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
| `services.elamond.settings` | written to `~/.config/elamon/elamond.toml`, as Nix as for NixOS; checked by `elamond check` as it is built, so a mistake fails `home-manager switch`. A change restarts the service. Drop-ins in `elamond.d` still add to it. It is in the Nix store, readable by every user: no secrets |

The firewall stays the system's: with a `port` of your own here, give the system the same
(`services.elamond.settings.port` beside `openFirewall`; without `enable` it only opens that port),
or open it yourself.

## 8. Updating, moving and removing

### Updating

Put the new service in place and run `elamond setup` again, which sees the new binary and restarts
the service:

- **Release binary:** unpack it and install it over the old one as in
  [section 1](#1-install-the-service), then `elamond setup`.
- **From source:** `git pull`, `cargo build --release` in `elamond/`, install the binary again,
  then `elamond setup`.
- **Nix profile:** `nix profile upgrade --all`, then `elamond setup`.
- **NixOS / Home Manager:** `nix flake update elamon`, then rebuild; the rebuild restarts the
  service.

The app and the service of one version go together: app 0.6 and service 0.6 speak API 2, which no
earlier app or service does, and each asks for the other to be updated when they don't match.
Update both. Pairings stay: the phones keep their keys and roles and pair nothing again.

**From 0.5 to 0.6** the settings change once (settings schema 2). The 0.6 service doesn't start
with 0.5's `[[links]]`, `[[actions]]` and `units`; `elamond check` names each line and what
replaces it, and `elamond edit` lets you fix them with the check before saving:

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

On NixOS and Home Manager the same in `services.elamond.settings` (`links.jellyfin = { … };`),
which a rebuild checks. The links the 0.5 app added move into the settings by themselves
([section 5](#links)); the app's own display settings are the computer's profiles now
([section 7](#profiles)). From 0.4 to 0.5 each phone paired again (then `elamond revoke` its old
entry).

### Backing up the service

What makes the service the same computer to its phones is in `~/.local/share/elamon`: its id, its
certificate and key with the rotations behind them, the paired phones (hashes of their keys, their
roles and profiles), the activity log and `elamond control off`'s mark (and the links the 0.5 app
added, while they haven't moved into the settings). Lose
it, and every phone has to pair again. Keep a copy:

```sh
elamond backup ~/elamond.backup
```

It asks for a passphrase twice, at least 8 characters (a few words are best), and writes one file
only you can read, encrypted with it: Argon2id makes the key from the passphrase (64 MiB of memory,
3 passes, 4 lanes), XChaCha20-Poly1305 seals the rest and authenticates the header. Keep the file
and the passphrase apart: without the passphrase the backup can't be opened, and whoever has both
can pose as this computer to its phones. The backup also carries the settings, `elamond.toml` and
the drop-ins in `elamond.d`, which a restore puts back only when asked.

For a script or a timer, give the passphrase in a file only you can read (`chmod 600`):
`elamond backup /backups/elamond.backup --passphrase-file ~/.local/share/keys/elamond-passphrase`.
Keep that file away from the backups: not beside them, not in a folder that is backed up or synced
with them, and not in `~/.config/elamon` either, whose settings may travel with a backup of your
home. Whoever gets both has the computer's identity. `--plain` writes the backup unencrypted, with a
warning: only for a copy that stays in a private folder, never one that goes to cloud storage or
another computer.

A backup holds the certificate's key as it was then. After `elamond rotate-cert`, older backups still
hold the old key; delete those you don't need, all of them if it was rotated because the key may
have leaked ([section 10](#a-new-certificate)).

### A new computer, or reinstalling

Back up first. On the new or reinstalled system install the service ([section 1](#1-install-the-service)),
and then:

```sh
systemctl --user stop elamond
elamond restore ~/elamond.backup
systemctl --user start elamond
```

The phones connect as before, without pairing again: the service has its old id and certificate
and knows their keys. They look for it at the addresses and names they already know, so give the
new computer the old one's address (a DHCP reservation does it) or keep a name they know: its
Tailscale name, or one in `names` ([section 7](#names-in-the-pairing-link)). A phone that can't
reach it any other way has to pair again.

`elamond restore` checks everything in the backup before it writes anything, and refuses a wrong
passphrase, a file that was changed or damaged, a backup from a newer elamond (update the service
first) and, unless told `--force`, a running service. Each file it replaces or takes away it keeps
beside as `NAME.before-restore-TIME` (old keys among them: delete those once all is well), and it
prints what it restored: the computer's id and certificate, the phones and the links.
`--with-settings` also puts back `elamond.toml` and the drop-ins in `elamond.d`, though never over a
file NixOS or Home Manager make: set those in `services.elamond.settings`.

On a computer that has phones paired already, a restore would bring back the backup's phones that
aren't paired there now, among them any you unpaired since, lost or stolen ones too. It lists them
and stops; `--with-old-phones` restores them all the same. Control held off by
`elamond control off` stays held off, whatever the backup says. And a backup whose certificate was
replaced here by `elamond rotate-cert` is restored with a warning: if it was replaced because the
key may have leaked, don't use that backup.

`elamond restore --force` writes under a running service. That service presents the restored
certificate within a few seconds and knows the restored phones at once, but keeps the id, links
and activity log it started with until it restarts: restart it right after,
`systemctl --user restart elamond`.

### A new phone

On the old phone, *Settings → Export pairings*: a passphrase twice, at least 8 characters, then
where to save the file (`elamon-pairings-DATE.elamon`). The file holds every paired computer with
the phone's key to it, encrypted with the passphrase: PBKDF2-HMAC-SHA256 with 600,000 iterations
makes the key, AES-256-GCM seals the list and authenticates the header. The keys are taken out of
Android's key store for this alone and never written anywhere unencrypted. Copy the file to the new
phone, install Elamon there and choose *Settings → Import pairings*, the file and its passphrase. A
computer the new phone has already is replaced, the others are added, and the app says how many.

Each computer gives the new phone a key of its own the first time the app reaches it after the
import: a new entry in `elamond clients` with the new phone's name and the old phone's role, listed
as *added … with the key of* the old phone's ID. One token gives one such key: the entry that gave
it, and the new one, can't ask again, so a file imported on several phones gives a key to the first
of them; pair the others. The old phone keeps working until it is revoked, and revoking it leaves
the new one paired. Moving for good, forget the computers on the old phone, or reset it, and
revoke its entry:

```sh
elamond clients
elamond revoke OLD_ID
```

`elamond revoke` names, loudly, the phones still paired that got a key of their own with the token
it revoked: a phone you don't know there got its key from that one. The two are a family to the
computer, which shares one rate of acting and one share of the activity log between them.
`elamond revoke OLD_ID --with-added` unpairs the whole family, by the old ID even after it is gone:
the way to cut off a phone that was lost or stolen, and whatever got a key with its token. A
computer that takes no more phones (64), or where another phone the same file was imported on got
the old phone's one key of its own already, can't give a key of its own; it sees the new phone as
the old one, one entry for both, and revoking it cuts off both. Pair the new phone again there
(scan the QR code of `elamond pair`; the app replaces its entry) before revoking the old one.

To remove the service: `systemctl --user disable --now elamond`, delete the binary and the unit
file, and `~/.config/elamon` and `~/.local/share/elamon`. In the app, *Forget computer* in the
computer's menu.

## 9. Troubleshooting

| What happens | What to do |
|---|---|
| Something doesn't work, and you don't know what | `elamond setup --check`: a line for each thing the service needs, with what to do about those marked `!`. |
| The computer isn't under *Found on this network* | Same network, UDP 5353 open on the computer; VPNs and many guest Wi-Fi networks block discovery, and the service only announces itself on wired and wireless interfaces it listens on (`mdns: announced on …` in the journal). Add it by address or name: `elamond pair` prints them. |
| The app can't connect | `elamond setup --check`, `systemctl --user status elamond`, `journalctl --user -u elamond -e`; TCP 47823 open; the address reachable from the phone, and one the service listens on (`listen: on …` in the journal). |
| It works only while you're logged in | `sudo loginctl enable-linger YOUR_USER`, as `elamond setup` prints it. |
| *The computer's certificate changed* | The service's data was deleted or the system reinstalled; restore a backup of it ([section 8](#a-new-computer-or-reinstalling)), or forget the computer and pair again. If neither happened, check the network first. After `elamond rotate-cert` the app follows by itself; after `elamond rotate-cert --no-statement` every phone says this, and pairs again ([section 10](#a-new-certificate)). |
| *Control is off* | `control = true` in `elamond.toml` or a drop-in in `elamond.d`; the service takes it up within a few seconds. If it is set, `elamond control status` tells whether `elamond control off` holds it off, or whether the settings stopped parsing, which holds it off until they parse again. |
| Pairing ends with the phone turned away | `elamond pair` asks before it lets a phone in: answer `y` there within 90 seconds, and keep it running until the phone is paired ([section 3](#3-pair-the-phone)). |
| *Not allowed* | The phone's role doesn't allow that: `elamond clients` shows it, `elamond clients role ID ROLE` changes it ([section 6](#6-roles-who-may-do-what)); an action may ask for `admin` (`role` in its settings). |
| *Too many requests* | The phone asked for more than 10 things in a row; wait a few seconds. When pairing: five wrong codes came from this address; run `elamond pair` again for a new code. |
| The phone keeps dropping off | The journal has a line for each stream a phone opens and closes, with its address (local network or VPN), how long it lasted and why it ended: `journalctl --user -u elamond -e`. |
| *needs permission*, or *The computer didn't allow that* | The polkit rule from section 5: `elamond setup` prints the command that installs it, `elamond setup --check` tells whether it is there. |
| `elamond setup` says the user manager doesn't answer | Run it in a session of your own, logged in at the computer or over ssh, not through `su` or `sudo`. |
| `elamond setup` says systemd isn't running | Another init system runs the computer: have it run `elamond serve` as your user at login. |
| A link shows *Local only* | It listens on 127.0.0.1; see section 5. |
| A link shows *No such unit* | The `unit` name is wrong, or the unit isn't installed; `systemctl list-units` (add `--user` for user units). |
| No temperatures or fans | `sudo sensors-detect` (lm-sensors), then restart the service. |
| A graphics card shows no load | NVIDIA needs its own driver (the service reads its management library). Intel doesn't report load without extra privileges; the app shows the clock. |
| A monitor shows no resolution | It's off or blanked, or the service can't open the graphics device: be logged in at the computer or in the `video` group. |
| Lock does nothing | Your desktop's locker doesn't react to logind's lock signal; see section 5. |

To see what the service finds, without the app: `elamond snapshot`.

## 10. Security

- Everything goes over TLS. The service's certificate is self-signed; the app pins it when pairing
  and refuses any other afterwards, but for one the pinned certificate's key signed
  (`elamond rotate-cert`, [below](#a-new-certificate)).
- Each phone holds its own random 256-bit key; the service keeps only its SHA-256. Keys stay out of
  Android backups.
- A pairing code works for five minutes, one attempt at a time, each wrong one answered after a
  second; five wrong ones shut that address out, twenty in all use the code up. The phone that
  gives the right code is let in only when you say so at `elamond pair` (unless it runs with
  `--yes`).
- Without a key the service answers only its name (`hello`), pairing, and the statements that
  lead from one certificate to the next (`identity/rotations`), which prove themselves.
- It holds 256 connections at once, 16 from one address until one shows a phone's key, and drops a
  connection that says nothing within seconds, so a flood of them doesn't keep the phones out.
- The key is all the computer knows a phone by. What the app asks before it uses it (unlocking the
  phone) only the app enforces: a key copied off a phone works without it. That is why
  `elamond revoke` matters: it is the one thing that stops a key. A lost or stolen phone:
  `elamond revoke ID --with-added`.
- Each phone sees and does only what its role allows; give phones that aren't yours `glance` or
  `monitor` ([section 6](#6-roles-who-may-do-what)).
- Unless `control = true`, and no `elamond control off`, the service doesn't act on the computer:
  phones only read, and may keep their list of links. With control on it acts as your user;
  commands can only come from the settings files, never from a phone, and the units a phone may
  start and stop only from them too; links open only web pages; it never signals itself, its
  parents or PID 1, and with the start time the app sends, never a process that took the pid of
  the one meant; a phone acts at most 10 times in a row, then once a second. Settings that stop
  parsing hold control off until they parse again.
- Every request to act is in the activity log, carried out or refused, and what was carried out
  in the journal with the name of the phone that asked: `journalctl --user -u elamond`.
- `elamond revoke` cuts a phone off at once, open streams included.
- A backup (`elamond backup`) and its passphrase together are the computer's identity; it is
  encrypted unless made with `--plain`, and written for your eyes only. Exported pairings are
  encrypted with their own passphrase; a phone that imports them asks each computer for a key of
  its own at its first connection, beside the exporting phone's, which stays until revoked
  ([section 8](#a-new-phone)). Any phone paired with a code may get one such key, once, with its
  own role and no more; `elamond clients` and `elamond revoke` say whose key added it, the activity
  log has it, and `elamond revoke ID --with-added` cuts off both. At most 64 phones are paired at
  once.
- An old key lives on in backups and in files a restore set aside; `elamond restore` never lets go
  of control held off, and brings back phones unpaired since only with `--with-old-phones`.

### A new certificate

```sh
elamond rotate-cert
```

gives the service a new certificate and key, from time to time. Before the old key is deleted it
signs the new certificate's fingerprint, and the service shows that signature to any phone that
asks. A phone that meets the new certificate checks the signature against
the certificate it pinned and moves to the new one by itself, saying *The computer's certificate
was renewed*; nothing needs pairing again. The running service presents the new certificate within
a few seconds (at once with `systemctl --user reload elamond`); connections already open keep the
old one until they end.

The last five rotations are kept, so a phone that was away for one or two still follows. One that
missed more says *The computer's certificate changed*: pair it again.
Deleting `~/.local/share/elamon` is not a rotation: it unpairs every phone.

That is routine renewal, and only that. If the key may have been copied off the computer, a phone
mustn't follow what it signs: whoever has it could sign their own certificate. Then:

```sh
elamond rotate-cert --no-statement   # a new certificate nothing leads to; the old key deleted unused
elamond revoke all                   # the keys phones sent to a computer that may not have been this one
elamond pair                         # each phone again; the app replaces the computer it had
```

Every phone then says the certificate changed. The old key lives on in backups made before (with
`elamond backup`) and in `key.der.before-restore-*` files a restore left in `~/.local/share/elamon`:
delete them, as `elamond rotate-cert` reminds you.
