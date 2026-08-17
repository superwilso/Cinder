#!/usr/bin/env bash
# configure.sh — choose which optional parts of Cinder to install.
#
# Reads the component catalogue (deploy/components.conf) and writes the answers to
# dist/<channel>/cinder_components.conf, which install_cinderhome.sh sources on the device and
# uses to gate each optional step. Adding a component means editing components.conf only —
# nothing in this script knows the component list.
#
#   ./tools/configure.sh                      interactive picker (default channel: stable)
#   ./tools/configure.sh dev                  interactive picker for the dev channel
#   ./tools/configure.sh --defaults           write the defaults, no prompting
#   ./tools/configure.sh --set signature=pv2 --disable gpunode --defaults
#   ./tools/configure.sh --show               print the current selection and exit
#   ./tools/configure.sh -o /path/out.conf    write somewhere else
#
# The generated file is SOURCED BY A SHELL ON THE DEVICE, so every value written here is
# whitelist-validated against the type declared in the catalogue. Nothing reaches the file that
# did not come from the catalogue's own enum list (or 0/1 for a bool).
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CH="$HERE/.."
CATALOGUE="$CH/deploy/components.conf"
[ -f "$CATALOGUE" ] || { echo "ERR: catalogue not found at $CATALOGUE" >&2; exit 1; }

CHANNEL=stable
OUT=""
INTERACTIVE=1
declare -A PRESET=()

while [ $# -gt 0 ]; do
    case "$1" in
        stable|dev) CHANNEL="$1"; shift ;;
        --defaults) INTERACTIVE=0; shift ;;
        --show)     INTERACTIVE=2; shift ;;
        -o)         OUT="${2:-}"; shift 2 ;;
        --enable)   PRESET["${2:-}"]=1; shift 2 ;;
        --disable)  PRESET["${2:-}"]=0; shift 2 ;;
        --set)      k="${2%%=*}"; v="${2#*=}"; PRESET["$k"]="$v"; shift 2 ;;
        -h|--help)  sed -n '2,18p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "ERR: unknown argument '$1' (try --help)" >&2; exit 2 ;;
    esac
done
[ -n "$OUT" ] || OUT="$CH/dist/$CHANNEL/cinder_components.conf"

# ── parse the catalogue ────────────────────────────────────────────────────────────────────
IDS=(); declare -A VAR TYPE DEF TITLE DESC
cur=""
while IFS= read -r line; do
    [[ "$line" =~ ^[[:space:]]*# ]] && continue
    if [[ "$line" =~ ^[^[:space:]] ]] && [[ "$line" == *"|"* ]]; then
        IFS='|' read -r f_id f_var f_type f_def f_title <<<"$line"
        f_id="$(echo "$f_id" | xargs)"; f_var="$(echo "$f_var" | xargs)"
        f_type="$(echo "$f_type" | xargs)"; f_def="$(echo "$f_def" | xargs)"
        f_title="$(echo "$f_title" | xargs)"
        [[ "$f_id"  =~ ^[a-z][a-z0-9-]*$   ]] || { echo "ERR: bad id '$f_id' in catalogue" >&2; exit 1; }
        [[ "$f_var" =~ ^[A-Z][A-Z0-9_]*$   ]] || { echo "ERR: bad varname '$f_var' in catalogue" >&2; exit 1; }
        IDS+=("$f_id"); VAR[$f_id]="$f_var"; TYPE[$f_id]="$f_type"
        DEF[$f_id]="$f_def"; TITLE[$f_id]="$f_title"; DESC[$f_id]=""
        cur="$f_id"
    elif [ -n "$cur" ] && [[ "$line" =~ ^[[:space:]]+[^[:space:]] ]]; then
        DESC[$cur]+="${line#"${line%%[![:space:]]*}"}"$'\n'
    fi
done < "$CATALOGUE"
[ ${#IDS[@]} -gt 0 ] || { echo "ERR: catalogue has no components" >&2; exit 1; }

# allowed values for an id: "0 1" for bool, the enum members otherwise
allowed() {
    local t="${TYPE[$1]}"
    if [ "$t" = bool ]; then echo "0 1"
    elif [[ "$t" == enum:* ]]; then echo "${t#enum:}" | tr ',' ' '
    else echo ""; fi
}
valid() {  # valid <id> <value>
    local v; for v in $(allowed "$1"); do [ "$v" = "$2" ] && return 0; done; return 1
}

# ── current selection: defaults, then anything already saved, then CLI overrides ────────────
declare -A SEL=()
for id in "${IDS[@]}"; do SEL[$id]="${DEF[$id]}"; done
if [ -f "$OUT" ]; then
    while IFS='=' read -r k v; do
        [[ "$k" =~ ^[A-Z][A-Z0-9_]*$ ]] || continue
        for id in "${IDS[@]}"; do
            if [ "${VAR[$id]}" = "$k" ] && valid "$id" "$v"; then SEL[$id]="$v"; fi
        done
    done < <(grep -E '^[A-Z][A-Z0-9_]*=' "$OUT" 2>/dev/null || true)
fi
for k in "${!PRESET[@]}"; do
    found=0
    for id in "${IDS[@]}"; do [ "$id" = "$k" ] && found=1; done
    [ "$found" = 1 ] || { echo "ERR: no such component '$k'" >&2; exit 2; }
    valid "$k" "${PRESET[$k]}" \
        || { echo "ERR: '${PRESET[$k]}' invalid for '$k' (allowed: $(allowed "$k"))" >&2; exit 2; }
    SEL[$k]="${PRESET[$k]}"
done

render_value() {  # pretty value for the list
    local id="$1" v="${SEL[$1]}"
    if [ "${TYPE[$id]}" = bool ]; then
        [ "$v" = 1 ] && echo "[x]" || echo "[ ]"
    else
        printf '<%s>' "$v"
    fi
}

show_list() {
    echo
    echo "  Cinder install — optional components   (channel: $CHANNEL)"
    echo "  ────────────────────────────────────────────────────────────"
    local i=1
    for id in "${IDS[@]}"; do
        printf "   %2d  %-7s %-42s %s\n" "$i" "$(render_value "$id")" "${TITLE[$id]}" "$id"
        i=$((i+1))
    done
    echo "  ────────────────────────────────────────────────────────────"
    echo "   <number> toggle/cycle    ?<number> describe    s save    q quit"
}

cycle() {  # advance an id to its next allowed value
    local id="$1" vals cur first prev=""
    vals="$(allowed "$id")"; cur="${SEL[$id]}"; first=""
    for v in $vals; do
        [ -z "$first" ] && first="$v"
        if [ -n "$prev" ] && [ "$prev" = "$cur" ]; then SEL[$id]="$v"; return; fi
        prev="$v"
    done
    SEL[$id]="$first"
}

if [ "$INTERACTIVE" = 2 ]; then
    for id in "${IDS[@]}"; do printf '%s=%s\n' "${VAR[$id]}" "${SEL[$id]}"; done
    exit 0
fi

if [ "$INTERACTIVE" = 1 ]; then
    if [ ! -t 0 ]; then
        echo "configure.sh: stdin is not a terminal — writing defaults. Use --defaults to silence." >&2
    else
        while true; do
            show_list
            printf '  > '
            read -r ans || break
            case "$ans" in
                q|Q) echo "  (not saved)"; exit 0 ;;
                s|S) break ;;
                \?*) n="${ans#\?}"
                     if [ "$n" -ge 1 ] 2>/dev/null && [ "$n" -le ${#IDS[@]} ]; then
                         id="${IDS[$((n-1))]}"
                         echo; echo "  ${TITLE[$id]}  ($id -> ${VAR[$id]})"
                         echo "  allowed: $(allowed "$id")   default: ${DEF[$id]}"
                         echo
                         printf '%s' "${DESC[$id]}" | sed 's/^/    /'
                     fi ;;
                '')  ;;
                *)   if [ "$ans" -ge 1 ] 2>/dev/null && [ "$ans" -le ${#IDS[@]} ]; then
                         cycle "${IDS[$((ans-1))]}"
                     fi ;;
            esac
        done
    fi
fi

# ── write ───────────────────────────────────────────────────────────────────────────────────
# Re-validate EVERYTHING on the way out. This file is sourced by a shell on the device; a value
# that is not literally one of the catalogue's own allowed strings must never reach it.
for id in "${IDS[@]}"; do
    valid "$id" "${SEL[$id]}" \
        || { echo "ERR: refusing to write invalid value '${SEL[$id]}' for '$id'" >&2; exit 1; }
done

mkdir -p "$(dirname "$OUT")"
{
    echo "# cinder_components.conf — generated by tools/configure.sh; do not edit by hand."
    echo "# Sourced by install_cinderhome.sh on the device. Values are whitelist-validated."
    echo "# channel: $CHANNEL"
    for id in "${IDS[@]}"; do
        echo
        echo "# ${TITLE[$id]}  (allowed: $(allowed "$id"))"
        printf '%s=%s\n' "${VAR[$id]}" "${SEL[$id]}"
    done
} > "$OUT"

echo
echo "wrote $OUT"
for id in "${IDS[@]}"; do printf '  %-10s %s\n' "$id" "${SEL[$id]}"; done
echo
echo "next: bash tools/pack_upg.sh $CHANNEL   (then push, then install)"
