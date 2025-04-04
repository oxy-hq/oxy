#!/bin/bash

REPO="oxy-hq/oxy"
INSTALL_DIR="$HOME/.local/bin"

# Ensure the install directory exists
mkdir -p "$INSTALL_DIR"

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

# Ensure unzip is installed
if ! command -v unzip >/dev/null 2>&1; then
	echo "The 'unzip' command is required but not installed. Please install 'unzip' and try again."
	exit 1
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

# Fetch the latest nightly build artifact URL
ARTIFACT_URL=$(curl -s "https://api.github.com/repos/$REPO/actions/artifacts" | jq -r ".artifacts[] | select(.name == \"nightly-$TARGET\") | .archive_download_url" | head -n 1)

if [ -z "$ARTIFACT_URL" ]; then
	echo "Failed to find the latest nightly build for $TARGET."
	exit 1
fi

echo "Downloading the latest nightly build for $TARGET from $ARTIFACT_URL..."

# Download the artifact
curl -L -H "Authorization: token $GITHUB_TOKEN" "$ARTIFACT_URL" -o nightly-artifact.zip

# Validate the downloaded file
if ! unzip -tq nightly-artifact.zip >/dev/null 2>&1; then
	echo "The downloaded file is not a valid zip archive. Please check the artifact URL or your network connection."
	rm -f nightly-artifact.zip
	exit 1
fi

# Extract the binary from the artifact
unzip nightly-artifact.zip -d nightly-artifact
BINARY_PATH=$(find nightly-artifact -type f -name "oxy-$TARGET")

if [ -z "$BINARY_PATH" ]; then
	echo "Failed to find the binary for $TARGET in the artifact."
	rm -rf nightly-artifact nightly-artifact.zip
	exit 1
fi

mv "$BINARY_PATH" "$INSTALL_DIR/oxy"

# Make the binary executable
chmod +x "$INSTALL_DIR/oxy"

# Cleanup
rm -rf nightly-artifact nightly-artifact.zip

echo "Oxy nightly version for $TARGET has been installed successfully!"
