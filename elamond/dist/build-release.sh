#!/usr/bin/env bash
# The release archives of elamond, as .github/workflows/release.yml makes them: the service for
# x86_64 and aarch64 Linux, linked against glibc 2.28 so it runs on every distribution from
# Debian 10, Ubuntu 20.04 and RHEL 8 on (not fully static: NVIDIA's library is loaded at run time,
# which musl can't), each packed with its unit, the licence and a README, a SHA-256 beside it.
#
#   elamond/dist/build-release.sh [OUT_DIR]       default: elamond/target/dist
#
# Needs a C compiler's worth of zig, cargo-zigbuild, and Rust's standard library for both targets
# (with rustup, it adds them). On NixOS:
#   nix shell nixpkgs#rustup nixpkgs#cargo-zigbuild nixpkgs#zig -c elamond/dist/build-release.sh
# TARGETS="x86_64" builds one of them only.
set -euo pipefail

cd "$(dirname "$0")/.."
version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1)
out=$(realpath -m "${1:-target/dist}")
glibc=2.28
targets=${TARGETS:-x86_64 aarch64}
# Archives that don't change with the time they were made: the last commit's.
export SOURCE_DATE_EPOCH=${SOURCE_DATE_EPOCH:-$(git log -1 --format=%ct 2>/dev/null || date +%s)}

if command -v rustup > /dev/null; then
	export RUSTUP_TOOLCHAIN=${RUSTUP_TOOLCHAIN:-stable}
	rustup toolchain install "$RUSTUP_TOOLCHAIN" --profile minimal \
		$(for arch in $targets; do printf -- '--target %s-unknown-linux-gnu ' "$arch"; done)
fi

mkdir -p "$out"
for arch in $targets; do
	target=$arch-unknown-linux-gnu
	cargo zigbuild --release --locked --target "$target.$glibc"
	bin=target/$target/release/elamond

	# Nothing newer than glibc 2.28 may be asked for.
	newest=$( (objdump -T "$bin" 2> /dev/null || readelf -W --dyn-syms "$bin") | grep -o 'GLIBC_[0-9.]*' | sort -uV | tail -n 1)
	echo "elamond for $arch needs $newest at most"
	if [ "$(printf '%s\n' "${newest#GLIBC_}" "$glibc" | sort -V | tail -n 1)" != "$glibc" ]; then
		echo "elamond for $arch needs $newest, newer than glibc $glibc" >&2
		exit 1
	fi

	name=elamond-$version-$arch-linux
	stage=$out/$name
	rm -rf "$stage"
	mkdir -p "$stage"
	install -m 0755 "$bin" "$stage/elamond"
	install -m 0644 dist/elamond.service ../LICENSE "$stage/"
	cat > "$stage/README.md" << EOF
# elamond $version for $arch Linux

The Elamon service: serves this computer's metrics to the Elamon app on a paired phone.
It runs on any Linux distribution with glibc 2.28 or newer (Debian 10, Ubuntu 20.04, RHEL 8 and
later; Fedora, Arch, openSUSE), and installs for your user, without root:

    install -Dm755 elamond ~/.local/bin/elamond
    ~/.local/bin/elamond setup

\`elamond setup\` installs and starts the systemd user unit (elamond.service here is its
template), and prints what needs root: lingering, the firewall and the polkit rule. Then pair the
phone with \`elamond pair\`. \`elamond setup --check\` tells what works.

The user guide: https://github.com/Elaizes/elamon/blob/main/docs/guide.md
License: GPL-3.0-or-later (LICENSE).
EOF
	touch -d "@$SOURCE_DATE_EPOCH" "$stage" "$stage"/*
	tar -C "$out" --sort=name --owner=0 --group=0 --numeric-owner --mtime="@$SOURCE_DATE_EPOCH" -cf - "$name" |
		gzip -9n > "$out/$name.tar.gz"
	rm -rf "$stage"
	(cd "$out" && sha256sum "$name.tar.gz" > "$name.tar.gz.sha256")
	echo "$out/$name.tar.gz"
done
