$ErrorActionPreference = "Stop"

$version = "0.1.0"
$repo = "fschutt/git2pdf"
$url = "https://github.com/$repo/releases/download/$version/git2pdf.exe"
$installDir = "$env:USERPROFILE\.git2pdf\bin"

if (!(Test-Path $installDir)) { New-Item -ItemType Directory -Path $installDir -Force | Out-Null }

$dest = Join-Path $installDir "git2pdf.exe"
Write-Host "Downloading git2pdf $version..."
Invoke-WebRequest -Uri $url -OutFile $dest -UseBasicParsing

$path = [Environment]::GetEnvironmentVariable("PATH", "User")
if ($path -notlike "*$installDir*") {
    [Environment]::SetEnvironmentVariable("PATH", "$path;$installDir", "User")
    Write-Host "Added $installDir to PATH (restart your terminal to use)."
}

Write-Host "git2pdf $version installed to $dest"
