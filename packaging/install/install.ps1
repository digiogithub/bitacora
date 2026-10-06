# Install bitacora-cli from GitHub Releases (Windows PowerShell).
#   irm https://github.com/digio-es/bitacora/releases/latest/download/install.ps1 | iex
# Environment: BITACORA_VERSION (default: latest), BITACORA_INSTALL_DIR (default: %LOCALAPPDATA%\Programs\bitacora-cli).
# The archive is verified against the release SHA256SUMS before anything is installed.
$ErrorActionPreference = 'Stop'

$repo = 'digio-es/bitacora'
$dir = if ($env:BITACORA_INSTALL_DIR) { $env:BITACORA_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA 'Programs\bitacora-cli' }
$target = 'x86_64-pc-windows-msvc'

if ($env:BITACORA_VERSION) {
  $version = $env:BITACORA_VERSION.TrimStart('v')
  $base = "https://github.com/$repo/releases/download/v$version"
} else {
  $base = "https://github.com/$repo/releases/latest/download"
  $resp = Invoke-WebRequest -Uri "https://github.com/$repo/releases/latest" -MaximumRedirection 0 -ErrorAction SilentlyContinue
  $loc = $resp.Headers['Location']
  if (-not $loc) { throw 'cannot resolve the latest release' }
  $version = ([string]$loc).Split('/')[-1].TrimStart('v')
}

$name = "bitacora-cli-$version-$target.zip"
$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ([System.Guid]::NewGuid().ToString())
New-Item -ItemType Directory -Path $tmp | Out-Null
try {
  Invoke-WebRequest -Uri "$base/$name" -OutFile (Join-Path $tmp $name)
  Invoke-WebRequest -Uri "$base/SHA256SUMS" -OutFile (Join-Path $tmp 'SHA256SUMS')
  $line = Get-Content (Join-Path $tmp 'SHA256SUMS') | Where-Object { $_ -match "  $([regex]::Escape($name))$" } | Select-Object -First 1
  if (-not $line) { throw "no checksum for $name" }
  $want = $line.Split(' ')[0].ToLower()
  $got = (Get-FileHash (Join-Path $tmp $name) -Algorithm SHA256).Hash.ToLower()
  if ($want -ne $got) { throw "checksum mismatch for $name" }
  Expand-Archive -Path (Join-Path $tmp $name) -DestinationPath $tmp
  New-Item -ItemType Directory -Force -Path $dir | Out-Null
  Copy-Item (Join-Path $tmp "bitacora-cli-$version-$target\bitacora-cli.exe") (Join-Path $dir 'bitacora-cli.exe') -Force
  Write-Host "installed bitacora-cli $version to $dir"
  Write-Host "note: add $dir to your PATH"
} finally {
  Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}
