#!/usr/bin/env bash
# Tira Mail - install the Rspamd local.d overrides and maps on a Linux node.
#
# Why this exists (M-19)
# ----------------------
# The old README told operators to copy `local.d/*.inc` only. That installs
# options.inc and worker-proxy.inc but silently drops the three `.conf`
# overrides Rspamd actually loads from local.d:
#
#   actions.conf       - the explicit reject/add_header/greylist thresholds
#   dkim_signing.conf  - `enabled = false` so Rspamd never double-signs (Stalwart
#                        owns DKIM); without it a package default could sign
#   multimap.conf      - wires the institution allowlist/blocklist maps to the
#                        SMTP envelope; without it the maps are idle and the
#                        envelope-based sender controls do nothing
#
# So this copies the WHOLE local.d directory (both extensions, plus the inert
# redis.conf.disabled template) and then gates on `rspamadm configtest`. It
# verifies the three .conf actually landed, because a copy that silently missed
# them is exactly the bug being fixed.
#
# Fail closed
# -----------
# If configtest fails the script exits non-zero and does NOT restart Rspamd: a
# running daemon keeps its previous (good) config in memory until an operator
# restarts it, so a broken staged config cannot take mail down by itself.
#
# Usage
# -----
#   sudo deploy/rspamd/install.sh                 # stage + configtest, no restart
#   sudo deploy/rspamd/install.sh --restart       # also restart after a clean test
#   RSPAMD_CONFDIR=/opt/rspamd/etc sudo -E deploy/rspamd/install.sh
#
# This script is syntax-checked in CI but was NOT run against a live Rspamd on
# the development node (no package, no Redis there); see README "ما لم يُثبت هنا".

set -euo pipefail

SRC="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEST="${RSPAMD_CONFDIR:-/etc/rspamd}"
RESTART=0
for arg in "$@"; do
  case "$arg" in
    --restart) RESTART=1 ;;
    *) echo "unknown argument: $arg (only --restart is supported)" >&2; exit 2 ;;
  esac
done

# The three .conf overrides whose absence is the M-19 regression, plus the two
# .inc files. Listed explicitly so the post-copy assertion cannot pass vacuously.
REQUIRED_CONF=(actions.conf dkim_signing.conf multimap.conf)
REQUIRED_INC=(options.inc worker-proxy.inc)

echo "Tira Rspamd install"
echo "  source : $SRC"
echo "  dest   : $DEST"

if [[ ! -d "$SRC/local.d" || ! -d "$SRC/maps" ]]; then
  echo "source layout incomplete: expected $SRC/local.d and $SRC/maps" >&2
  exit 1
fi

if ! command -v rspamadm >/dev/null 2>&1; then
  echo "rspamadm not found in PATH; install Rspamd first (the configtest gate is mandatory)" >&2
  exit 1
fi

mkdir -p "$DEST/local.d" "$DEST/maps"
if [[ ! -w "$DEST/local.d" || ! -w "$DEST/maps" ]]; then
  echo "no write access to $DEST; run as root (sudo) or set RSPAMD_CONFDIR to a writable dir" >&2
  exit 1
fi

# Copy the entire local.d contents: *.inc AND *.conf AND the inert *.disabled.
# `/.` copies directory contents (not the directory itself) and preserves names,
# so redis.conf.disabled stays disabled until an operator renames it.
cp -a "$SRC/local.d/." "$DEST/local.d/"
cp -a "$SRC/maps/." "$DEST/maps/"
chmod 0644 "$DEST/local.d/"* 2>/dev/null || true

# Maps are read by the rspamd worker; give them to the service account when it
# exists. local.d overrides stay root-owned but world-readable (0644 above).
if id rspamd >/dev/null 2>&1; then
  chown -R rspamd:rspamd "$DEST/maps"
else
  echo "  note: no 'rspamd' user found; left $DEST/maps owned by the caller (set ownership manually)" >&2
fi

# M-19 regression guard: prove the three .conf and two .inc actually landed.
missing=0
for f in "${REQUIRED_CONF[@]}" "${REQUIRED_INC[@]}"; do
  if [[ ! -f "$DEST/local.d/$f" ]]; then
    echo "  MISSING after copy: local.d/$f" >&2
    missing=1
  fi
done
if [[ $missing -ne 0 ]]; then
  echo "install incomplete; the .conf overrides are required (this is the M-19 bug)" >&2
  exit 1
fi
echo "  ok  all ${#REQUIRED_CONF[@]} .conf and ${#REQUIRED_INC[@]} .inc overrides present"

# Fail-closed gate: a config that does not parse must never be restarted into.
echo "  running rspamadm configtest"
if ! rspamadm configtest; then
  echo "configtest FAILED; not restarting. A running Rspamd keeps its previous config." >&2
  echo "Fix the staged files under $DEST/local.d and re-run this script." >&2
  exit 1
fi
echo "  ok  configtest passed"

if [[ $RESTART -eq 1 ]]; then
  echo "  restarting rspamd"
  systemctl restart rspamd
  echo "  ok  restarted; verify 11332 is loopback-only: ss -lntp | grep 11332"
else
  echo ""
  echo "Staged and validated. Next (operator):"
  echo "  systemctl restart rspamd"
  echo "  ss -lntp | grep 11332        # must show 127.0.0.1:11332, not 0.0.0.0"
  echo "  scripts/milter-apply.ps1 -Enable   # from the control node, verifies TCP first"
  echo "  send a GTUBE message and check the X-Spam header"
fi

echo ""
echo "RSPAMD INSTALL: PASS"