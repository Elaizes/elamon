# Elamon

Watch and control a Linux computer from your Android phone, live.

> **A personal project.** Elamon is made for its author's own use, on NixOS, and grows with what
> that needs: what comes next is decided by that, not by requests. Still, nothing in it is tied to
> one computer. Every change is built and tested on NixOS (a VM test of
> the module) and on Debian, Ubuntu, Fedora, Arch and openSUSE Tumbleweed (the service installed
> and started in each), so it should work on yours too. Use it if it fits; there is no support
> beyond the guide, and settings and the protocol may still change between versions: update the
> app and the service together.

- **Elamon**, the Android app (Android 8 and newer). No Google services, no trackers, no ads.
- **elamond**, the Elamon service, a small program on the computer (Linux, any distribution, any
  desktop or none). It reads the kernel's own counters and serves them over HTTPS, only to phones
  and browsers paired with it.

**Setting it up and using it: [the user guide](docs/guide.md).**

## What it does

Every screen is a dashboard of widgets that the service describes, set in its settings on the
computer (`~/.config/elamon`, or Nix on NixOS): the standard Overview, Processes, Sensors and
Control, a tab for each UPS and a Journal page come in the settings it starts with, and you can
change them or add your own — any reading, table, graph, button or choice — without updating the
app ([settings reference](docs/config.md)). How the app looks and behaves on each phone is that
phone's profile, set on the computer too.

Watching, always:

- a dashboard of the CPU, memory, each graphics card, temperatures, network, storage and battery,
  each with a page of its own: charts you can read with a finger, from the last ten minutes back to
  all the service has kept, and the processes using the most;
- processes, sortable and searchable;
- every temperature sensor and fan;
- connected monitors with their modes and arrangement, read from the kernel, so under any desktop
  (under Hyprland, whether each is on and its brightness too);
- a UPS, or an EcoFlow power station, through NUT: its status, charge and time left, the power in
  and out and each outlet's, the battery, and a timeline of when it was on battery, kept for good
  (its last 10 000 changes), its legend in hours or in shares of the time;
- your own readings: a program's output, a web service's answer or a file's contents, each
  shown with its own unit and format in a dashboard of yours;
- the system journal, warnings first, with search and older lines on request, and the journal of
  a link's unit from its row;
- events: rules in the settings tell of a disk filling, a unit that failed, a low battery, a UPS
  on battery or any value of yours past its limit, once as it begins and once as it ends; the
  service keeps them, the app lists them, and the service sends them to an [ntfy](https://ntfy.sh)
  topic, so the ntfy app notifies you with Elamon closed. Which rules, their limits and words,
  and what is sent (without names, unless you say so) are all settings.

Controlling, once the computer allows it:

- links to the web pages it hosts: open them, see if they answer, start, stop and restart their
  units;
- media players and the volume;
- the power profile, locking, restarting and shutting down;
- commands you define on the computer, with a parameter to choose from the values you allow;
- quitting and killing processes;
- a station's outlets and settings (charge limits, charging power, X-Boost…), those the computer's
  settings list and within the station's limits; one that cuts the computer's own power is typed in
  first.
- monitors under Hyprland: each on or off, its picture asleep or awake, its brightness (a laptop's
  panel, or an external monitor over DDC/CI); turning off the last one that shows a picture asks
  for its name.

In a browser too: the service serves a web page of its own, paired with the same code, that shows
the same dashboards and runs the same actions. Given the role `owner`, your own browser also has an
admin panel (the service's state, the settings merged with where each part comes from, the paired
clients and their roles, pairing, holding control off, and the settings files, checked before
they're saved and backed up) and, in the page and the app, a dashboard editor: forms the service
describes, saved to a drop-in of its own, never a command.

Each phone or browser has a role, set on the computer: glancing at the basics, watching everything,
everyday control, all of it, or, for your own, the settings too. Every request to act is kept in an
activity log, and `elamond control off` stops every phone and browser at once.

Moving is kept simple: `elamond backup` saves the computer's identity and its paired phones in one
file only you can read, which `elamond restore` puts back on a reinstalled or new computer without
pairing the phones again; a new phone pairs with `elamond pair`.

The app: Material You with frosted glass on Android 12 and newer, light, dark or pure black. It
opens on the computer you opened last. Two things are the phone's own, not the computer's: the
app's scale, 80 to 140%, and, if you want one, a theme that stays whatever the profiles say. On
the phone's home screen, a widget with figures of the computer's, its gauges as small rings, and
four Quick Settings tiles that run its actions; which, the computer's profile says.

## Quick start

1. Install the service. On NixOS, from this flake (Home Manager has a module too):

   ```nix
   imports = [ inputs.elamon.nixosModules.default ];
   services.elamond = {
     enable = true;
     openFirewall = true;
   };
   ```

   On another distribution with systemd and glibc 2.28 or newer, this puts the newest release's
   `elamond` in `~/.local/bin`, its checksum checked, and lets `elamond setup` install it for your
   user; then run what it prints as root (lingering, the firewall, the polkit rule):

   ```sh
   curl -fsSL https://raw.githubusercontent.com/Elaizes/elamon/main/install.sh | sh
   ```

   `elamond update` installs a newer release later. By hand, from the
   [releases](https://github.com/Elaizes/elamon/releases) or `cargo`, and the details:
   [section 1 of the guide](docs/guide.md#1-install-the-service).

2. Run `elamond pair` on the computer and point the phone's camera at the QR code it prints, or
   enter its six-digit code in the app, then let the phone in where `elamond pair` asks
   ([details](docs/guide.md#3-pair-the-phone)).

3. For the control panel, add `control = true` to the service's settings
   ([details](docs/guide.md#5-the-control-panel)).

4. For a UPS or a power station, add it to the settings as a source, `[sources.<id>]`, with what
   phones may switch on it ([details](docs/guide.md#7-upses-and-power-stations)).

5. For readings of your own, add sources of the kind `command`, `http` or `file`
   ([details](docs/guide.md#8-your-own-readings)); the system journal is on the Control tab
   ([details](docs/guide.md#9-the-journal)).

6. For notifications, name an ntfy topic's file in the settings, `[notify.<id>]`
   ([details](docs/guide.md#10-events-and-notifications)).

7. For a browser, open `https://<computer>:47823/` and type in the code of `elamond pair`
   ([details](docs/guide.md#11-in-a-browser)).

Phones and browsers talk to the computer directly, over the local network or a VPN they share: TCP
47823, and UDP 5353 for the app to find the computer by itself. The service can listen on chosen
interfaces only (`listen = ["tailscale0"]`), and announces itself only on the local network. Its
pairing QR code carries the computer's Tailscale name and names you give (`names = [...]`) after its
addresses, so the phone still finds it when they change. Everything is encrypted, a phone or a
browser is paired only when you let it in at the computer, each has its own key that the computer
can revoke, and the service only acts on the computer when told it may, starting and stopping only
the units its settings name ([security](docs/guide.md#17-security)). Nothing goes through a server
of anyone's, with one exception that is yours to make: name an ntfy topic, and the service posts its
events to that ntfy server, without names of units, disks or the computer unless you ask for them.

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

UPSes and power stations came with 0.7, your own readings and the system journal with 0.8, the
phone's own scale and theme with 0.8.1, events and notifications through ntfy with 0.9, monitors
under Hyprland, a home-screen widget and Quick Settings tiles with 0.10, the widget's rings with
0.10.1, the web page with its admin panel and the dashboard editor with 0.11. Next: 1.0, once a
month of use goes by without a change to the settings or the protocol. Not planned: the service
for Windows or macOS, app stores (the app is the APK in each release).

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).

Icons: Material Symbols by Google, Apache License 2.0.

UPS support uses upst-core, the core of upst (Apache-2.0), and works with nutflow for EcoFlow
stations.
