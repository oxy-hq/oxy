#!/bin/bash

set -e

APP_NAME="onyx"
INSTALL_DIR="$HOME/.local/bin"
REPO="onyx-hq/onyx-public-releases"

# Get the version to install from the environment, default to the latest release tag if not provided
VERSION=${ONYX_VERSION:-latest}

# Get the latest release tag from GitHub API if version is latest
if [ "$VERSION" == "latest" ]; then
  VERSION=$(curl -s https://api.github.com/repos/$REPO/releases/latest | grep 'tag_name' | cut -d\" -f4)
fi

# Ensure the install directory exists
mkdir -p "$INSTALL_DIR"

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

# Detect OS and architecture
OS=$(uname -s)
ARCH=$(uname -m)

# Determine the correct app package to download
case "$OS" in
    Linux)
        if [ -f /etc/debian_version ]; then
            PACKAGE_TYPE="deb"
        elif [ -f /etc/redhat-release ]; then
            PACKAGE_TYPE="rpm"
        else
            PACKAGE_TYPE="AppImage"
        fi
        case "$ARCH" in
            x86_64)
                PACKAGE="${APP_NAME}_${VERSION}_amd64.${PACKAGE_TYPE}"
                ;;
            aarch64)
                PACKAGE="${APP_NAME}_${VERSION}_aarch64.${PACKAGE_TYPE}"
                ;;
            *)
                echo "Unsupported architecture: $ARCH"
                exit 1
                ;;
        esac
        ;;
    Darwin)
        case "$ARCH" in
            x86_64)
                PACKAGE="${APP_NAME}_${VERSION}_amd64.dmg"
                ;;
            arm64)
                PACKAGE="${APP_NAME}_${VERSION}_aarch64.dmg"
                ;;
            *)
                echo "Unsupported architecture: $ARCH"
                exit 1
                ;;
        esac
        ;;
    *)
        echo "Unsupported OS: $OS"
        exit 1
        ;;
esac

# Download the app package
echo "Downloading $PACKAGE..."
RELEASE_URL="https://github.com/$REPO/releases/download/$VERSION"
curl -L -o "$PACKAGE" "$RELEASE_URL/$PACKAGE"

# Handle macOS DMG file
if [[ "$OS" == "Darwin" ]]; then
    echo "Opening $PACKAGE..."
    open "$PACKAGE"
    echo "Please follow the instructions to install $APP_NAME."
    exit 0
fi

# Handle Linux installation
echo "Installing $PACKAGE..."
case "$PACKAGE_TYPE" in
    deb)
        sudo dpkg -i "$PACKAGE"
        ;;
    rpm)
        sudo rpm -i "$PACKAGE"
        ;;
    AppImage)
        chmod +x "$PACKAGE"
        sudo mv "$PACKAGE" "$INSTALL_DIR/$APP_NAME"
        ;;
    *)
        echo "Unsupported Linux package type: $PACKAGE_TYPE"
        exit 1
        ;;
esac

echo "$APP_NAME has been installed."
