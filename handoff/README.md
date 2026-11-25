# Victory development handoff

This checkout preserves the history and license of
[`fxceless/victory-fork`](https://github.com/fxceless/victory-fork). The handoff
starts from `a803c87c48078e57b296e06e08146d7ceca44530`.
Read [PROGRESS.md](PROGRESS.md) before interpreting a successful build as a
rendering fix. See [the asset policy](../docs/asset-policy.md) before adding
test fixtures or evidence.

## Build and launch

Use Windows x64, Rustup (the toolchain is pinned), and the MSVC C++ build
tools. The Windows SDK's DXC and DXIL validator are used for the optional
hardware shader tests. The Direct3D debug layer needs Windows Graphics
Tools.

From the root of the checkout, in PowerShell:

```powershell
cargo build --locked --profile diag -p victory --features diagnostics
$env:VICTORY_GAME_DIR = 'D:\YourOwnTitleFiles'
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\handoff\Run-Victory.ps1 -CheckPaths
.\handoff\Launch-Victory-Fork.cmd
```

Keep your own title files outside the checkout. You can also pass `-GameDir`
to the PowerShell launcher. It keeps its logs, captures, cache, and profiles
under `%LOCALAPPDATA%\victory-handoff`. It doesn't launch a second Victory
instance. `-CheckPaths` only checks configuration.

## Isolated diagnostics

Build a second executable without replacing the normal build:

```powershell
$previousTarget = $env:CARGO_TARGET_DIR
try {
    $env:CARGO_TARGET_DIR = "$env:LOCALAPPDATA\Temp\victory-handoff-target"
    cargo build --locked --profile diag -p victory --features diagnostics
} finally { $env:CARGO_TARGET_DIR = $previousTarget }
```

Use `.\handoff\Run-Victory.ps1 -IsolatedBuild` with **one** of these:

| Options | What the run tests |
| --- | --- |
| `-PresentTokenTrace` | Intro flicker. Correlate guest present tags and source handles with the RGB grid log by frame. |
| `-PackedLightAfter '2,6,12'` | Dark scene. Capture packed-light writer/alias pairs after the first frame that records at least 500 draws. |
| `-SrgbViews` | Color comparison against a default isolated run on the same shot. |
| `-NoCapture` or `-CinematicFast` | Relay-off versus relay-on frame time. Do not enable GPU debugging or target dumps in these runs. |

`Measure-Victory-Cinematic.ps1 -Log <baseline>,<relay>` compares the frame-time
windows. Use a warmed cache, the same profile and scene, and compare the
number of dropped passes as well as the timing.

Original-container tests take local fixture paths via environment
variables and are ignored unless explicitly requested. Do not add title
files, original containers, dumps, raw logs, captures, or saves to this repo.
