#!/usr/bin/env bash
# Install the system packages needed to build Bitacora on Ubuntu 24.04 (apt based).
#
#   script/install-linux-deps.sh            # everything (GPUI app: bitacora-app)
#   script/install-linux-deps.sh --minimal  # only what the GPUI-free crates need (CI `test-core`)
#
# Idempotent. Uses sudo only when not running as root. We deliberately do not install
# libgit2-dev or libsqlite3-dev: SQLite is bundled and git goes through the git CLI / gix.
set -euo pipefail

minimal=0
for arg in "$@"; do
  case "$arg" in
    --minimal) minimal=1 ;;
    -h | --help)
      sed -n '2,9p' "$0" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *)
      echo "unknown argument: $arg" >&2
      exit 2
      ;;
  esac
done

if [[ "$(uname -s)" != "Linux" ]] || ! command -v apt-get > /dev/null 2>&1; then
  echo "install-linux-deps.sh supports apt-based Linux only (Ubuntu 24.04); install the equivalents manually." >&2
  exit 1
fi

sudo_cmd=()
if [[ "$(id -u)" -ne 0 ]]; then
  sudo_cmd=(sudo)
fi

# Compiler toolchain and build helpers (also enough for the GPUI-free crates).
minimal_packages=(
  gcc g++ clang cmake pkg-config mold
  libssl-dev libzstd-dev
  libdbus-1-dev # keyring / secret-service
)

# GPUI (bitacora-app): fonts, windowing, GPU, audio.
gui_packages=(
  libfontconfig-dev libfreetype-dev
  libwayland-dev libxkbcommon-dev libxkbcommon-x11-dev libx11-xcb-dev libxcb1-dev
  libvulkan1 mesa-vulkan-drivers
  libasound2-dev
)

packages=("${minimal_packages[@]}")
if [[ "$minimal" -eq 0 ]]; then
  packages+=("${gui_packages[@]}")
fi

export DEBIAN_FRONTEND=noninteractive
"${sudo_cmd[@]}" apt-get update
"${sudo_cmd[@]}" apt-get install -y --no-install-recommends "${packages[@]}"
echo "installed ${#packages[@]} packages ($([[ "$minimal" -eq 1 ]] && echo minimal || echo full))"
