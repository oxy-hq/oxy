#!/bin/bash

REPO="onyx-hq/onyx-public-releases"
INSTALL_DIR="$HOME/.local/bin"
CONFIG_DIR="$HOME/.config/onyx"
CONFIG_FILE="config.yml"

# Ensure the install directory exists
mkdir -p "$INSTALL_DIR"

# Get the latest release tag from GitHub API
LATEST_TAG=$(curl -s https://api.github.com/repos/$REPO/releases/latest | grep 'tag_name' | cut -d\" -f4)

# Determine the OS and architecture
OS=$(uname | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

# Ensure the install directory is in the PATH
if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
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

# Check if the config file exists and compare it before downloading the latest file
if [ -f "$CONFIG_DIR/$CONFIG_FILE" ]; then
	curl -L https://raw.githubusercontent.com/$REPO/main/example_config.yml >"$CONFIG_DIR/$CONFIG_FILE.latest"
	echo "Config file already exists. The latest version will be saved as $CONFIG_FILE.latest"
	echo "To show the differences, run: diff $CONFIG_DIR/$CONFIG_FILE $CONFIG_DIR/$CONFIG_FILE.latest"
	echo "Or open $CONFIG_DIR/$CONFIG_FILE.latest in your favorite editor"
else
	echo "Config file does not exist. Downloading the latest version..."
	curl -L https://raw.githubusercontent.com/$REPO/main/example_config.yml -o "$CONFIG_DIR/$CONFIG_FILE"
fi

echo "Onyx has been installed successfully!"
