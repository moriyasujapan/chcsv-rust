#!/usr/bin/env bash
# chcsv-rust build prerequisites fetcher.
#
# The repository intentionally does not contain Oracle-related binaries or
# third-party sources. This script fetches what is needed to build:
#
#   1. ODPI-C sources (MIT / Apache-2.0) into ./vendor/odpi
#      -> statically compiled into the binary by build.rs; loads the Oracle
#         client library at runtime (dlopen), so nothing Oracle is needed
#         at link time.
#   2. (manual) Oracle Instant Client — proprietary license, download it
#      yourself from the Oracle site:
#      https://www.oracle.com/database/technologies/instant-client/linux-x86-64-downloads.html
#      Unpack a basic/basic-lite ZIP for your platform into ./instantclient_21_21
#      (any directory name works; export LD_LIBRARY_PATH pointing at it).
#
# Usage:
#   ./setup.sh                          # fetch ODPI-C only
#   ./setup.sh --with-instant-client URL # also download+unpack an IC ZIP
set -euo pipefail
cd "$(dirname "$0")"

# --- 1. ODPI-C ---------------------------------------------------------------
if [ -f vendor/odpi/include/dpi.h ]; then
  echo "vendor/odpi: already present"
else
  echo "cloning ODPI-C into vendor/odpi ..."
  git clone --depth 1 https://github.com/oracle/odpi.git vendor/odpi
fi

# --- 2. Oracle Instant Client (optional convenience) ------------------------
if [ "${1:-}" = "--with-instant-client" ]; then
  url="${2:?usage: --with-instant-client <ZIP URL from oracle.com>}"
  echo "downloading Instant Client ..."
  curl -fL -o /tmp/instantclient.zip "$url"
  unzip -q -o /tmp/instantclient.zip -d .
  icdir=$(find . -maxdepth 1 -type d -name 'instantclient_*' | head -1)
  echo "Instant Client unpacked into $icdir"
else
  cat <<'MSG'

Next: obtain Oracle Instant Client (proprietary — not in this repo):
  https://www.oracle.com/database/technologies/instant-client/linux-x86-64-downloads.html
and unpack it into this directory, e.g. ./instantclient_21_21

MSG
fi

# --- 3. libaio shims (Ubuntu 24.04 ships only libaio.so.1t64) ---------------
for d in instantclient_*; do
  [ -d "$d" ] || continue
  if ! ldconfig -p 2>/dev/null | grep -q 'libaio\.so\.1 '; then
    for cand in /usr/lib/x86_64-linux-gnu/libaio.so.1t64 /usr/lib64/libaio.so.1t64; do
      if [ -e "$cand" ]; then
        ln -sf "$cand" "$d/libaio.so.1"
        echo "linked libaio shim: $d/libaio.so.1 -> $cand"
        break
      fi
    done
  fi
done

echo
echo "Now build with:  cargo build --release"
echo "Run with:        LD_LIBRARY_PATH=$PWD/$(ls -d instantclient_* 2>/dev/null | head -1 || echo instantclient_21_21) ./target/release/chcsv ..."
