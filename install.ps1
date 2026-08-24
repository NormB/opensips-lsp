# opensips-lsp one-command installer for Windows (PowerShell).
#
#   irm https://raw.githubusercontent.com/NormB/opensips-lsp/main/install.ps1 | iex
#
# Downloads the latest release for this machine, installs the server
# to %LOCALAPPDATA%\opensips-lsp, and installs the VS Code extension
# into every editor CLI found (code, code-insiders, codium).
$ErrorActionPreference = 'Stop'

$repo = 'NormB/opensips-lsp'
$arch = if ($env:PROCESSOR_ARCHITECTURE -eq 'ARM64') { 'aarch64' } else { 'x86_64' }
$tag = (Invoke-RestMethod "https://api.github.com/repos/$repo/releases/latest").tag_name
Write-Host "Installing opensips-lsp $tag for $arch ..."

$dest = Join-Path $env:LOCALAPPDATA 'opensips-lsp'
New-Item -ItemType Directory -Force -Path $dest | Out-Null
$zip = Join-Path $env:TEMP 'opensips-lsp.zip'
Invoke-WebRequest "https://github.com/$repo/releases/download/$tag/opensips-lsp-$tag-$arch-windows.zip" -OutFile $zip
Expand-Archive -Path $zip -DestinationPath $dest -Force
Remove-Item $zip
Write-Host "Server installed: $dest\opensips-lsp.exe"
Write-Host "NOTE: add $dest to your PATH if editors other than VS Code need it."

$vsix = Join-Path $env:TEMP 'opensips-lsp-ext.vsix'
$installed = $false
foreach ($editor in 'code', 'code-insiders', 'codium') {
    if (Get-Command $editor -ErrorAction SilentlyContinue) {
        if (-not (Test-Path $vsix)) {
            Invoke-WebRequest "https://github.com/$repo/releases/download/$tag/opensips-lsp-ext-$tag.vsix" -OutFile $vsix
        }
        & $editor --install-extension $vsix --force | Out-Null
        Write-Host "Extension installed into $editor."
        $installed = $true
    }
}
if ($installed) {
    Write-Host ""
    Write-Host "NOTE: installing from a file means your editor will NOT offer"
    Write-Host "      updates for this extension - a sideloaded VSIX carries no"
    Write-Host "      marketplace metadata. To get updates, either re-run this"
    Write-Host "      script, or install it from the Extensions view instead."
}
if (-not $installed) {
    Write-Host "No editor CLI found. Install the extension by hand:"
    Write-Host "  1. Download: https://github.com/$repo/releases/download/$tag/opensips-lsp-ext-$tag.vsix"
    Write-Host "  2. In VS Code press Ctrl+Shift+X, click the '...' menu,"
    Write-Host "     choose 'Install from VSIX...' and pick the file."
}
