# Windows PowerShell 5.1 | Compare Victory world-rendering log windows.
[CmdletBinding()]
param(
    [Parameter(Mandatory, Position = 0)]
    [string[]]$Log,
    [ValidateRange(1, 1000000)]
    [int]$MinDrawsPerFrame = 1000
)
$ErrorActionPreference = 'Stop'
$culture = [Globalization.CultureInfo]::InvariantCulture
$frameLine = '(?<ms>\d+(?:\.\d+)?) ms/frame \((?<prep>\d+(?:\.\d+)?) preparing:.*?;\s*(?<gpu>\d+(?:\.\d+)?) on the GPU;\s*(?<back>\d+(?:\.\d+)?) copying back.*?\),\s*(?<draws>\d+) draws/frame, dropped (?<drops>\d+) without a host pipeline \((?<pending>\d+) being made\)'

function Median([double[]]$Values) {
    $sorted = @($Values | Sort-Object)
    $middle = [int][Math]::Floor($sorted.Count / 2)
    if ($sorted.Count % 2 -eq 1) { return $sorted[$middle] }
    ($sorted[$middle - 1] + $sorted[$middle]) / 2
}

foreach ($path in $Log) {
    $file = Get-Item -LiteralPath $path
    $previous = $null
    $relay = $false
    $world = [Collections.Generic.List[object]]::new()
    foreach ($raw in [IO.File]::ReadLines($file.FullName)) {
        $line = $raw -replace '\x1b\[[0-9;]*m', ''
        if ($line -match 'what shaders wrote is copied back by a thread of its own') {
            $relay = $true
        }
        if ($line -match "the card: (?<used>\d+) MB in use of a budget of (?<budget>\d+) MB") {
            # The memory report follows the timing line in a frame window.
            $memory = [pscustomobject]@{ Used = [int]$Matches.used; Budget = [int]$Matches.budget }
            if ($null -ne $previous) { $previous.Memory = $memory; $previous = $null }
        }
        if ($line -match $frameLine) {
            $previous = $null
            if ([int]$Matches.draws -lt $MinDrawsPerFrame) { continue }
            $previous = [pscustomobject]@{
                FrameMs = [double]::Parse($Matches.ms, $culture)
                PrepareMs = [double]::Parse($Matches.prep, $culture)
                GpuMs = [double]::Parse($Matches.gpu, $culture)
                CopyBackMs = [double]::Parse($Matches.back, $culture)
                Pending = [int]$Matches.pending
                Memory = $null
            }
            $world.Add($previous)
        }
    }
    $frameMs = $null
    $fps = $null
    $prepMs = $null
    $gpuMs = $null
    $copyMs = $null
    $overBudget = 0
    $memoryRows = 0
    if ($world.Count) {
        $frameMs = Median $world.FrameMs
        if ($frameMs -gt 0) { $fps = [Math]::Round(1000 / $frameMs, 2) }
        $prepMs = Median $world.PrepareMs
        $gpuMs = Median $world.GpuMs
        $copyMs = Median $world.CopyBackMs
        $memoryRows = @($world | Where-Object { $null -ne $_.Memory -and $_.Memory.Budget -gt 0 }).Count
        $overBudget = @($world | Where-Object { $null -ne $_.Memory -and $_.Memory.Budget -gt 0 -and $_.Memory.Used -gt $_.Memory.Budget }).Count
    }
    [pscustomobject]@{
        Log = $file.FullName
        RelaySeen = $relay
        MinDrawsPerFrame = $MinDrawsPerFrame
        WorldWindows = $world.Count
        NoPendingPipelineWindows = @($world | Where-Object Pending -eq 0).Count
        MedianWindowFrameMs = $frameMs
        FpsEquivalent = $fps
        MedianWindowPrepareMs = $prepMs
        MedianWindowGpuMs = $gpuMs
        MedianWindowCopyBackMs = $copyMs
        LocalVramOverBudgetWindows = $overBudget
        LocalVramReportedWindows = $memoryRows
    }
}
