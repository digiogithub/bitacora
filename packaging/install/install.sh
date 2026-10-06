#!/bin/sh
# Install bitacora-cli from GitHub Releases (Linux and macOS).
#   curl -fsSL https://github.com/digio-es/bitacora/releases/latest/download/install.sh | sh
# Environment: BITACORA_VERSION (default: latest), BITACORA_INSTALL_DIR (default: ~/.local/bin).
# The archive is verified against the release SHA256SUMS before anything is installed.
set -eu

REPO="digio-es/bitacora"
DIR="${BITACORA_INSTALL_DIR:-$HOME/.local/bin}"

case "$(uname -s)" in
  Linux) os=unknown-linux-gnu ;;
  Darwin) os=apple-darwin ;;
  *) echo "unsupported OS: $(uname -s)" >&2; exit 1 ;;
esac
case "$(uname -m)" in
  x86_64 | amd64) arch=x86_64 ;;
  aarch64 | arm64) arch=aarch64 ;;
  *) echo "unsupported CPU: $(uname -m)" >&2; exit 1 ;;
esac
target="$arch-$os"

if [ -n "${BITACORA_VERSION:-}" ]; then
  version="${BITACORA_VERSION#v}"
  base="https://github.com/$REPO/releases/download/v$version"
else
  base="https://github.com/$REPO/releases/latest/download"
  version=$(curl -fsSLI -o /dev/null -w '%{url_effective}' "https://github.com/$REPO/releases/latest" | sed 's|.*/v||')
fi

name="bitacora-cli-$version-$target.tar.gz"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

curl -fsSL "$base/$name" -o "$tmp/$name"
curl -fsSL "$base/SHA256SUMS" -o "$tmp/SHA256SUMS"

want=$(grep "  $name\$" "$tmp/SHA256SUMS" | cut -d' ' -f1)
[ -n "$want" ] || { echo "no checksum for $name" >&2; exit 1; }
if command -v sha256sum >/dev/null 2>&1; then
  got=$(sha256sum "$tmp/$name" | cut -d' ' -f1)
else
  got=$(shasum -a 256 "$tmp/$name" | cut -d' ' -f1)
fi
[ "$want" = "$got" ] || { echo "checksum mismatch for $name" >&2; exit 1; }

tar -xzf "$tmp/$name" -C "$tmp"
mkdir -p "$DIR"
install -m 755 "$tmp/bitacora-cli-$version-$target/bitacora-cli" "$DIR/bitacora-cli"
echo "installed bitacora-cli $version to $DIR/bitacora-cli"
case ":$PATH:" in *":$DIR:"*) ;; *) echo "note: add $DIR to your PATH" ;; esac
