param (
    [string]$InstallDir = "$env:USERPROFILE\.onyx\bin"
)

if (-not (Test-Path -Path $InstallDir)) {
    New-Item -ItemType Directory -Path $InstallDir -Force
}

# Map architecture to target
$arch = (Get-CimInstance Win32_Processor).Architecture
$target = ""

switch ($arch) {
    9 { # x64
        $target = "x86_64-pc-windows-msvc"
    }
    12 { # ARM64
        $target = "aarch64-pc-windows-msvc"
    }
    default {
        Write-Host "Unsupported architecture: $arch"
        exit 1
    }
}

# Get the latest release tag from GitHub API
$repo = "onyx-hq/onyx"
$latestTagUrl = "https://api.github.com/repos/$repo/releases/latest"
$latestTagResponse = Invoke-RestMethod -Uri $latestTagUrl -Headers @{ "User-Agent" = "PowerShell" }
$latestTag = $latestTagResponse.tag_name

# Download the release binary
$binaryUrl = "https://github.com/$repo/releases/download/$latestTag/onyx-$target.exe"
$outputPath = "$InstallDir\onyx.exe"

if (Test-Path -Path $outputPath) {
    Write-Host "Existing Onyx executable found. Upgrading..."
    Remove-Item -Path $outputPath -Force
}

Invoke-WebRequest -Uri $binaryUrl -OutFile $outputPath

# Add the installation directory to the PATH if not already present
$envPath = [System.Environment]::GetEnvironmentVariable("Path", [System.EnvironmentVariableTarget]::User)
if (-not $envPath.Contains($InstallDir)) {
    [System.Environment]::SetEnvironmentVariable("Path", "$envPath;$InstallDir", [System.EnvironmentVariableTarget]::User)
    Write-Host "Added $InstallDir to PATH"
} else {
    Write-Host "$InstallDir is already in PATH"
}

Write-Host "Onyx has been installed to $InstallDir"
