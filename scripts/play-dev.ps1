# Development & profiling launcher tuned for 16 GB RAM + 6C/12T CPU + RX 7600 XT 16 GB (1080p).
#   scripts\play-dev.ps1 [-GameDir <path>] [-Build diag|release] [-Console series-s|series-x]
#                        [-Panel] [-DebugLayer] [-DredLight] [-MemoryMap] [-RayTracing] [-LevelsOnDemand] [-InplaceProbe] [-NoKeepShown] [-Warm] [-Safe]
#                        [-LogFile <path>] [-More @{...}]
# Note: -Console series-x is now viable to test on 16 GB VRAM for sharper visuals.
# Run from a normal (non-Administrator) PowerShell window so the GPU driver shader cache stays unified.
param(
    [string]$GameDir = $env:VICTORY_GAME_DIR,
    [ValidateSet('diag', 'release')][string]$Build = 'diag',
    [ValidateSet('series-s', 'series-x')][string]$Console = 'series-s',
    [switch]$Panel,
    [switch]$DebugLayer,
    [switch]$DredLight,
    [switch]$MemoryMap,
    [switch]$RayTracing,
    [switch]$LevelsOnDemand,
    [switch]$InplaceProbe,
    [switch]$NoKeepShown,
    [switch]$Warm,
    [switch]$Safe,
    [string]$LogFile = (Join-Path $env:LOCALAPPDATA 'victory\dev-run.log'),
    [hashtable]$More = @{}
)

if (-not $GameDir) {
    Write-Host "Set -GameDir or `$env:VICTORY_GAME_DIR to your game dump folder containing game_scarlett_master.exe."
    exit 2
}
$Exe = Join-Path $GameDir 'game_scarlett_master.exe'
if (-not (Test-Path $Exe)) {
    Write-Host "No game_scarlett_master.exe found in $GameDir."
    exit 2
}

$victory = Join-Path $PSScriptRoot "..\target\$Build\victory.exe"
if (-not (Test-Path $victory)) {
    Write-Host "No $victory. Build it first:"
    if ($Build -eq 'diag') {
        Write-Host "  cargo build --locked --profile diag -p victory --features diagnostics"
    } else {
        Write-Host "  cargo build --release --locked -p victory"
    }
    exit 2
}

$preservedCache = $env:VICTORY_CACHE_DIR
$preservedData = $env:VICTORY_DATA_DIR
$preservedConfig = $env:VICTORY_CONFIG

Get-ChildItem Env:VICTORY_* | ForEach-Object { Remove-Item "Env:$($_.Name)" }

if ($preservedConfig) { $env:VICTORY_CONFIG = $preservedConfig }
if ($preservedData) { $env:VICTORY_DATA_DIR = $preservedData }
$env:VICTORY_GAME_DIR = (Resolve-Path $GameDir).Path
$env:VICTORY_CACHE_DIR = if ($preservedCache) { $preservedCache } else { Join-Path $env:LOCALAPPDATA 'VIctory\cache-pareto' }

$settings = @{
    VICTORY_WINDOW_MAX           = '1920x1080'
    VICTORY_TEXTURE_NO_WAIT      = '1'
    VICTORY_RELAY                = '2'
    VICTORY_WIDE_BACK            = '1'
    VICTORY_CARD_REHASH_RUNS     = '1'
    VICTORY_JOIN_COPIES          = '1'
    VICTORY_INDIRECT_WINDOW      = '1'
    VICTORY_RELAY_COPY           = '1'
    VICTORY_CARD_STEPS           = '2'
    VICTORY_INDIRECT_BATCH       = '2'
    VICTORY_MAP_THREADS          = '1'
    VICTORY_CARD_AWAKE           = '1'
    VICTORY_SHOW_AFTER           = '1'
    VICTORY_AMBIENT              = '0.8'
    VICTORY_AMBIENT_AIR          = '1.5'
    VICTORY_CARD_BUFFERS_MIN_KB  = '64'
}

if ($Build -eq 'diag') {
    $settings.VICTORY_LOG = 'info'
    $settings.VICTORY_D3D12_DRED = if ($DredLight) { 'light' } else { 'faults' }
    $settings.VICTORY_MEMORY_MAP = if ($MemoryMap) { '1' } else { '0' }
} else {
    $settings.VICTORY_LOG = 'warn'
}

if ($Safe) {
    $settings.VICTORY_UNDER_WIDE = '0'
    $settings.VICTORY_LAND_ECHO = '0'
    $settings.VICTORY_CARD_BUFFERS_MIN_KB = '256'
} else {
    $settings.VICTORY_THIN = '1'
    $settings.VICTORY_JOIN_PARTS = '8'
    $settings.VICTORY_UPLOAD_AHEAD = '1'
    $settings.VICTORY_PLACED_ORDER = '1'
    $settings.VICTORY_COPY_BESIDE = '1'
    $settings.VICTORY_STORE_BESIDE = '1'
    $settings.VICTORY_THIN_THREADS = '6'
    $settings.VICTORY_LAND_THREADS = '6'
    $settings.VICTORY_WIDE_THREADS = '5'
    $settings.VICTORY_WIDE_SPIN = '50'
    $settings.VICTORY_VIEW_THREADS = '5'
    $settings.VICTORY_UNDER_THREADS = '6'
}

if ($Panel) { $settings.VICTORY_PANEL = '1' }
if ($DebugLayer) { $settings.VICTORY_D3D12_DEBUG = '1' }
if ($RayTracing) { $settings.VICTORY_BVH_BUILDER = '1' }
if ($LevelsOnDemand) { $settings.VICTORY_LEVELS_ON_DEMAND = '1' }
if ($InplaceProbe) { $settings.VICTORY_INPLACE_PROBE = '1' }
if ($NoKeepShown) { $settings.VICTORY_KEEP_SHOWN = '0' }

foreach ($k in $More.Keys) { $settings[$k] = [string]$More[$k] }
foreach ($k in $settings.Keys) { Set-Item "Env:$k" $settings[$k] }

if ($Warm) {
    Write-Host "Pre-warming driver pipelines (`$env:VICTORY_CACHE_DIR = $($env:VICTORY_CACHE_DIR))..."
    & $victory warm --used --jobs 6
    if ($LASTEXITCODE -ne 0) {
        Write-Host "warning: victory warm returned exit code $LASTEXITCODE"
    }
}

$logDir = Split-Path -Parent $LogFile
if ($logDir) { New-Item -ItemType Directory -Force -Path $logDir | Out-Null }

$Exe = (Resolve-Path $Exe).Path
Write-Host "Running [$Build] ($Console) -> log: $LogFile"
Write-Host "Cache: $($env:VICTORY_CACHE_DIR)"

$stream = [System.IO.File]::Open($LogFile, [System.IO.FileMode]::Create, [System.IO.FileAccess]::Write, [System.IO.FileShare]::ReadWrite)
$writer = New-Object System.IO.StreamWriter($stream, [System.Text.UTF8Encoding]::new($false))
$writer.AutoFlush = $true
$prevEap = $ErrorActionPreference
$ErrorActionPreference = 'Continue'
$code = 0
Push-Location (Join-Path $PSScriptRoot '..')
try {
    & $victory run $Exe --console $Console --execute 2>&1 | ForEach-Object {
        $line = if ($_ -is [System.Management.Automation.ErrorRecord]) {
            $_.ToString()
        } else {
            [string]$_
        }
        $writer.WriteLine($line)
        [Console]::Out.WriteLine($line)
    }
    $code = $LASTEXITCODE
} finally {
    $ErrorActionPreference = $prevEap
    Pop-Location
    $writer.Dispose()
    $stream.Dispose()
}
exit $code
