#!/bin/sh
# Installs elamond, the Elamon service, for you from the newest release on GitHub, then runs
# `elamond setup`. Any Linux distribution with glibc 2.28 or newer, on x86_64 or aarch64:
#
#   curl -fsSL https://raw.githubusercontent.com/Elaizes/elamon/main/install.sh | sh
#   curl -fsSL https://raw.githubusercontent.com/Elaizes/elamon/main/install.sh | sh -s -- --no-setup
#   sh install.sh [--version X.Y.Z] [--no-setup] [--force] [options of elamond setup]
#
# It asks GitHub for the list of releases (pre-releases too, drafts never), takes the newest by
# version that has elamond for this architecture, downloads its archive and the archive's SHA-256,
# checks one against the other, and puts elamond in ~/.local/bin (ELAMON_BIN_DIR) in one step.
# The checksum comes from the same release: it catches a download damaged on the way, not a
# release forged on GitHub, which only HTTPS to GitHub stands against. Later: `elamond update`.
#
# For tests, ELAMON_RELEASES_URL names another list in GitHub's shape; plain http is taken from it,
# and from the files it names, only on this computer (127.0.0.1, ::1, localhost).
#
# Everything runs from the last line, so a script cut short while it downloads does nothing.

set -u

RELEASES='https://api.github.com/repos/Elaizes/elamon/releases?per_page=100'
GUIDE=https://github.com/Elaizes/elamon/blob/main/docs/guide.md
AGENT=elamon-install
# The most each download may be: the list of releases, an archive, a checksum.
LIST_MAX=8388608
ARCHIVE_MAX=134217728
SUM_MAX=65536

usage() {
	cat << 'EOF'
Installs elamond, the Elamon service, for you, then runs elamond setup.

  curl -fsSL https://raw.githubusercontent.com/Elaizes/elamon/main/install.sh | sh
  sh install.sh [OPTIONS] [options of elamond setup: --no-start, --print, ...]

  --version X.Y.Z   that release, not the newest
  --no-setup        only put elamond in place; run elamond setup yourself
  --force           install even if elamond is already here
  -h, --help        this

ELAMON_BIN_DIR: where elamond goes (~/.local/bin). Later updates: elamond update.
EOF
}

say() {
	printf '%s\n' "$*"
}

die() {
	printf 'install.sh: %s\n' "$*" >&2
	exit 1
}

# A path with the home folder written ~, for reading.
show() {
	case $1 in
		"$HOME"/*) printf '%s/%s' '~' "${1#"$HOME"/}" ;;
		*) printf '%s' "$1" ;;
	esac
}

has() {
	command -v "$1" > /dev/null 2>&1
}

# Whether URL may be downloaded from: https anywhere, plain http only from this computer and only
# with ELAMON_RELEASES_URL set. No user name before the host, no other port than digits.
url_ok() {
	case $1 in
		https://*) return 0 ;;
		http://*) [ -n "${ELAMON_RELEASES_URL:-}" ] || return 1 ;;
		*) return 1 ;;
	esac
	host=${1#http://}
	host=${host%%/*}
	case $host in
		127.0.0.1 | localhost | '[::1]') return 0 ;;
		127.0.0.1:* | localhost:* | '[::1]:'*) ;;
		*) return 1 ;;
	esac
	case ${host##*:} in
		'' | *[!0-9]*) return 1 ;;
	esac
	return 0
}

# fetch URL FILE MAX [ACCEPT]: URL into FILE, at most MAX bytes, over https only (see url_ok).
fetch() {
	url_ok "$1" || die "won't download $1: only https is taken"
	case $1 in
		https://*) proto='=https' ;;
		*) proto='=http' ;;
	esac
	if [ "$get" = curl ]; then
		# -q first: no ~/.curlrc, which could turn checks off or add headers.
		curl -q --fail --silent --show-error --location --globoff --proto "$proto" --proto-redir "$proto" \
			--tlsv1.2 --max-redirs 5 --connect-timeout 20 --max-time 900 --retry 2 --max-filesize "$3" \
			--user-agent "$AGENT" --header "Accept: ${4:-*/*}" --output "$2" "$1" || return 1
	else
		only=
		[ "$proto" = '=https' ] && only=--https-only
		wget --quiet --output-document="$2" --timeout=30 --tries=3 --max-redirect=5 ${only:+"$only"} \
			--user-agent="$AGENT" --header="Accept: ${4:-*/*}" "$1" || return 1
	fi
	size=$(wc -c < "$2") || return 1
	[ "$size" -le "$3" ] || die "$1 is larger than it can be ($size bytes)"
}

# Reads GitHub's list of releases (JSON) and prints, for each release that isn't a draft and has
# both elamond's archive for ARCH and its SHA-256: VERSION ARCHIVE_URL SHA256_URL PAGE_URL. Only
# X.Y.Z versions count, unless WANT names one. Records end at each comma, so no line is ever long;
# strings are told from the rest by their quotes once \\ and \" in them are set aside, so nothing
# in a release's notes can pass for one of its keys, whatever the layout.
releases() {
	awk -v arch="$1" -v want="$2" '
		function flush() {
			if (lit != "") value(lit, 0)
			lit = ""
		}
		function punct(c) {
			flush()
			if (c == "{" || c == "[") {
				d++
				top[d] = c
				key[d] = ""
				expect[d] = (c == "{") ? "key" : "value"
				if (d == 2 && c == "{" && top[1] == "[") {
					tag = ""; draft = ""; page = ""; urls = ""
				}
			} else if (c == "}" || c == "]") {
				if (d == 2 && c == "}" && top[1] == "[") release()
				if (d > 0) d--
			} else if (c == ":") {
				expect[d] = "value"
			} else if (c == ",") {
				if (top[d] == "{") expect[d] = "key"
			}
		}
		function string(s) {
			flush()
			if (top[d] == "{" && expect[d] == "key") key[d] = s
			else value(s, 1)
		}
		function value(v, quoted) {
			if (quoted && (index(v, "\001") || index(v, "\002") || index(v, "\\"))) v = ""
			if (d == 2 && top[1] == "[" && top[2] == "{") {
				if (key[2] == "tag_name" && quoted) tag = v
				else if (key[2] == "draft") draft = quoted ? "" : v
				else if (key[2] == "html_url" && quoted) page = v
			} else if (d == 4 && top[1] == "[" && top[3] == "[" && top[4] == "{" && key[2] == "assets" && key[4] == "browser_download_url" && quoted) {
				urls = urls " " v
			}
		}
		function ends(s, t) {
			return length(s) >= length(t) && substr(s, length(s) - length(t) + 1) == t
		}
		function release(   v, name, n, u, i, a, s) {
			if (draft != "false" || tag == "") return
			v = tag
			sub(/^v/, "", v)
			if (want != "") {
				if (v != want) return
			} else if (v !~ /^[0-9]+\.[0-9]+\.[0-9]+$/) return
			name = "/elamond-" v "-" arch "-linux.tar.gz"
			a = ""; s = ""
			n = split(urls, u, " ")
			for (i = 1; i <= n; i++) {
				if (ends(u[i], name)) a = u[i]
				else if (ends(u[i], name ".sha256")) s = u[i]
			}
			if (a != "" && s != "") print v, a, s, (page == "" ? "-" : page)
		}
		BEGIN { RS = ","; d = 0; instr = 0; lit = "" }
		NR > 1 {
			if (instr) buf = buf ","
			else punct(",")
		}
		{
			line = $0
			gsub(/\\\\/, "\001", line)
			gsub(/\\"/, "\002", line)
			n = split(line, part, "\"")
			for (i = 1; i <= n; i++) {
				if (i > 1) {
					if (instr) { instr = 0; string(buf) }
					else { flush(); instr = 1; buf = "" }
				}
				if (instr) {
					buf = buf part[i]
					continue
				}
				s = part[i]
				gsub(/[ \t\r\n]+/, "", s)
				m = length(s)
				for (j = 1; j <= m; j++) {
					c = substr(s, j, 1)
					if (index("{}[]:,", c)) punct(c)
					else lit = lit c
				}
			}
		}
		END { flush() }
	'
}

main() {
	want=''
	setup=yes
	force=''
	n=$#
	while [ "$n" -gt 0 ]; do
		a=$1
		shift
		n=$((n - 1))
		case $a in
			-h | --help)
				usage
				exit 0
				;;
			--no-setup) setup= ;;
			--force) force=yes ;;
			--version)
				[ "$n" -gt 0 ] || die "--version needs a version, like --version 0.6.1"
				want=$1
				shift
				n=$((n - 1))
				;;
			--version=*) want=${a#--version=} ;;
			-*) set -- "$@" "$a" ;; # for elamond setup, in order
			*) die "unknown argument \"$a\" (sh install.sh --help)" ;;
		esac
	done
	want=${want#v}
	case $want in
		'') ;;
		[!0-9]* | *[!0-9A-Za-z.+-]*) die "\"$want\" isn't a version, like 0.6.1" ;;
	esac
	if [ -z "$setup" ] && [ $# -gt 0 ]; then
		die "$* would be for elamond setup, which --no-setup leaves out"
	fi

	[ "$(id -u)" != 0 ] || die "run it as the user the service is for, not root: elamond runs as you, and elamond setup prints the few commands that need root"
	[ "$(uname -s)" = Linux ] || die "elamond runs on Linux only"
	machine=$(uname -m)
	case $machine in
		x86_64 | amd64) arch=x86_64 ;;
		aarch64 | arm64) arch=aarch64 ;;
		*) die "there's no elamond release for $machine; build it from source: $GUIDE#building-from-source" ;;
	esac
	libc=$(getconf GNU_LIBC_VERSION 2> /dev/null) || libc=
	case $libc in
		'glibc '*) ;;
		*) die "elamond's releases need glibc, and this system's C library is another (musl?); build it from source: $GUIDE#building-from-source" ;;
	esac
	glibc=${libc#glibc }
	major=${glibc%%.*}
	minor=${glibc#*.}
	minor=${minor%%.*}
	case $major$minor in
		'' | *[!0-9]*) die "can't read the glibc version (\"$libc\")" ;;
	esac
	if [ "$major" -lt 2 ] || { [ "$major" -eq 2 ] && [ "$minor" -lt 28 ]; }; then
		die "glibc $glibc here, and elamond's releases need 2.28 or newer; build it from source: $GUIDE#building-from-source"
	fi

	bin_dir=${ELAMON_BIN_DIR:-$HOME/.local/bin}
	case $bin_dir in
		/*) ;;
		*) die "ELAMON_BIN_DIR must be a full path, not \"$bin_dir\"" ;;
	esac
	bin_dir=${bin_dir%/}
	target=$bin_dir/elamond

	# Already here: `elamond update` is the way on, not a second install.
	there=
	if [ -e "$target" ] || [ -h "$target" ]; then
		there=$target
	elif found=$(command -v elamond 2> /dev/null); then
		there=$found
	fi
	if [ -n "$there" ] && [ -z "$force" ]; then
		have=$("$there" --version 2> /dev/null) || have=
		case $(readlink -f "$there" 2> /dev/null || printf '%s' "$there") in
			/nix/store/*)
				say "${have:-elamond} is installed with Nix ($there): update it with Nix (guide, section 8)."
				say "--force installs another one in $(show "$bin_dir") beside it."
				;;
			*)
				say "${have:-elamond} is already installed: $(show "$there")"
				say "To update it: elamond update. To install over it anyway: add --force."
				;;
		esac
		if [ -n "$want" ] && [ "$have" != "elamond $want" ]; then
			exit 1
		fi
		exit 0
	fi

	missing=
	for t in tar gzip sha256sum mktemp awk sort; do
		has "$t" || missing="$missing $t"
	done
	if has curl; then
		get=curl
	elif has wget; then
		get=wget
	else
		missing="$missing curl"
	fi
	[ -z "$missing" ] || die "missing:$missing. Install it with your distribution's package manager, then run this again"

	tmp=$(mktemp -d) || die "can't make a temporary folder"
	trap 'rm -rf "$tmp"; [ -z "${part:-}" ] || rm -f "$part"' EXIT
	trap 'exit 1' HUP INT TERM

	list=${ELAMON_RELEASES_URL:-$RELEASES}
	fetch "$list" "$tmp/releases.json" "$LIST_MAX" application/vnd.github+json || die "can't read the list of releases ($list)"
	pick=$(releases "$arch" "$want" < "$tmp/releases.json" | sort -t . -k 1,1n -k 2,2n -k 3,3n | tail -n 1)
	if [ -z "$pick" ]; then
		[ -z "$want" ] || die "there's no release $want with elamond for $arch"
		die "there's no release with elamond for $arch yet"
	fi
	read -r version archive_url sum_url page << EOF
$pick
EOF
	name=elamond-$version-$arch-linux
	archive=$name.tar.gz

	say "Downloading elamond $version for $arch"
	fetch "$archive_url" "$tmp/$archive" "$ARCHIVE_MAX" || die "can't download $archive_url"
	fetch "$sum_url" "$tmp/$archive.sha256" "$SUM_MAX" || die "can't download $sum_url"

	# The checksum file names the archive and nothing else; sha256sum -c would check whatever
	# file it names.
	[ "$(grep -c . "$tmp/$archive.sha256")" = 1 ] || die "$archive.sha256 should hold one checksum"
	read -r sum file extra < "$tmp/$archive.sha256" || [ -n "${sum:-}" ] || die "can't read $archive.sha256"
	if [ "${file#\*}" != "$archive" ] || [ -n "$extra" ]; then
		die "$archive.sha256 is the checksum of another file"
	fi
	case $sum in
		*[!0-9a-fA-F]*) die "$archive.sha256 holds no SHA-256" ;;
	esac
	[ ${#sum} -eq 64 ] || die "$archive.sha256 holds no SHA-256"
	sum=$(printf '%s' "$sum" | tr 'A-F' 'a-f')
	got=$(sha256sum "$tmp/$archive") || die "can't read $archive"
	[ "${got%% *}" = "$sum" ] || die "$archive doesn't match its SHA-256: damaged on the way? Nothing was installed; try again"
	say "  OK  SHA-256 matches"

	# Only elamond comes out of the archive, and it must be a file of its own.
	mkdir "$tmp/x" || die "can't unpack $archive"
	tar -xzf "$tmp/$archive" -C "$tmp/x" "$name/elamond" || die "can't unpack $archive"
	new=$tmp/x/$name/elamond
	if [ -h "$new" ] || [ ! -f "$new" ]; then
		die "$archive holds no elamond file"
	fi

	# Copied beside where it goes, checked there (it runs from there, whatever /tmp allows), then
	# renamed over the old one, which is atomic.
	mkdir -p "$bin_dir" || die "can't make $bin_dir"
	if [ -d "$target" ] && [ ! -h "$target" ]; then
		die "$target is a folder"
	fi
	part=$(mktemp "$bin_dir/.elamond.XXXXXX") || die "can't write to $bin_dir"
	cp "$new" "$part" || die "can't write to $bin_dir"
	chmod 755 "$part" || die "can't write to $bin_dir"
	says=$("$part" --version 2> /dev/null) || die "the elamond downloaded doesn't run here"
	[ "$says" = "elamond $version" ] || die "the elamond downloaded says \"$says\", not elamond $version"
	mv -f "$part" "$target" || die "can't put elamond in $bin_dir"
	part=
	say "  OK  installed $(show "$target") (elamond $version)"
	case $page in
		https://*) say "      release notes: $page" ;;
	esac

	on_path=
	case :$PATH: in
		*:"$bin_dir":*) on_path=yes ;;
	esac
	first=$(command -v elamond 2> /dev/null) || first=
	if [ -z "$on_path" ]; then
		say "  !   $(show "$bin_dir") isn't on your PATH: run elamond as $(show "$target"), or add the folder to PATH (many distributions do at the next login)"
	elif [ -n "$first" ] && [ "$first" != "$target" ]; then
		say "  !   another elamond comes first on your PATH: $first"
	fi

	if [ -z "$setup" ]; then
		say "Next: $(show "$target") setup"
		exit 0
	fi
	say ""
	"$target" setup "$@" || die "elamond setup didn't finish. elamond is installed: run $(show "$target") setup again once that is mended"
}

main "$@"
