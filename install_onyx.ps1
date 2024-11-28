param (
    [string]$InstallDir = "$HOME/.onyx/bin"
)

# Create the installation directory if it doesn't exist
if (-not (Test-Path -Path $InstallDir)) {
    New-Item -ItemType Directory -Path $InstallDir -Force
}

# Determine the shell and update the PATH
$shellName = $env:SHELL -replace '.*/', ''
switch ($shellName) {
    "bash" {
        Add-Content -Path "$HOME/.bashrc" -Value "export PATH=`$PATH:$InstallDir"
        . "$HOME/.bashrc"
    }
    "zsh" {
        Add-Content -Path "$HOME/.zshrc" -Value "export PATH=`$PATH:$InstallDir"
        . "$HOME/.zshrc"
    }
    default {
        Write-Host "Unsupported shell: $shellName. Please add $InstallDir to your PATH manually before installing this tool"
    }
}

# Map architecture to target
$arch = (Get-CimInstance Win32_Processor).Architecture
$os = $env:OS
$target = ""

switch ($arch) {
    9 { # x64
        if ($os -eq "Darwin") {
            $target = "x86_64-apple-darwin"
        } else {
            $target = "x86_64-unknown-linux-gnu"
        }
    }
    12 { # ARM64
        if ($os -eq "Darwin") {
            $target = "aarch64-apple-darwin"
        } else {
            $target = "aarch64-unknown-linux-gnu"
        }
    }
    default {
        Write-Host "Unsupported architecture: $arch"
        exit 1
    }
}

# Download the release binary
$repo = "your-repo/onyx"
$latestTag = "v1.0.0" # Replace with the actual latest tag or fetch dynamically
$binaryUrl = "https://github.com/$repo/releases/download/$latestTag/onyx-$target"
$outputPath = "$InstallDir/onyx-$target"

Invoke-WebRequest -Uri $binaryUrl -OutFile $outputPath

# Make the binary executable
chmod +x $outputPath

Write-Host "Onyx has been installed to $InstallDir"