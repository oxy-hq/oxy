#!/bin/bash

REPO="oxy-hq/oxy-nightly"

if [ "$(id -u)" -eq 0 ]; then
	INSTALL_DIR="/usr/local/bin"
else
	INSTALL_DIR="$HOME/.local/bin"
fi

# Ensure the install directory exists
mkdir -p "$INSTALL_DIR"

# Get the channel (edge or nightly) and version from environment variables
# Channel: 'edge' for latest main builds, 'nightly' for scheduled daily builds
# Version: specific tag (e.g., 'edge-7cbf0a5', 'nightly-20251204-7cbf0a5') or 'latest'
CHANNEL=${OXY_CHANNEL:-edge}
VERSION=${OXY_VERSION:-latest}

# Determine the OS and architecture
OS=$(uname | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

# Ensure the install directory is in the PATH (only for user-specific installation)
if [ "$(id -u)" -ne 0 ] && [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
	echo "The install directory is not in your PATH. Adding it now..."
	SHELL_NAME=$(basename "$SHELL")
	case "$SHELL_NAME" in
	bash)
		echo "export PATH=\$PATH:$INSTALL_DIR" >>"$HOME/.bashrc"
		source "$HOME/.bashrc"
		;;
	zsh)
		echo "export PATH=\$PATH:$INSTALL_DIR" >>"$HOME/.zshrc"
		source "$HOME/.zshrc"
		;;
	*)
		echo "Unsupported shell: $SHELL_NAME. Please add $INSTALL_DIR to your PATH manually before installing this tool"
		;;
	esac
fi

# Map architecture to target
case $ARCH in
x86_64)
	TARGET="x86_64-unknown-linux-gnu"
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

# Determine the tag to download
if [ "$VERSION" == "latest" ]; then
	# For 'latest', use the GitHub latest release (which is always the most recent edge or nightly)
	TAG="latest"
	echo "Installing latest $CHANNEL build of Oxy..."
else
	# Use the specific version tag provided
	TAG="$VERSION"
	echo "Installing Oxy version $TAG..."
fi

# Download the release binary
if [ "$TAG" == "latest" ]; then
	BINARY_URL="https://github.com/$REPO/releases/latest/download/oxy-$TARGET"
else
	BINARY_URL="https://github.com/$REPO/releases/download/$TAG/oxy-$TARGET"
fi

echo "Downloading from: $BINARY_URL"
if ! curl -L "$BINARY_URL" -o oxy-$TARGET; then
	echo "Error: Failed to download Oxy binary"
	exit 1
fi

# Make the binary executable
chmod +x oxy-$TARGET

# Move the binary to the install directory
mv oxy-$TARGET $INSTALL_DIR/oxy

echo ""
echo "✅ Oxy has been installed successfully!"
echo "   Location: $INSTALL_DIR/oxy"
echo "   Target: $TARGET"
if [ "$TAG" == "latest" ]; then
	echo "   Channel: $CHANNEL (latest)"
else
	echo "   Version: $TAG"
fi
echo ""
echo "Run 'oxy --version' to verify the installation."
