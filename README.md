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
  paired with it.

**Setting it up and using it: [the user guide](docs/guide.md).**

## What it does

Every screen is a dashboard of widgets that the service describes, set in its settings on the
computer (`~/.config/elamon`, or Nix on NixOS): the standard Overview, Processes, Sensors and Control
come in the settings it starts with, and you can change them or add your own — any reading,
table, graph, button or choice — without updating the app ([settings reference](docs/config.md)).
How the app looks and behaves on each phone is that phone's profile, set on the computer too.

Watching, always:

- a dashboard of the CPU, memory, each graphics card, temperatures, network, storage and battery,
  each with a page of its own: ten minutes of history on charts you can read with a finger, and
  the processes using the most;
- processes, sortable and searchable;
- every temperature sensor and fan;
- connected monitors with their modes and arrangement, read from the kernel, so under any desktop.

Controlling, once the computer allows it:

- links to the web pages it hosts: open them, see if they answer, start, stop and restart their
  units;
- media players and the volume;
- the power profile, locking, restarting and shutting down;
- commands you define on the computer, with a parameter to choose from the values you allow;
- quitting and killing processes.

Each phone has a role, set on the computer: glancing at the basics, watching everything, everyday
control, or all of it. Every request to act is kept in an activity log, and `elamond control off`
stops every phone at once.

Moving is kept simple: `elamond backup` saves the computer's identity and its paired phones in one
file only you can read, which `elamond restore` puts back on a reinstalled or new computer without
pairing the phones again; a new phone pairs with `elamond pair`.

The app: Material You with frosted glass on Android 12 and newer, light, dark or pure black.

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

The phone and the computer talk directly, over the local network or a VPN they share: TCP 47823,
and UDP 5353 for finding the computer by itself. The service can listen on chosen interfaces only
(`listen = ["tailscale0"]`), and announces itself only on the local network. Its pairing QR code
carries the computer's Tailscale name and names you give (`names = [...]`) after its addresses, so
the phone still finds it when they change. Everything is encrypted, a phone is paired only when
you let it in at the computer, each phone has its own key that the computer can revoke, and the
service only acts on the computer when told it may, starting and stopping only the units its
settings name ([security](docs/guide.md#10-security)).

## Technical notes

- A discrete GPU that has powered itself down is reported as asleep, not woken up to be read.
- NVIDIA GPUs are read through NVML, AMD and Intel through sysfs. Intel graphics don't report
  their load without extra privileges (perf); the app shows the clock instead.
- Per-process GPU use comes from the kernel's DRM fdinfo, so it covers your own processes.
- Reading a monitor's current mode opens the GPU's DRM node for a moment. With no compositor
  running, whoever opens it first briefly becomes its DRM master; the service closes it right away.
- Power actions, power profiles and units go through D-Bus (logind, power-profiles-daemon,
  systemd), media through MPRIS, the volume through `wpctl` or `pactl`.
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
nix flake check                                   # the NixOS module in a VM
maestro/run.sh                                    # the app on an emulator only (a real phone: --yes-real-phone), against a throwaway service
```

The protocol between the app and the service: [docs/protocol.md](docs/protocol.md); the settings,
dashboards and profiles: [docs/config.md](docs/config.md).

## Where it's going

Next, as they're needed: a UPS's state, history and controls (through NUT), custom readings
and the system journal, notifications through ntfy, and controlling monitors under
Hyprland. Not planned: the
service for Windows or macOS, app stores (the app is the APK in each release), a web version.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).

Icons: Material Symbols by Google, Apache License 2.0.
