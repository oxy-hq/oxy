param (
    [string]$InstallDir = "$env:USERPROFILE\.oxy\bin"
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


# Download the release binary
$binaryUrl = "https://github.com/$repo/releases/download/latest/oxy-$target.exe"
$outputPath = "$InstallDir\oxy.exe"

if (Test-Path -Path $outputPath) {
    Write-Host "Existing Oxy executable found. Upgrading..."
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

Write-Host "Oxy has been installed to $InstallDir"
