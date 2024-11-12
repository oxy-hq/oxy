#!/bin/bash

REPO="onyx-hq/onyx-public-releases"
INSTALL_DIR="$HOME/.local/bin"
CONFIG_DIR="$HOME/.config/onyx"
CONFIG_FILE="config.yml"

# Get the latest release tag from GitHub API
LATEST_TAG=$(curl -s https://api.github.com/repos/$REPO/releases/latest | grep 'tag_name' | cut -d\" -f4)

# Determine the OS and architecture
OS=$(uname | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

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

# Download the release binary
BINARY_URL="https://github.com/$REPO/releases/download/$LATEST_TAG/onyx-$TARGET"
curl -L $BINARY_URL -o onyx

# Make the binary executable
chmod +x onyx

# Move the binary to the install directory
mv onyx $INSTALL_DIR/onyx

# Create the config directory if it doesn't exist
mkdir -p "$CONFIG_DIR"

# Copy the example config file to the config directory
curl -L https://raw.githubusercontent.com/$REPO/main/example_config.yml -o "$CONFIG_DIR"/$CONFIG_FILE

echo "Onyx has been installed successfully!"
