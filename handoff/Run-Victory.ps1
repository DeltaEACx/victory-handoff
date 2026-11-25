# Windows PowerShell | run the fork with isolated persistent storage and launch logs.
param(
    [string]$GameDir = $env:VICTORY_GAME_DIR,
    [switch]$SoftwareRenderer,
    [switch]$TraceWorld,
    [switch]$DebugD3D,
    [switch]$FirstBoot,
    [switch]$CinematicFast,
    [switch]$IsolatedBuild,
    [switch]$MeshMaskFallback,
    [switch]$SrgbViews,
    [switch]$NoCapture,
    [switch]$PresentTokenTrace,
    [string]$CaptureFrames,
    [string]$PackedLightAfter,
    [switch]$DumpTargets,
    [switch]$CheckPaths
)
$ErrorActionPreference = 'Stop'
if ($CinematicFast -and $SoftwareRenderer) {
    throw 'CinematicFast needs the hardware renderer; omit SoftwareRenderer.'
}
if (($MeshMaskFallback -or $SrgbViews) -and -not $IsolatedBuild) {
    throw 'MeshMaskFallback and SrgbViews require IsolatedBuild.'
}
if ($PresentTokenTrace -and (-not $IsolatedBuild -or $CinematicFast -or $PackedLightAfter)) {
    throw 'PresentTokenTrace requires IsolatedBuild and a separate diagnostic run.'
}
if ($NoCapture -and ($TraceWorld -or $CaptureFrames -or $DumpTargets -or $PackedLightAfter)) {
    throw 'NoCapture cannot be combined with a capture switch.'
}
if ($CaptureFrames -and $CaptureFrames -notmatch '^[1-9][0-9]{0,9}(,[1-9][0-9]{0,9}){0,15}$') {
    throw 'CaptureFrames expects up to 16 comma-separated positive frame numbers.'
}
if ($CaptureFrames -and ($TraceWorld -or $CinematicFast)) {
    throw 'For a targeted capture, omit TraceWorld and CinematicFast.'
}
if ($DumpTargets -and (-not $CaptureFrames -or ($CaptureFrames -split ',').Count -gt 3)) {
    throw 'DumpTargets requires CaptureFrames with at most three frame numbers.'
}
if ($PackedLightAfter) {
    if (-not $IsolatedBuild -or $CaptureFrames -or $DumpTargets -or $TraceWorld -or $CinematicFast) {
        throw 'PackedLightAfter requires IsolatedBuild and its own capture run.'
    }
    if ($PackedLightAfter -notmatch '^[1-9][0-9]{0,4}(,[1-9][0-9]{0,4}){0,2}$' -or
        ($PackedLightAfter -split ',' | Where-Object { [int]$_ -gt 65535 })) {
        throw 'PackedLightAfter expects one to three comma-separated offsets from 1 to 65535.'
    }
}
$repo = Split-Path -Parent $PSScriptRoot
$local = Join-Path $env:LOCALAPPDATA 'victory-handoff'
$data = Join-Path $local 'data'
$captures = Join-Path $local 'captures'
$logs = Join-Path $local 'logs'
if ($FirstBoot) {
    # The title owns its setup UI. Give it empty title storage and saves
    # without renaming, copying, or resetting the normal profile.
    $id = [guid]::NewGuid().ToString('N')
    $session = Join-Path $local "first-boot\$id"
    $data = Join-Path $session 'data'
    $captures = Join-Path $session 'captures'
    $logs = Join-Path $session 'logs'
}
elseif ($IsolatedBuild) {
    $captures = Join-Path $captures ('isolated-' + [guid]::NewGuid().ToString('N'))
}
if (-not $GameDir) {
    throw 'Pass -GameDir or set VICTORY_GAME_DIR to the directory containing your own title executable.'
}
$env:VICTORY_GAME_DIR = (Get-Item -LiteralPath $GameDir).FullName
$env:VICTORY_CACHE_DIR = Join-Path $local 'cache'
$env:VICTORY_DATA_DIR = $data
$env:VICTORY_CAPTURE = $captures
$env:VICTORY_LOG = 'info'
if ($DebugD3D) { $env:VICTORY_D3D12_DEBUG = '1' }
else { Remove-Item Env:VICTORY_D3D12_DEBUG -ErrorAction SilentlyContinue }
if ($IsolatedBuild) {
    # A comparison run without a switch must not inherit its previous setting.
    Remove-Item Env:VICTORY_WIPER_MASK_REPLACE, Env:VICTORY_RGBA8_SRGB_VIEWS, Env:VICTORY_PRESENT_PROBE -ErrorAction SilentlyContinue
}
Remove-Item Env:VICTORY_PACKED_LIGHT_AFTER -ErrorAction SilentlyContinue
Remove-Item Env:VICTORY_PRESENT_TOKEN_TRACE -ErrorAction SilentlyContinue
if ($PresentTokenTrace) { $env:VICTORY_PRESENT_TOKEN_TRACE = '1' }
if ($MeshMaskFallback) { $env:VICTORY_WIPER_MASK_REPLACE = '1' }
if ($SrgbViews) { $env:VICTORY_RGBA8_SRGB_VIEWS = '1' }
if ($CaptureFrames) {
    Remove-Item Env:VICTORY_CAPTURE_WORLD, Env:VICTORY_CAPTURE_WORLD_ALSO, Env:VICTORY_DUMP_TARGETS -ErrorAction SilentlyContinue
    $env:VICTORY_CAPTURE_FRAMES = $CaptureFrames
}
if ($DumpTargets) { $env:VICTORY_DUMP_TARGETS = '1' }
if ($PackedLightAfter) {
    Remove-Item Env:VICTORY_CAPTURE_WORLD, Env:VICTORY_CAPTURE_WORLD_ALSO, Env:VICTORY_CAPTURE_FRAMES -ErrorAction SilentlyContinue
    $env:VICTORY_PACKED_LIGHT_AFTER = $PackedLightAfter
    $env:VICTORY_DUMP_TARGETS = '1'
}
if ($NoCapture) {
    Remove-Item Env:VICTORY_CAPTURE, Env:VICTORY_CAPTURE_WORLD, Env:VICTORY_CAPTURE_WORLD_ALSO, Env:VICTORY_CAPTURE_FRAMES, Env:VICTORY_DUMP_TARGETS -ErrorAction SilentlyContinue
    $env:VICTORY_PRESENT_PROBE = '0'
}
if ($TraceWorld) {
    $env:VICTORY_CAPTURE_WORLD = '1'
    $env:VICTORY_CAPTURE_WORLD_ALSO = '1,10,20,60,120'
    $env:VICTORY_D3D12_DRED = '1'
}
if ($SoftwareRenderer) { $env:VICTORY_D3D12_WARP = '1' }
else { Remove-Item Env:VICTORY_D3D12_WARP -ErrorAction SilentlyContinue }
if ($CinematicFast) {
    # Relay mode overlaps GPU execution, preparation, and copy-back. GPU
    # timing and kept snapshots disable it internally.
    $env:VICTORY_LATE_WAIT = '3'
    $env:VICTORY_RELAY = '2'
    Remove-Item Env:VICTORY_GPU_TIMING, Env:VICTORY_KEEP_SNAPSHOTS -ErrorAction SilentlyContinue
    if (-not $TraceWorld) {
        # A captured present forces a GPU wait and writes a PNG on the
        # presenting thread. Leave captures out of the performance run.
        Remove-Item Env:VICTORY_CAPTURE, Env:VICTORY_CAPTURE_WORLD, Env:VICTORY_CAPTURE_WORLD_ALSO, Env:VICTORY_CAPTURE_FRAMES, Env:VICTORY_DUMP_TARGETS -ErrorAction SilentlyContinue
        $env:VICTORY_PRESENT_PROBE = '0'
    }
}
$runner = if ($IsolatedBuild) {
    Join-Path $env:LOCALAPPDATA 'Temp\victory-handoff-target\diag\victory.exe'
} else {
    Join-Path $repo 'target\diag\victory.exe'
}
if (-not [IO.File]::Exists($runner)) { throw "Diagnostics build missing: $runner" }
$exe = Join-Path $env:VICTORY_GAME_DIR 'game_scarlett_master.exe'
if (-not [IO.File]::Exists($exe)) { throw "Game executable missing: $exe" }
if ($CheckPaths) {
    Write-Output "build       : $runner"
    if ($MeshMaskFallback) { Write-Output 'mesh mask   : opt-in RT2 replacement (not integer MAX)' }
    if ($SrgbViews) { Write-Output 'sRGB views  : opt-in RGBA8 decoding' }
    if ($CaptureFrames) { Write-Output "frames      : $CaptureFrames; dump targets: $($DumpTargets.IsPresent)" }
    if ($PackedLightAfter) { Write-Output "packed light: world frame + $PackedLightAfter" }
    if ($PresentTokenTrace) { Write-Output 'token trace : first 768 presents (CPU-only)' }
    if ($IsolatedBuild -or $NoCapture) {
        Write-Output "capture     : $(if ($env:VICTORY_CAPTURE) { $env:VICTORY_CAPTURE } else { '(off)' })"
        Write-Output "RGB probe   : $(if ($env:VICTORY_PRESENT_PROBE -eq '0') { 'off' } else { 'on' })"
    }
    if ($CinematicFast) {
        $captureEnabled = [bool]$env:VICTORY_CAPTURE
        Write-Output "cinematic   : late wait $env:VICTORY_LATE_WAIT, relay $env:VICTORY_RELAY, capture $captureEnabled"
    }
    & $runner config
    exit $LASTEXITCODE
}
if (Get-Process victory -ErrorAction SilentlyContinue) {
    throw 'Victory is already running. Use its existing window; a second instance could contend for saves and caches.'
}
$null = [IO.Directory]::CreateDirectory($logs)
if ($env:VICTORY_CAPTURE) { $null = [IO.Directory]::CreateDirectory($captures) }
$null = [IO.Directory]::CreateDirectory($data)
$prefix = if ($CinematicFast) { 'startup-fast-' } else { 'startup-' }
$log = Join-Path $logs ($prefix + (Get-Date -Format 'yyyyMMdd-HHmmss') + '.log')
if ($FirstBoot) {
    Write-Host "First-boot profile: $data"
    Write-Host 'Your normal profile is unchanged. Use Launch-Victory-Fork.cmd to go back to it.'
}
if ($CinematicFast) { Write-Host 'Cinematic performance mode: GPU relay 2. Use the normal launcher to revert.' }
if ($IsolatedBuild) { Write-Host "Isolated build: $runner" }
if ($MeshMaskFallback) { Write-Warning 'Windshield mask fallback: RT2 replacement for this pass only; scene lighting is unchanged.' }
if ($SrgbViews) { Write-Host 'Sampled RGBA8 sRGB views enabled.' }
if ($PackedLightAfter) { Write-Host "Packed-light capture offsets after the first 500-draw frame: $PackedLightAfter" }
if ($IsolatedBuild -and $env:VICTORY_CAPTURE) { Write-Host "Capture folder: $captures" }
Write-Host "Launch log: $log"
Push-Location $repo
try {
    # Redirect at the native shell so a terminated PowerShell pipeline cannot
    # leave a zero-byte log despite Victory having emitted diagnostic lines.
    # Victory logs normal INFO to stderr, so merge it into the same file.
    if (@($runner, $exe, $log) -match '%') {
        throw 'Use paths without percent signs; cmd.exe would expand them in this launcher.'
    }
    $command = '""{0}" run "{1}" --execute --console series-x > "{2}" 2>&1"' -f $runner, $exe, $log
    & $env:ComSpec /d /v:off /s /c $command
    $code = $LASTEXITCODE
    Write-Host "Victory exited with $code. Persistent saves remain under $env:VICTORY_DATA_DIR"
    exit $code
} finally { Pop-Location }
