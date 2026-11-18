#!/bin/sh
# Install muxing: https://github.com/KapitanPapryk/muxing
#
#   curl -fsSL https://raw.githubusercontent.com/KapitanPapryk/muxing/main/install.sh | sh
#
# Environment overrides:
#   MUXING_VERSION      release tag to install, e.g. v0.1.0 (default: latest)
#   MUXING_INSTALL_DIR  where to put the binary (default: ~/.local/bin)
set -eu

REPO="KapitanPapryk/muxing"
VERSION="${MUXING_VERSION:-latest}"
INSTALL_DIR="${MUXING_INSTALL_DIR:-$HOME/.local/bin}"

say() { printf '%s\n' "$*"; }
die() { printf 'muxing install: %s\n' "$*" >&2; exit 1; }

[ "$(uname -s)" = "Linux" ] || die "only Linux is supported (got $(uname -s))"

case "$(uname -m)" in
  x86_64 | amd64) arch=x86_64 ;;
  aarch64 | arm64) arch=aarch64 ;;
  *) die "unsupported architecture: $(uname -m)" ;;
esac
asset="muxing-$arch-unknown-linux-musl.tar.gz"

if [ "$VERSION" = latest ]; then
  base="https://github.com/$REPO/releases/latest/download"
else
  base="https://github.com/$REPO/releases/download/$VERSION"
fi

if command -v curl >/dev/null 2>&1; then
  fetch() { curl -fsSL "$1" -o "$2"; }
elif command -v wget >/dev/null 2>&1; then
  fetch() { wget -qO "$2" "$1"; }
else
  die "curl or wget is required"
fi
command -v tar >/dev/null 2>&1 || die "tar is required"
command -v sha256sum >/dev/null 2>&1 || die "sha256sum is required"
command -v tmux >/dev/null 2>&1 || say "warning: tmux is not installed; muxing needs it at runtime"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT INT TERM

say "downloading $asset ($VERSION)..."
fetch "$base/$asset" "$tmp/$asset" || die "download failed: $base/$asset"
fetch "$base/$asset.sha256" "$tmp/$asset.sha256" || die "download failed: $base/$asset.sha256"
(cd "$tmp" && sha256sum -c --quiet "$asset.sha256") || die "checksum verification failed"

tar -xzf "$tmp/$asset" -C "$tmp"
mkdir -p "$INSTALL_DIR"
install -m 755 "$tmp/muxing-$arch-unknown-linux-musl/muxing" "$INSTALL_DIR/muxing"

say "installed $("$INSTALL_DIR/muxing" --version) to $INSTALL_DIR/muxing"

case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *) say ""
     say "note: $INSTALL_DIR is not on your PATH. Add it, e.g.:"
     say "  bash/zsh: echo 'export PATH=\"$INSTALL_DIR:\$PATH\"' >> ~/.profile"
     say "  fish:     fish_add_path $INSTALL_DIR" ;;
esac

say ""
say "next steps:"
say "  muxing new myproject              # create ~/.config/muxing/myproject.yml"
say "  muxing start myproject            # start and attach"
say "  muxing service install --now      # start 'autostart: true' projects at login"
say "  loginctl enable-linger \$USER      # ...and at boot, without logging in"
