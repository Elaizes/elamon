# Elamon

> **Test software in active development.** This is an early test version: features, settings and
> the protocol between the app and the service change from one version to the next, and things may
> break. Use it where that's fine, and update the app and the service together.

Watch and control a Linux computer from your Android phone, live.

- **Elamon**, the Android app (Android 8 and newer). No Google services, no trackers, no ads.
- **elamond**, a small service on the computer (Linux, any distribution, any desktop or none). It
  reads the kernel's own counters and serves them over HTTPS, only to phones paired with it.

**Setting it up and using it: [the user guide](docs/guide.md).**

## What it does

Watching, always:

- a dashboard of the CPU, memory, each graphics card, temperatures, network, storage and battery,
  each with a page of its own: ten minutes of history on charts you can read with a finger, and
  the processes using the most;
- processes, sortable, searchable and grouped by program;
- every temperature sensor and fan;
- connected monitors with their modes and arrangement, read from the kernel, so under any desktop.

Controlling, once the computer allows it:

- the web services it hosts: open them, see if they answer, start, stop and restart their units;
- media players and the volume;
- the power profile, locking, restarting and shutting down;
- commands you define on the computer;
- quitting and killing processes.

The app: Material You with frosted glass on Android 12 and newer, light, dark or pure black.

## Quick start

1. Install the service. On NixOS, from this flake:

   ```nix
   imports = [ inputs.elamon.nixosModules.default ];
   services.elamond = {
     enable = true;
     openFirewall = true;
   };
   ```

   Home Manager, Nix on other distributions, and Fedora, Debian, Ubuntu, Arch, openSUSE and the
   rest: [section 1 of the guide](docs/guide.md#1-install-the-service).

2. Run `elamond pair` on the computer and point the phone's camera at the QR code it prints, or
   enter its six-digit code in the app ([details](docs/guide.md#3-pair-the-phone)).

3. For the control panel, add `control = true` to the service's settings
   ([details](docs/guide.md#5-the-control-panel)).

The phone and the computer talk directly, over the local network or a VPN they share: TCP 47823,
and UDP 5353 for finding the computer by itself. Everything is encrypted, each phone has its own
key that the computer can revoke, and the service only acts on the computer when told it may
([security](docs/guide.md#9-security)).

## Technical notes

- A discrete GPU that has powered itself down is reported as asleep, not woken up to be read.
- NVIDIA GPUs are read through NVML, AMD and Intel through sysfs. Intel graphics don't report
  their load without extra privileges (perf); the app shows the clock instead.
- Per-process GPU use comes from the kernel's DRM fdinfo, so it covers your own processes.
- Reading a monitor's current mode opens the GPU's DRM node for a moment. With no compositor
  running, whoever opens it first briefly becomes its DRM master; the service closes it right away.
- Power actions, power profiles and units go through D-Bus (logind, power-profiles-daemon,
  systemd), media through MPRIS, the volume through `wpctl` or `pactl`.
- The service uses about 0.4 % of one CPU core while no phone watches and around 1 % while one
  does, and some 11 MB of memory.

## Building

With Nix, `nix develop` gives the Android SDK, JDK and Rust:

```sh
nix develop -c ./gradlew assembleRelease     # app: app/build/outputs/apk/release/
nix develop -c bash -c 'cd elamond && cargo build --release'
nix build .#elamond                           # service package, for x86_64-linux and aarch64-linux
nix develop .#emulator                        # an x86_64 Android emulator for trying the app
```

Without Nix: Android SDK 35, JDK 17, and Rust 1.88 or newer.

The protocol between the app and the service: [docs/protocol.md](docs/protocol.md).

## Not there yet

Services for Windows and macOS, ready-made service binaries, and published app builds.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
