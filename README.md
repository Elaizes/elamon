# Elamon

> **Test software in active development.** This is an early test version: features, settings and
> the protocol between the app and the service change from one version to the next, and things may
> break. Use it where that's fine, and update the app and the service together.

Watch and control a Linux computer from your Android phone, live.

- **Elamon**, the Android app (Android 8 and newer). No Google services, no trackers, no ads.
- **elamond**, the Elamon service, a small program on the computer (Linux, any distribution, any
  desktop or none). It reads the kernel's own counters and serves them over HTTPS, only to phones
  paired with it.

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

- links to the web pages it hosts: open them, see if they answer, start, stop and restart their
  units;
- media players and the volume;
- the power profile, locking, restarting and shutting down;
- commands you define on the computer;
- quitting and killing processes.

Each phone has a role, set on the computer: glancing at the basics, watching everything, everyday
control, or all of it. Every request to act is kept in an activity log, and `elamond control off`
stops every phone at once.

Moving is kept simple: `elamond backup` saves the computer's identity and its paired phones in one
encrypted file, which `elamond restore` puts back on a reinstalled or new computer without pairing
the phones again, and the app exports its pairings, encrypted with a passphrase, for a new phone.

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
and UDP 5353 for finding the computer by itself. The service can listen on chosen interfaces only
(`listen = ["tailscale0"]`), and announces itself only on the local network. Its pairing QR code
carries the computer's Tailscale name and names you give (`names = [...]`) after its addresses, so
the phone still finds it when they change. Everything is encrypted, each phone has its own key that
the computer can revoke, the computer's certificate can be renewed without pairing again
(`elamond rotate-cert`), and the service only acts on the computer when told it may
([security](docs/guide.md#10-security)).

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

Without Nix: Android SDK 37, JDK 17, and Rust 1.88 or newer. A release build is signed when
`~/.android/elamon-release.properties` (or the file `-Pelamon.signing` names) gives `storeFile`,
`storePassword`, `keyAlias` and `keyPassword`; otherwise it stays unsigned.

Tests:

```sh
nix develop -c bash -c 'cd elamond && cargo test' # the service, and the protocol fixtures in elamond/fixtures
nix develop -c ./gradlew :app:testDebugUnitTest    # app, against the same fixtures
nix flake check                                   # the NixOS module in a VM
maestro/run.sh                                    # the app on an emulator against a throwaway service
```

The protocol between the app and the service: [docs/protocol.md](docs/protocol.md).

## Not there yet

The service for Windows and macOS, ready-made binaries of it, and published app builds.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
