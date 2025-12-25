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

# Download to a temporary file first
TEMP_FILE=$(mktemp)
if ! curl -fSL "$BINARY_URL" -o "$TEMP_FILE"; then
	echo "Error: Failed to download Oxy binary from GitHub"
	rm -f "$TEMP_FILE"
	exit 1
fi

# Verify the downloaded file is valid (not an HTML error page)
# Check if file starts with ELF (Linux) or Mach-O (macOS) magic bytes
FILE_TYPE=$(file "$TEMP_FILE")
if [[ "$FILE_TYPE" != *"executable"* ]] && [[ "$FILE_TYPE" != *"Mach-O"* ]] && [[ "$FILE_TYPE" != *"ELF"* ]]; then
	echo "Error: Downloaded file is not a valid binary executable"
	echo "File type detected: $FILE_TYPE"
	echo "This usually means the release is missing files on GitHub"
	rm -f "$TEMP_FILE"
	exit 1
fi

# Check if file size is reasonable (at least 1MB for a Rust binary)
FILE_SIZE=$(stat -f%z "$TEMP_FILE" 2>/dev/null || stat -c%s "$TEMP_FILE" 2>/dev/null)
if [ "$FILE_SIZE" -lt 1048576 ]; then
	echo "Error: Downloaded file is too small ($FILE_SIZE bytes)"
	echo "This usually means the release is missing files on GitHub"
	rm -f "$TEMP_FILE"
	exit 1
fi

# Make the binary executable
chmod +x "$TEMP_FILE"

# Move the binary to the install directory (this will replace the old file only if all checks passed)
mv "$TEMP_FILE" "$INSTALL_DIR/oxy"

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
