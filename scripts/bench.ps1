# Measures Duri's CPU and memory use: duri.exe plus every WebView2 process it
# started. Memory is the private working set (Task Manager's "Memory" column).
#
#   powershell -ExecutionPolicy Bypass -File scripts\bench.ps1 [-Seconds 60] [-Label idle]
#
# Leave the island alone while it runs, unless you are measuring a busy state.
param([int]$Seconds = 60, [string]$Label = "idle")

$root = Get-Process duri -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $root) { Write-Error "Duri is not running."; exit 1 }

function Get-Tree([int]$rootId) {
  $all = Get-CimInstance Win32_Process
  $ids = [System.Collections.Generic.List[int]]::new(); $ids.Add($rootId)
  for ($i = 0; $i -lt $ids.Count; $i++) {
    foreach ($p in $all) { if ($p.ParentProcessId -eq $ids[$i] -and -not $ids.Contains([int]$p.ProcessId)) { $ids.Add([int]$p.ProcessId) } }
  }
  return $ids
}

function Get-CpuSeconds($ids) {
  $total = 0.0
  foreach ($id in $ids) { $p = Get-Process -Id $id -ErrorAction SilentlyContinue; if ($p) { $total += $p.TotalProcessorTime.TotalSeconds } }
  return $total
}

function Get-PrivateMB($ids) {
  $perf = Get-CimInstance Win32_PerfFormattedData_PerfProc_Process | Where-Object { $ids -contains [int]$_.IDProcess }
  return ($perf | Measure-Object WorkingSetPrivate -Sum).Sum / 1MB
}

$ids = Get-Tree $root.Id
$cores = [Environment]::ProcessorCount
$cpu0 = Get-CpuSeconds $ids; $t0 = Get-Date
$mem = @()
for ($s = 0; $s -lt $Seconds; $s += 5) { Start-Sleep 5; $mem += Get-PrivateMB $ids }
$cpu1 = Get-CpuSeconds $ids; $elapsed = ((Get-Date) - $t0).TotalSeconds

$busy = $cpu1 - $cpu0
$result = [ordered]@{
  label            = $Label
  version          = $root.MainModule.FileVersionInfo.ProductVersion
  processes        = $ids.Count
  seconds          = [math]::Round($elapsed, 1)
  cpuPercentOfPc   = [math]::Round(100 * $busy / ($elapsed * $cores), 3)
  cpuPercentOfCore = [math]::Round(100 * $busy / $elapsed, 2)
  memoryMB         = [math]::Round(($mem | Measure-Object -Average).Average, 1)
  memoryMaxMB      = [math]::Round(($mem | Measure-Object -Maximum).Maximum, 1)
  cores            = $cores
}
[pscustomobject]$result | ConvertTo-Json -Compress
