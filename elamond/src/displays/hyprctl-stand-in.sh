#!/bin/sh
# A stand-in hyprctl for tests (elamond's, maestro's and the VM's): its outputs are files in outputs/
# beside it (disabled=, dpms=, backend=), the instances it lists are in instances, each call goes
# to log and each Lua it runs to evals. Files beside it change what it does: hang, hyprlang (no
# Lua), asleep (an output turned on stays off), stuck (nothing it is told is done).
here=$(dirname "$0")
printf '%s\n' "$*" >> "$here/log"
[ -f "$here/hang" ] && sleep 30
sig=""
if [ "$1" = "--instance" ]; then sig="$2"; shift 2; fi
if [ "$1 $2" = "-j instances" ]; then cat "$here/instances"; exit 0; fi
if [ -n "$sig" ] && ! grep -q "\"$sig\"" "$here/instances"; then echo "error: no such instance" >&2; exit 1; fi
field() { v=${1#*"$2" = \'}; printf '%s' "${v%%\'*}"; }
set_key() { f="$here/outputs/$1"; [ -f "$f" ] || exit 0; sed -i "s/^$2=.*/$2=$3/" "$f"; }
case "$1" in
eval)
	[ -f "$here/hyprlang" ] && { echo "error: eval needs Lua settings"; exit 7; }
	[ "$2" = "return" ] && { echo ok; exit 0; }
	printf '%s\n' "$2" >> "$here/evals"
	[ -f "$here/stuck" ] && { echo ok; exit 0; }
	case "$2" in
	"hl.monitor("*"disabled = true"*) set_key "$(field "$2" output)" disabled true ;;
	"hl.monitor("*"disabled = false"*) [ -f "$here/asleep" ] || set_key "$(field "$2" output)" disabled false ;;
	"hl.dispatch(hl.dsp.dpms("*)
		if [ "$(field "$2" action)" = on ]; then set_key "$(field "$2" monitor)" dpms true; else set_key "$(field "$2" monitor)" dpms false; fi ;;
	*) echo "error: unknown"; exit 7 ;;
	esac
	echo ok ;;
-j)
	printf '['
	first=1
	for f in "$here"/outputs/*; do
		disabled=false; dpms=true; backend=drm
		# shellcheck source=/dev/null
		. "$f"
		[ $first = 1 ] || printf ','
		first=0
		printf '{"name":"%s","disabled":%s,"dpmsStatus":%s,"width":1920,"height":1080,"refreshRate":144.149,"x":0,"y":0,"scale":1,"transform":0,"mirrorOf":"none","hardwareDetails":{"backend":"%s"}}' "$(basename "$f")" "$disabled" "$dpms" "$backend"
	done
	printf ']\n' ;;
*) echo "error: unknown request"; exit 1 ;;
esac
