#!/bin/sh
# A release of elamond as GitHub has one, made from a binary here, for trying install.sh and
# `elamond update` without GitHub. Into DIR: elamond-VERSION-ARCH-linux.tar.gz packed as
# elamond/dist/build-release.sh packs it, its .sha256, and releases.json, GitHub's list of
# releases with each one made in DIR so far, naming their files at http://127.0.0.1:PORT/.
#
#   .github/local-release.sh BINARY VERSION DIR PORT
#
# Then serve DIR there (python3 -m http.server -b 127.0.0.1 -d DIR PORT) and point
# ELAMON_RELEASES_URL at http://127.0.0.1:PORT/releases.json.
set -eu

if [ $# -ne 4 ]; then
	echo "usage: $0 BINARY VERSION DIR PORT" >&2
	exit 2
fi
binary=$1
version=$2
dir=$3
base=http://127.0.0.1:$4
name=elamond-$version-$(uname -m)-linux

mkdir -p "$dir/stage/$name"
cp "$binary" "$dir/stage/$name/elamond"
chmod 755 "$dir/stage/$name/elamond"
tar -C "$dir/stage" -czf "$dir/$name.tar.gz" "$name"
rm -rf "$dir/stage"
(cd "$dir" && sha256sum "$name.tar.gz" > "$name.tar.gz.sha256")

cat > "$dir/release-$version.json" << EOF
  {
    "html_url": "$base/v$version",
    "tag_name": "v$version",
    "draft": false,
    "prerelease": true,
    "assets": [
      { "name": "$name.tar.gz", "browser_download_url": "$base/$name.tar.gz" },
      { "name": "$name.tar.gz.sha256", "browser_download_url": "$base/$name.tar.gz.sha256" }
    ]
  }
EOF
{
	echo '['
	sep=
	for f in "$dir"/release-*.json; do
		[ -z "$sep" ] || echo "$sep"
		cat "$f"
		sep=,
	done
	echo ']'
} > "$dir/releases.json"
echo "$dir/$name.tar.gz"
