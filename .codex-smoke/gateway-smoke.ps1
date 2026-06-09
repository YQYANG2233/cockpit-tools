$ErrorActionPreference = 'Stop'

$RepoRoot = Split-Path -Parent $PSScriptRoot
$WorkspaceRoot = Split-Path -Parent $RepoRoot
$NodeDir = Join-Path $WorkspaceRoot 'toolchain\node\node-v24.14.0-win-x64'
if (Test-Path $NodeDir) {
  $env:PATH = "$NodeDir;$env:PATH"
}

$ServicePort = 19580 + (Get-Random -Minimum 0 -Maximum 80)
$WebPort = 18180 + (Get-Random -Minimum 0 -Maximum 80)
$env:COCKPIT_TOOLS_RPC_TOKEN = "smoke-token-$([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds())"
$env:COCKPIT_TOOLS_SERVICE_ADDR = "127.0.0.1:$ServicePort"
$env:COCKPIT_TOOLS_WEB_ADDR = "127.0.0.1:$WebPort"
$env:COCKPIT_TOOLS_WEB_ROOT = Join-Path $RepoRoot 'dist'

$ServiceExe = Join-Path $RepoRoot 'target\debug\cockpit-service.exe'
$WebExe = Join-Path $RepoRoot 'target\debug\cockpit-web.exe'

if (!(Test-Path $ServiceExe)) {
  throw "Missing $ServiceExe; run cargo build -p cockpit-service -p cockpit-web first."
}
if (!(Test-Path $WebExe)) {
  throw "Missing $WebExe; run cargo build -p cockpit-service -p cockpit-web first."
}
if (!(Test-Path $env:COCKPIT_TOOLS_WEB_ROOT)) {
  throw "Missing $env:COCKPIT_TOOLS_WEB_ROOT; run npm run build first."
}

$Children = @()
try {
  $Children += Start-Process -FilePath $ServiceExe -WorkingDirectory $RepoRoot -WindowStyle Hidden -PassThru
  $Children += Start-Process -FilePath $WebExe -WorkingDirectory $RepoRoot -WindowStyle Hidden -PassThru
  & node (Join-Path $PSScriptRoot 'gateway-smoke.mjs')
  if ($LASTEXITCODE -ne 0) {
    throw "gateway smoke failed with exit code $LASTEXITCODE"
  }
}
finally {
  foreach ($Child in $Children) {
    if ($null -ne $Child -and !$Child.HasExited) {
      Stop-Process -Id $Child.Id -Force
      Wait-Process -Id $Child.Id -ErrorAction SilentlyContinue
    }
  }
}
