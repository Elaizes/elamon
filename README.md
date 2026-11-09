# Elamon

Watch a Linux computer from your Android phone, live: processor load and clocks, temperatures
and fans, memory, graphics cards, battery, network, disks, the processes that use them, and the
monitors plugged in.

Two parts:

- **Elamon**, the Android app (Android 8 and newer). No Google services, no trackers, no ads.
- **elamond**, a small service on the computer (Linux, any distribution, any desktop or none).
  It reads the kernel's own counters and serves them over HTTPS to phones paired with it.

## What it shows

- **Overview**: CPU load with ten minutes of history, per-core bars, clocks, load average;
  memory and swap; each GPU (NVIDIA through NVML, AMD and Intel through sysfs); battery health,
  draw and time left; network speed per interface; disk usage and throughput; power profile.
- **Processes**: sorted by CPU, memory, GPU, disk or name; search by name, command or PID;
  grouped by program; details of each.
- **Sensors**: every hwmon temperature, grouped by device, with their limits; fan speeds.
- **Displays**: connected monitors with maker, model, size, current and native mode, highest
  refresh rate, and their arrangement; free ports. Read from the kernel (DRM and EDID), so it
  works the same under any compositor.

A discrete GPU that has powered itself down stays asleep: the service reports it as sleeping
instead of waking it to read it.

## Set up

1. Install the service on the computer (see below) and start it:

       systemctl --user enable --now elamond

2. Pair a phone. On the computer run

       elamond pair

   It prints a QR code and a six-digit code. Point the phone's camera at the QR code, or open
   Elamon, tap **Add computer** (or the computer under *Found on this network*) and enter the
   code. When pairing by code, the app shows the computer's certificate digits; check that they
   match the `Certificate:` line `elamond pair` printed.

The phone reaches the computer over the local network or any VPN the two share. The service
listens on TCP port **47823**; open it in the computer's firewall for the networks you trust.

### Installing the service

**Nix flake**: `packages.x86_64-linux.elamond`, and a Home Manager module:

```nix
imports = [ elamon.homeManagerModules.default ];
services.elamond.enable = true;
```

**From source** (Rust 1.88 or newer):

```sh
cd elamond
cargo build --release
install -Dm755 target/release/elamond ~/.local/bin/elamond
install -Dm644 dist/elamond.service ~/.config/systemd/user/elamond.service
sed -i "s|/usr/bin/elamond|$HOME/.local/bin/elamond|" ~/.config/systemd/user/elamond.service
systemctl --user daemon-reload
systemctl --user enable --now elamond
```

To keep it running while nobody is logged in, enable lingering: `loginctl enable-linger`.

### Service commands

| Command | |
|---|---|
| `elamond serve` | run the service (what the service does) |
| `elamond pair` | show a QR code and a code for pairing a phone |
| `elamond clients` | list paired phones |
| `elamond revoke <id\|all>` | unpair a phone |
| `elamond snapshot` | print one reading as JSON, to see what the service finds |

### Settings

Optional, in `~/.config/elamon/elamond.toml`:

```toml
port = 47823            # TCP port
listen = "::"           # address to listen on; "::" covers IPv4 and IPv6
name = ""               # name shown in the app; the host name when empty
mdns = true             # announce the service on the local network
interval_ms = 1000      # how often the service takes a reading
history_minutes = 10    # how much history a phone gets when it connects
```

State lives in `~/.local/share/elamon/`: the service's id and TLS certificate, the paired phones
(only hashes of their tokens) and a pending pairing code. Deleting the directory unpairs every
phone and gives the service a new certificate.

## Security

- Everything goes over TLS. The service's certificate is self-signed; the app pins it at pairing
  (from the QR code, or after you compare the digits) and refuses any other certificate later.
- A paired phone holds a random 256-bit token; the service keeps only its SHA-256. Each phone has
  its own and can be unpaired with `elamond revoke`.
- A pairing code is good for five minutes and five attempts, one attempt at a time.
- Without a token the service answers only `hello` (its name and id) and pairing.
- The service only reads. It needs no root: it runs as your user and sees what your user sees.
- The app keeps tokens out of Android backups and device transfers.

## Good to know

- Intel graphics don't report their load without extra privileges (perf); the app shows the
  clock instead.
- Per-process GPU use comes from the kernel's DRM fdinfo, so it covers your own processes.
- Reading a monitor's current mode opens the GPU's DRM node for a moment. With no compositor
  running, whoever opens it first briefly becomes its DRM master; the service closes it right away.
- The service itself uses about 0.3 % of one CPU core while no phone watches and around 1 % while
  one does, and 8 MB of memory.

## Building

With Nix, `nix develop` gives the Android SDK, JDK and Rust:

```sh
nix develop -c ./gradlew assembleRelease     # app: app/build/outputs/apk/release/
nix develop -c bash -c 'cd elamond && cargo build --release'
nix build .#elamond                           # service package
nix develop .#emulator                        # an x86_64 Android emulator for trying the app
```

Without Nix: Android SDK 35, JDK 17 and Rust from your distribution or rustup.

The protocol between the two is described in [docs/protocol.md](docs/protocol.md).

## Planned

A control panel, opt-in on the computer: power profile, ending processes, suspend and shutdown.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
