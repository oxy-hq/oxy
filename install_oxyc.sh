#!/bin/bash
#
# Install `oxyc`, the Oxy CLI — one self-contained binary, no Node, no npm.
#
# Deliberately shaped like install_oxy.sh next to it: same install directory
# rules, same uname-to-target mapping, same release host. The binaries carry
# their own `template/`, `skills/` and `json-schemas/` inside them, so a single
# downloaded file is a complete installation.
#
#   curl -fsSL https://raw.githubusercontent.com/oxy-hq/oxygen/main/install_oxyc.sh | bash
#
# OXYC_VERSION pins a release tag; the default is the latest.

set -euo pipefail

REPO="oxy-hq/oxygen"

if [ "$(id -u)" -eq 0 ]; then
	INSTALL_DIR="/usr/local/bin"
else
	INSTALL_DIR="$HOME/.local/bin"
fi

mkdir -p "$INSTALL_DIR"

VERSION=${OXYC_VERSION:-latest}

OS=$(uname | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

# Ensure the install directory is in the PATH (only for user-specific installation)
if [ "$(id -u)" -ne 0 ] && [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
	echo "The install directory is not in your PATH. Adding it now..."
	SHELL_NAME=$(basename "${SHELL:-}")
	case "$SHELL_NAME" in
	bash)
		echo "export PATH=\$PATH:$INSTALL_DIR" >>"$HOME/.bashrc"
		;;
	zsh)
		echo "export PATH=\$PATH:$INSTALL_DIR" >>"$HOME/.zshrc"
		;;
	*)
		echo "Unsupported shell: $SHELL_NAME. Please add $INSTALL_DIR to your PATH manually."
		;;
	esac
	# Only affects THIS script's remaining commands — the parent shell needs a
	# new session, which the closing message says.
	export PATH="$PATH:$INSTALL_DIR"
fi

# Map architecture to target
case $ARCH in
x86_64)
	if [ "$OS" == "darwin" ]; then
		TARGET="x86_64-apple-darwin"
	else
		TARGET="x86_64-unknown-linux-gnu"
	fi
	;;
aarch64 | arm64)
	if [ "$OS" == "darwin" ]; then
		TARGET="aarch64-apple-darwin"
	else
		TARGET="aarch64-unknown-linux-gnu"
	fi
	;;
*)
	echo "Unsupported architecture: $ARCH"
	exit 1
	;;
esac

if [ "$VERSION" == "latest" ]; then
	BINARY_URL="https://github.com/$REPO/releases/latest/download/oxyc-$TARGET"
else
	BINARY_URL="https://github.com/$REPO/releases/download/$VERSION/oxyc-$TARGET"
fi

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

# --fail so an HTML 404 page is never written out and chmod'd as a "binary";
# without it the failure surfaces later as a confusing exec format error.
if ! curl -fsSL "$BINARY_URL" -o "$TMP/oxyc"; then
	echo "Failed to download $BINARY_URL"
	echo "No release asset for $TARGET at version $VERSION."
	exit 1
fi

chmod +x "$TMP/oxyc"
mv "$TMP/oxyc" "$INSTALL_DIR/oxyc"

echo "oxyc ($VERSION, $TARGET) installed to $INSTALL_DIR/oxyc"
echo ""
if command -v oxyc >/dev/null 2>&1; then
	echo "Try:  oxyc --help"
else
	echo "Open a new terminal (or add $INSTALL_DIR to PATH), then: oxyc --help"
fi
