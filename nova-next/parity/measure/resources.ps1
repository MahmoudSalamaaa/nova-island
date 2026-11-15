# Measures what Nova Island costs, the same way for 1.7 (Electron) and 1.8 (Rust):
#   RAM  private working set of "Nova Island.exe" plus every process it started (Electron's GPU and
#        renderer processes and nova-helper.exe, or 1.8's WebView2 processes), summed
#   CPU  all of those together, as a share of the whole PC (all cores = 100%)
# Run it while Nova Island is running and set up for the scenario you want to measure:
#   powershell -ExecutionPolicy Bypass -File resources.ps1 -Scenario idle-glass -Seconds 600
# Results go to parity/measure/results/<scenario>-<time>.json, plus a one-line summary.
param(
  [string]$Scenario = 'idle-glass',
  [int]$Seconds = 600,
  [int]$Every = 5,
  [string]$Name = 'Nova Island'
)
$ErrorActionPreference = 'Stop'

function Get-Tree {
  $all = Get-CimInstance Win32_Process -Property ProcessId, ParentProcessId, Name
  $roots = $all | Where-Object { $_.Name -eq "$Name.exe" }
  if (-not $roots) { throw "$Name.exe isn't running" }
  # the root is the copy whose parent isn't another copy of the app
  $ids = [System.Collections.Generic.HashSet[int]]::new()
  $queue = [System.Collections.Generic.Queue[int]]::new()
  foreach ($r in $roots) { if ($ids.Add([int]$r.ProcessId)) { $queue.Enqueue([int]$r.ProcessId) } }
  while ($queue.Count) {
    $p = $queue.Dequeue()
    foreach ($c in $all | Where-Object { $_.ParentProcessId -eq $p }) {
      if ($ids.Add([int]$c.ProcessId)) { $queue.Enqueue([int]$c.ProcessId) }
    }
  }
  return $ids
}

$cores = [Environment]::ProcessorCount
$samples = @()
$end = (Get-Date).AddSeconds($Seconds)
Write-Host "Measuring '$Scenario' for $Seconds s ..."
while ((Get-Date) -lt $end) {
  $ids = Get-Tree
  $perf = Get-CimInstance Win32_PerfFormattedData_PerfProc_Process -Property IDProcess, WorkingSetPrivate, PercentProcessorTime, PrivateBytes |
    Where-Object { $ids.Contains([int]$_.IDProcess) }
  $samples += [pscustomobject]@{
    t         = (Get-Date).ToString('o')
    processes = $ids.Count
    ramMB     = [math]::Round((($perf | Measure-Object WorkingSetPrivate -Sum).Sum) / 1MB, 1)
    commitMB  = [math]::Round((($perf | Measure-Object PrivateBytes -Sum).Sum) / 1MB, 1)
    # PercentProcessorTime is per core; divide by the core count for a share of the whole PC
    cpu       = [math]::Round((($perf | Measure-Object PercentProcessorTime -Sum).Sum) / $cores, 2)
  }
  Start-Sleep -Seconds $Every
}

function Median($xs) { $s = @($xs | Sort-Object); if (-not $s.Count) { return 0 }; $s[[int][math]::Floor(($s.Count - 1) / 2)] }
function P95($xs) { $s = @($xs | Sort-Object); if (-not $s.Count) { return 0 }; $s[[int][math]::Ceiling(0.95 * $s.Count) - 1] }

$exe = (Get-Process -Name $Name -ErrorAction SilentlyContinue | Select-Object -First 1).Path
$summary = [pscustomobject]@{
  scenario     = $Scenario
  seconds      = $Seconds
  version      = if ($exe) { (Get-Item $exe).VersionInfo.ProductVersion } else { '' }
  processes    = Median ($samples.processes)
  ramMedianMB  = Median ($samples.ramMB)
  ramP95MB     = P95 ($samples.ramMB)
  cpuMeanPct   = [math]::Round((($samples | Measure-Object cpu -Average).Average), 2)
  installedMB  = if ($exe) { [math]::Round(((Get-ChildItem (Split-Path $exe) -Recurse -File | Measure-Object Length -Sum).Sum) / 1MB, 1) } else { 0 }
}

$dir = Join-Path $PSScriptRoot 'results'
New-Item -ItemType Directory -Force $dir | Out-Null
$file = Join-Path $dir ("{0}-{1}.json" -f $Scenario, (Get-Date -Format 'yyyyMMdd-HHmmss'))
[pscustomobject]@{ summary = $summary; samples = $samples } | ConvertTo-Json -Depth 4 | Out-File -Encoding utf8 $file
$summary | Format-List
Write-Host "Saved $file"
