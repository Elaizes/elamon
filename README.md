# Elamon

Watch and control a Linux computer from your Android phone or a browser, live.

It has two parts:

- **Elamon**, the Android app (Android 8 and newer). No Google services, no trackers, no ads.
- **elamond**, the Elamon service: a small program on the computer (Linux with systemd, any
  desktop or none). It reads the computer's own counters and serves them over HTTPS, only to the
  phones and browsers you paired with it. It also serves a web page that shows the same as the app.

**Setting it up and using it: [the user guide](docs/guide.md).**

> **A personal project.** Elamon is made for its author's own use, on NixOS, and what comes next is
> decided by that, not by requests. Nothing in it is tied to one computer, though: every change is
> built and tested on NixOS (a VM test of the module) and on Debian, Ubuntu, Fedora, Arch and
> openSUSE Tumbleweed (the service installed and started in each). Use it if it fits. There is no
> support beyond the guide, and the settings and the protocol may still change between versions:
> update the app and the service together.

## What it does

### Watch

| | |
|---|---|
| **The computer** | CPU, memory, each graphics card, temperatures and fans, network, storage, battery; each with a page of its own and charts you read with a finger |
| **Processes** | sortable and searchable, with the ones using the most on every page |
| **History** | charts from the last ten minutes back to everything the service has kept on disk |
| **Monitors** | the connected ones with their modes and arrangement, under any desktop |
| **A UPS or an EcoFlow power station** | through NUT: status, charge and time left, power in and out, each outlet, and a timeline of when it was on battery |
| **Your own readings** | a program's output, a web service's answer or a file's contents, shown with the unit and format you give |
| **The journal** | the system journal, warnings first, with search; and the journal of one service |
| **Events** | rules tell of a disk filling, a unit that failed, a low battery, a UPS on battery, or any value of yours past its limit. The service keeps them, the app lists them, and it can send them to an [ntfy](https://ntfy.sh) topic, so the phone notifies you with Elamon closed |

### Control

Off until the computer's settings allow it (`control = true`). Then:

| | |
|---|---|
| **Links** | open the web pages the computer hosts, see if they answer, start, stop and restart their units |
| **Media** | players and the volume |
| **Power** | the power profile, lock, sleep, restart, shut down |
| **Your own commands** | defined on the computer, with a parameter picked from the values you allow. A phone only presses the button; it never writes a command |
| **Processes** | quit and kill |
| **Monitors under Hyprland** | each on or off, asleep or awake, its brightness (a laptop's panel, or an external monitor over DDC/CI) |
| **A power station** | its outlets and settings (charge limits, charging power, X-Boost…), only those the computer's settings list. Cutting the computer's own power has to be typed in first |

### The settings are files on the computer

Every screen is a dashboard of widgets that the service describes. The standard ones (Overview,
Processes, Sensors, Control, a tab for each UPS, the Journal) are in the settings the service starts
with, in `~/.config/elamon` or in Nix. Change them or add your own, with any reading, table, graph,
button or choice, without updating the app: [settings reference](docs/config.md). How the app looks
on each phone, and which tabs it has, is that phone's profile, set on the computer too.

### Who may do what

Each phone or browser has a role, given at the computer:

| Role | May |
|---|---|
| `glance` | see the basics |
| `monitor` | see everything |
| `operate` | everyday control |
| `admin` | everything a phone can do |
| `owner` | also change the settings from the page or the app; only for your own devices |

Every request to act is kept in an activity log, and `elamond control off` stops every phone and
browser at once.

### In a browser

The service's own web page pairs with the same code as the app, shows the same dashboards and runs
the same actions. On a wide window it has a side bar and fills the window; its theme and layout are
picked in the browser. With the role `owner` it also has:

- an **admin panel**: the service's state, the merged settings and where each part comes from, the
  paired clients and their roles, pairing, holding control off, and the settings files (checked
  before they're saved, and backed up);
- a **dashboard editor**, in the page and in the app: forms the service describes, saved to a file
  of its own. It never writes a command.

### On the phone

- Material You with frosted glass on Android 12 and newer; light, dark or pure black.
- The app opens on the computer you opened last.
- Two things belong to the phone, not the computer: the app's scale (80 to 140%) and, if you want
  one, a theme that stays whatever the profiles say.
- A home-screen widget with the computer's figures (gauges as small rings) and four Quick Settings
  tiles that run its actions. Which ones, the computer's profile says.

### Moving

`elamond backup` saves the computer's identity and its paired phones in one file only you can read.
`elamond restore` puts it back on a reinstalled or new computer, and the phones connect without
pairing again. A new phone pairs with `elamond pair`.

## Quick start

1. **Install the service.** On NixOS, from this flake (Home Manager has a module too):

   ```nix
   imports = [ inputs.elamon.nixosModules.default ];
   services.elamond = {
     enable = true;
     openFirewall = true;
   };
   ```

   On another distribution with systemd and glibc 2.28 or newer:

   ```sh
   curl -fsSL https://raw.githubusercontent.com/Elaizes/elamon/main/install.sh | sh
   ```

   This puts the newest release's `elamond` in `~/.local/bin` (its checksum checked) and runs
   `elamond setup`, which installs it for your user. Then run, as root, the commands it prints:
   lingering, the firewall, the polkit rule. `elamond update` installs a newer release later.
   Installing by hand or from source: [section 1 of the guide](docs/guide.md#1-install-the-service).

2. **Pair the phone.** Run `elamond pair` on the computer. Point the phone's camera at the QR code
   it prints, or enter its six-digit code in the app. Then answer `y` on the computer
   ([details](docs/guide.md#3-pair-the-phone)).

That is enough to watch. The rest is optional, each a few lines in the settings:

| For | Add | Guide |
|---|---|---|
| the control panel | `control = true` | [section 5](docs/guide.md#5-the-control-panel) |
| a UPS or a power station | a source, `[sources.<id>]`, with what phones may switch on it | [section 7](docs/guide.md#7-upses-and-power-stations) |
| readings of your own | sources of the kind `command`, `http` or `file` | [section 8](docs/guide.md#8-your-own-readings) |
| notifications | an ntfy topic's file, `[notify.<id>]` | [section 10](docs/guide.md#10-events-and-notifications) |
| a browser | nothing: open `https://<computer>:47823/` and type in the code of `elamond pair` | [section 11](docs/guide.md#11-in-a-browser) |

## How it connects

- Phones and browsers talk to the computer **directly**, over the local network or a VPN they
  share: TCP 47823, and UDP 5353 for the app to find the computer by itself.
- Everything is encrypted. A phone or a browser is paired only when you let it in at the computer.
  Each has its own key, which the computer can revoke.
- The service acts on the computer only when its settings say it may, and starts and stops only
  the units its settings name ([security](docs/guide.md#17-security)).
- It can listen on chosen interfaces only (`listen = ["tailscale0"]`), and announces itself only on
  the local network.
- The pairing QR code carries the computer's addresses, then its Tailscale name and the names you
  give (`names = [...]`), so the phone still finds it when addresses change.
- Nothing goes through anyone's server, with one exception that is yours to make: name an ntfy
  topic, and the service posts its events to that ntfy server, without the names of units, disks
  or the computer unless you ask for them.

## Technical notes

- A discrete GPU that has powered itself down is reported as asleep, not woken up to be read.
- NVIDIA GPUs are read through NVML, AMD and Intel through sysfs. Intel graphics don't report
  their load without extra privileges (perf); the app shows the clock instead.
- Per-process GPU use comes from the kernel's DRM fdinfo, so it covers your own processes.
- Reading a monitor's current mode opens the GPU's DRM node for a moment. With no compositor
  running, whoever opens it first briefly becomes its DRM master; the service closes it right away.
- Power actions, power profiles and units go through D-Bus (logind, power-profiles-daemon,
  systemd), media through MPRIS, the volume through `wpctl` or `pactl`.
- Monitors are turned on and off through Hyprland's own `hyprctl` (its Lua settings), asked only
  while a phone shows them and done once Hyprland shows them done; a laptop panel's brightness
  through logind, an external monitor's through the computer's `ddcutil`, read once a minute at
  most and never while it sleeps.
- A UPS is read over NUT's network protocol, from upsd or, for an EcoFlow station, nutflow; the
  computer needs no NUT client. With `upst record` running, the history and the timeline also
  cover the time the service didn't run.
- Events are made by rules of the settings as the service reads the computer, with or without a
  phone connected, and kept in a file beside its state; each is sent to ntfy by the computer's
  `curl`, the topic on its input and never among its arguments, and tried again while the network
  or the server is down.
- Readings of your own: a command runs without a shell, in a process group the service ends
  with it; a web service is asked through the computer's `curl`, without following redirects; the
  journal is read through `journalctl` as the service's user (the system's in the
  `systemd-journal` group), and only while a phone shows it.
- The web page is files built into elamond as they are, HTML, CSS and JavaScript with nothing
  compiled or bundled, the same on every computer of a version. It runs only its own scripts (its
  Content-Security-Policy allows no other), and a browser keeps its key in an HttpOnly cookie that
  the page's scripts can't read.
- Charts reach back further than the ten minutes in memory: the service keeps what they show on
  disk, every 10 seconds for a day, every 5 minutes for 30 days and every hour from then on.
- The service uses about 0.3 % of one CPU core while no phone watches, 1–2.5 % while one does
  (most of it the process list), and 8–11 MB of memory (resident; systemd also counts page
  cache).

## Building

With Nix, `nix develop` gives the Android SDK, JDK and Rust:

```sh
nix develop -c ./gradlew assembleRelease     # app: app/build/outputs/apk/release/
nix develop -c bash -c 'cd elamond && cargo build --release'
nix build .#elamond                           # the service's package, for x86_64-linux and aarch64-linux
nix develop .#emulator                        # an x86_64 Android emulator for trying the app
```

Without Nix: Android SDK 37, JDK 17, and Rust 1.89 or newer. A release build is signed when
`~/.android/elamon-release.properties` (or the file `-Pelamon.signing` names) gives `storeFile`,
`storePassword`, `keyAlias` and `keyPassword`; otherwise it stays unsigned.

Tests:

```sh
nix develop -c bash -c 'cd elamond && cargo test' # the service, and the protocol fixtures in elamond/fixtures
nix develop -c ./gradlew :app:testDebugUnitTest    # app, against the same fixtures
nix develop -c node --test elamond/web/test/*.test.mjs  # the web page
nix develop -c tsc -p elamond/web/jsconfig.json        # the web page's types
nix develop .#browser -c elamond/web/test/browser.sh   # the page in a headless Chromium, against a throwaway service
nix flake check                                   # VM tests: the NixOS module, a UPS on a mock station, the sources and the journal, events, the monitors, the web page
maestro/run.sh                                    # the app on an emulator only (a real phone: --yes-real-phone), against a throwaway service
```

The protocol between the service and its clients, the app and the page:
[docs/protocol.md](docs/protocol.md); the settings, dashboards and profiles:
[docs/config.md](docs/config.md).

## Where it's going

| Version | Brought |
|---|---|
| 0.7 | UPSes and power stations |
| 0.8 | your own readings, the system journal |
| 0.8.1 | the phone's own scale and theme |
| 0.9 | events, and notifications through ntfy |
| 0.10 | monitors under Hyprland, a home-screen widget, Quick Settings tiles (0.10.1: the widget's rings) |
| 0.11 | the web page with its admin panel, the dashboard editor |
| 0.11.2 | an evener look of the app |
| 0.11.3 | the web page's new colours and its light, dark and black themes |
| 0.11.4 | the page's side bar, the page as wide as the window, a layout picked in the browser |

Next:

- **0.11.5**: the page's admin panel.
- **1.0**: once a month of use goes by without a change to the settings or the protocol.
- **After 1.0**: a certificate the browsers take without a warning.

Not planned: the service for Windows or macOS, and app stores (the app is the APK in each release).

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).

Icons: Material Symbols by Google, Apache License 2.0.

UPS support uses upst-core, the core of upst (Apache-2.0), and works with nutflow for EcoFlow
stations.
