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
- [6. Settings reference](#6-settings-reference)
- [7. Updating and removing](#7-updating-and-removing)
- [8. Troubleshooting](#8-troubleshooting)
- [9. Security](#9-security)

## 1. Install the service

### NixOS

Add the flake to your inputs:

```nix
inputs.elamon = {
  url = "github:Elaizes/elamon";
  inputs.nixpkgs.follows = "nixpkgs";
};
```

Import the module and turn the service on:

```nix
imports = [ inputs.elamon.nixosModules.default ];

services.elamond = {
  enable = true;        # runs for everyone who logs in
  openFirewall = true;  # TCP 47823, and UDP 5353 for discovery
};
```

Then `sudo nixos-rebuild switch`. To keep the service running while nobody is logged in, enable
lingering for your user: `users.users.YOUR_USER.linger = true;` or `loginctl enable-linger`.

### Home Manager

For one user, with the firewall still opened by the system (`openFirewall` works without
`enable`):

```nix
imports = [ inputs.elamon.homeManagerModules.default ];

services.elamond.enable = true;
```

### Nix on another distribution

With flakes enabled:

```sh
nix profile install github:Elaizes/elamon
systemctl --user link ~/.nix-profile/lib/systemd/user/elamond.service
systemctl --user enable --now elamond
```

### Fedora, Debian, Ubuntu, Arch, openSUSE and others

The service is built from source. It needs git, a C compiler and Rust 1.88 or newer.

| System | Build tools |
|---|---|
| Fedora | `sudo dnf install git cargo gcc` |
| Arch | `sudo pacman -S --needed git rust base-devel` |
| openSUSE | `sudo zypper install git cargo gcc` |
| Debian, Ubuntu | `sudo apt install git build-essential curl`, then Rust from [rustup](https://rustup.rs), since their own is older than 1.88 |

Build and install it for your user; nothing goes outside your home folder:

```sh
git clone https://github.com/Elaizes/elamon.git
cd elamon/elamond
cargo build --release
install -Dm755 target/release/elamond ~/.local/bin/elamond
install -Dm644 dist/elamond.service ~/.config/systemd/user/elamond.service
sed -i "s|/usr/bin/elamond|$HOME/.local/bin/elamond|" ~/.config/systemd/user/elamond.service
systemctl --user daemon-reload
systemctl --user enable --now elamond
loginctl enable-linger    # keep it running while you're logged out
```

If your shell doesn't find `elamond` afterwards, `~/.local/bin` isn't on your `PATH`; call it as
`~/.local/bin/elamond`.

## 2. Open the firewall

The service listens on **TCP 47823**. **UDP 5353** is mDNS, which lets the app find the computer by
itself; without it, add the computer by address. Over a VPN only 47823 matters, and an interface
the firewall already trusts needs nothing.

| Firewall | Commands |
|---|---|
| NixOS | `services.elamond.openFirewall = true;` |
| firewalld (Fedora, openSUSE) | `sudo firewall-cmd --permanent --add-port=47823/tcp`<br>`sudo firewall-cmd --permanent --add-service=mdns`<br>`sudo firewall-cmd --reload` |
| ufw (Ubuntu, when it's on) | `sudo ufw allow 47823/tcp`<br>`sudo ufw allow 5353/udp` |

Arch and Debian ship with no firewall running; then there's nothing to open.

## 3. Pair the phone

On the computer:

```sh
elamond pair
```

It prints a QR code and a six-digit code, good for five minutes and five wrong attempts.

- **QR code:** point the phone's camera at it and open the link. The link carries the computer's
  address and certificate, so pairing takes one tap.
- **Code:** in the app, tap the computer under *Found on this network*, or *Add computer* and its
  address. The app shows the computer's certificate digits first: they must match the
  `Certificate:` line `elamond pair` printed. Then enter the code.

Each phone gets its own key. `elamond clients` lists the paired phones and `elamond revoke ID`
unpairs one.

## 4. The app

### Home

Every paired computer is a card with its CPU, memory and temperature, refreshed every few seconds
while the screen is open, and a dot for online or offline. Below them, *Found on this network*
lists computers on the local network that run the service and that this phone isn't paired with
yet. Tap a card to open the computer; **Add computer** pairs another one. The **?** opens the
help, the gear the settings.

### A computer

The name, how fresh the data is (*Live · every 1 s*) and the computer's state sit in the big title
at the top; scroll and they move into the bar. Four tabs along the bottom:

- **Overview** — tiles for the CPU, memory, each graphics card, temperatures, network, storage and
  battery. **Tap a tile** for its page: a large chart of the last ten minutes, details, and the
  processes using the most of it (on the CPU page also every core with its clock).
- **Processes** — the heaviest first. **Sort** with the chips (CPU, memory, GPU, disk, name),
  **search** by name, command or PID, **By app** adds up the processes of one program. **Tap a
  process** for its details: user, state, start time, disk and GPU use, the full command, and —
  with control on — **Quit** and **Force kill**.
- **Sensors** — every temperature the computer reports, grouped by device, a chart of the CPU and
  GPU temperatures, and the fans. A limit appears next to a sensor when it gets close to it.
- **Control** — links; the monitors (maker, model, size, current and native mode, highest
  refresh rate, how they're arranged, free ports), shown even when control is off; media, power
  profile, actions and power; see [section 5](#5-the-control-panel).

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

| | |
|---|---|
| Theme | follow the phone, or always light or dark |
| Pure black | black instead of dark grey in dark mode; saves power on OLED screens |
| Wallpaper colours | take the colours from the wallpaper (Android 12+) |
| Blur | frosted glass behind the bars and dialogs (Android 12+) |
| Animations | rolling numbers and moving gauges |
| Update every | 1, 2 or 5 seconds; slower saves data on mobile networks |
| Temperature | °C or °F |
| Network speed | bytes per second, or bits per second as providers state it |
| Keep screen on | while a computer is open, to use the phone as a dashboard |
| Confirm actions | ask before power, kill, restart and commands |
| Haptic feedback | a tick on long presses |

### How do I…

- **…see what slows the computer down?** Overview → tap the CPU or Memory tile: the bottom of the
  page lists the processes using the most. Or the Processes tab, sorted by CPU or memory.
- **…close a program that hangs?** Processes → find it → tap → *Quit*; if it doesn't go, *Force
  kill*. (Needs control.)
- **…check how hot it runs?** The Temperature tile, or Sensors for every sensor and the fans.
- **…open a web page the computer hosts?** Control → tap its tile. Over a VPN the link uses the
  VPN address by itself.
- **…restart a web app that stopped answering?** Control → hold its link → *Restart*.
- **…add a link?** Control → **+**. Pick one of the ports the computer listens on, or type the
  address; add its systemd unit to start and stop it from here.
- **…pause the music?** Control → Media: play, pause, skip, seek, and the computer's volume.
- **…make it quieter or faster?** Control → Power profile.
- **…run my own script from the phone?** Add it as an `[[actions]]` entry on the computer; it
  shows up as a button under Actions.
- **…turn it off?** Control → Power → *Shut down*. Only someone at the computer can turn it on
  again, unless it wakes on LAN.
- **…watch several computers?** Pair each one; the home screen shows them all.
- **…keep it on screen as a dashboard?** Settings → *Keep screen on*.
- **…stop a phone from seeing a computer?** On the computer `elamond clients`, then
  `elamond revoke ID`; on the phone, ⋮ → *Forget computer*.

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
file the module builds, and restarts when they change. Anything you want to add by hand goes in
a drop-in beside it, `~/.config/elamon/elamond.d/*.toml` ([section 6](#6-settings-reference)).

### What comes from where

| | Found by the service | Set on the computer | Added in the app |
|---|---|---|---|
| Power: lock, sleep, restart, shut down | which of them the system allows | permission (polkit, below) | — |
| Power profile | profiles of power-profiles-daemon | permission (polkit) | — |
| Media and volume | any MPRIS player that's running; the default output through `wpctl` or `pactl` | — | — |
| Quitting and killing processes | — | — | — |
| Links | ports in use and systemd units, as suggestions | `[[links]]` in `elamond.toml` | the **+** button |
| Commands | — | `[[actions]]` in `elamond.toml`, and only there | — |

### Links

A link opens a web page the computer hosts. Each is a tile with its state: *Running*, *Stopped*,
*Failed*, *Local only* or *No such unit*. Tap to open it in the browser; hold it for start, stop
and restart.

```toml
[[links]]
name = "Media server"
url = "http://{host}:8096"    # {host}: the address the phone reached the computer at
unit = "SERVICE.service"      # optional: lets the app start, stop and restart it
scope = "system"              # or "user" (the default)
```

- `{host}` makes the link work on the local network and over a VPN alike. `localhost` and
  `127.0.0.1` in a URL are replaced the same way.
- *Local only* means what the link points to listens on the computer's own address (127.0.0.1):
  other devices can't open it. The app warns before trying. Make it listen on all addresses, or
  reach it through an SSH tunnel.
- Links from `elamond.toml` are fixed in the app; change them in the file. Links added in the app
  are stored on the computer (`~/.local/share/elamon/links.json`), so every paired phone sees
  them, and any of them can change or remove them.
- Before 0.5 links were called services: `[[services]]` in the settings still works and means
  `[[links]]`, and `services.json` is read until the first change writes `links.json`.

### Commands

```toml
[[actions]]
name = "Back up now"
command = ["/path/to/backup-script", "--now"]   # run without a shell
confirm = true      # ask in the app first (default)
timeout = 600       # seconds before it's stopped (default 60)
```

Each becomes a button; what it prints comes back to the phone. Commands can only be defined on the
computer, so a phone can't run anything that isn't listed. They run as your user, with the service's
environment.

### Power, the profile and system units: polkit

The service runs outside a login session, so polkit asks for a password it can't give for powering
off, restarting, sleeping, switching the power profile and managing **system** units. The app
marks those *needs permission*. User units need no permission.

On NixOS the module writes the rule:

```nix
services.elamond.control = {
  users = [ "YOUR_USER" ];            # power and the profile without a password
  units = [ "SERVICE.service" ];      # system units these users may start, stop and restart
};
```

Elsewhere, save this as `/etc/polkit-1/rules.d/50-elamon.rules`, with your user name and units:

```js
polkit.addRule(function (action, subject) {
  if (subject.user != "YOUR_USER") return polkit.Result.NOT_HANDLED;
  var id = action.id;
  if (id.indexOf("org.freedesktop.login1.power-off") == 0 ||
      id.indexOf("org.freedesktop.login1.reboot") == 0 ||
      id.indexOf("org.freedesktop.login1.suspend") == 0 ||
      id == "org.freedesktop.UPower.PowerProfiles.switch-profile" ||
      id == "net.hadess.PowerProfiles.switch-profile")
    return polkit.Result.YES;
  if (id == "org.freedesktop.systemd1.manage-units" &&
      ["SERVICE.service"].indexOf(action.lookup("unit")) >= 0)
    return polkit.Result.YES;
  return polkit.Result.NOT_HANDLED;
});
```

*Lock* locks every session of your user through logind; your desktop's locker has to respond to
that (GNOME and KDE do; on wlroots compositors, set up e.g. hypridle's `lock_cmd`).

## 6. Settings reference

### `elamond.toml`

`~/.config/elamon/elamond.toml`, or the file `ELAMON_CONFIG` names, then every
`~/.config/elamon/elamond.d/*.toml` in name order. A later file adds to `[[links]]` and
`[[actions]]` and replaces any other key it sets. Under NixOS or Home Manager, where
`elamond.toml` comes from Nix and can't be edited, the drop-ins are where your own additions go.
All keys are optional.

| Key | Default | |
|---|---|---|
| `port` | `47823` | TCP port |
| `listen` | `"::"` | address to listen on; `"::"` covers IPv4 and IPv6 |
| `name` | host name | the name shown in the app |
| `mdns` | `true` | announce the service on the local network |
| `interval_ms` | `1000` | time between readings, 250–10 000 |
| `history_minutes` | `10` | history a phone gets when it connects (at most 7 200 readings) |
| `control` | `false` | allow the control panel |
| `[[links]]` | | `name`, `url`, `unit`, `scope` — see above; `[[services]]` before 0.5 |
| `[[actions]]` | | `name`, `command`, `confirm`, `timeout` (seconds, at most 3 600) — see above |

The service rereads the files when one of them changes, or on `systemctl --user reload elamond`. A
file that doesn't parse is named in the log and the running settings stay. `control`, `links`
and `actions` apply at once; `port`, `listen`, `name`, `mdns`, `interval_ms` and `history_minutes`
after a restart.

Its state lives in `~/.local/share/elamon`: its id and certificate, the paired phones (only
hashes of their keys), links added in the app and a pending pairing code. Deleting that folder
unpairs every phone and gives the service a new certificate.

Where these live follows `XDG_CONFIG_HOME` and `XDG_DATA_HOME`. `ELAMON_CONFIG_DIR` and
`ELAMON_DATA_DIR` name the two folders outright, `ELAMON_CONFIG` the settings file alone, and
`ELAMON_PCI_IDS` the PCI database the service takes graphics card names from.

Before 0.5 the settings were `elamond.toml` and `elamond.d/`. They are still read: `elamond.toml` while
there is no `elamond.toml`, and `elamond.d/*.toml` before `elamond.d/*.toml`. The log says once what
to rename.

### NixOS module

| Option | |
|---|---|
| `services.elamond.enable` | run the service for the people who log in (never for system users such as a display manager's greeter) |
| `services.elamond.users` | only these users run it; empty means every regular user. One user's service holds the port at a time |
| `services.elamond.package` | the service's package |
| `services.elamond.settings` | `elamond.toml` for every user's service, as Nix; `elamond pair` and the other commands read it too |
| `services.elamond.openFirewall` | open the service's port (`settings.port`, 47823 by default) and UDP 5353; works without `enable` |
| `services.elamond.control.users` | users whose service may use power and the profile without a password |
| `services.elamond.control.units` | system units those users may start, stop and restart |

### Home Manager module

| Option | |
|---|---|
| `services.elamond.enable` | run the service for this user |
| `services.elamond.package` | the service's package |
| `services.elamond.settings` | written to `~/.config/elamon/elamond.toml`; a change restarts the service. Drop-ins in `elamond.d` still add to it |

## 7. Updating and removing

Update the app and the service together; this is a test version and they change in step.

- **NixOS / Home Manager:** `nix flake update elamon`, then rebuild.
- **Nix profile:** `nix profile upgrade --all`, then `systemctl --user daemon-reload` and
  `systemctl --user restart elamond`.
- **From source:** `git pull`, `cargo build --release` in `elamond/` (`elamond/` before 0.5),
  install the binary again and restart the service.

Pairings and links added in the app survive updates.

To remove the service: `systemctl --user disable --now elamond`, delete the binary and the unit
file, and `~/.config/elamon` and `~/.local/share/elamon`. In the app, *Forget computer* in the
computer's menu.

## 8. Troubleshooting

| What happens | What to do |
|---|---|
| The computer isn't under *Found on this network* | Same network, UDP 5353 open on the computer; VPNs and many guest Wi-Fi networks block discovery. Add it by address: `elamond pair` prints them. |
| The app can't connect | `systemctl --user status elamond`, `journalctl --user -u elamond -e`; TCP 47823 open; the address reachable from the phone. |
| It works only while you're logged in | `loginctl enable-linger` |
| *The computer's certificate changed* | The service's data was deleted or the system reinstalled; forget the computer and pair again. If neither happened, check the network first. |
| *Control is off* | `control = true` in `elamond.toml` or a drop-in in `elamond.d`; the service takes it up within a few seconds. |
| *needs permission*, or *The computer didn't allow that* | The polkit rule from section 5. |
| A link shows *Local only* | It listens on 127.0.0.1; see section 5. |
| A link shows *No such unit* | The `unit` name is wrong, or the unit isn't installed; `systemctl list-units` (add `--user` for user units). |
| No temperatures or fans | `sudo sensors-detect` (lm-sensors), then restart the service. |
| A graphics card shows no load | NVIDIA needs its own driver (the service reads its management library). Intel doesn't report load without extra privileges; the app shows the clock. |
| A monitor shows no resolution | It's off or blanked, or the service can't open the graphics device: be logged in at the computer or in the `video` group. |
| Lock does nothing | Your desktop's locker doesn't react to logind's lock signal; see section 5. |

To see what the service finds, without the app: `elamond snapshot`.

## 9. Security

- Everything goes over TLS. The service's certificate is self-signed; the app pins it when pairing
  and refuses any other afterwards.
- Each phone holds its own random 256-bit key; the service keeps only its SHA-256. Keys stay out of
  Android backups.
- A pairing code works for five minutes and five attempts, one attempt at a time.
- Without a key the service answers only its name (`hello`) and pairing.
- Unless `control = true`, the service doesn't act on the computer: phones only read, and may
  keep their list of links. With control on it acts as your user; commands can only come from
  the settings files, never from a phone; it never signals itself, its parents or PID 1; and
  every action is logged with the name of the phone that asked: `journalctl --user -u elamond`.
- `elamond revoke` cuts a phone off at once, open streams included.
