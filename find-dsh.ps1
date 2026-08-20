param([string]$Pids = '')

# WMI details for dsh processes. Output JSON array:
# [{ "ProcessId": 123, "Name": "node.exe", "CommandLine": "...", "CreationDate": "...",
#    "WorkingSet": 123456, "KernelTime": 123, "UserTime": 456 }], or [].
$ids = @()
if ($Pids) {
  foreach ($part in ($Pids -split ',')) {
    $n = ($part.Trim() -as [int])
    if ($n -and $n -gt 0) { $ids += [int]$n }
  }
}

# Also find dsh node.exe processes by command line (covers non-web profiles).
$dsh = @(Get-CimInstance Win32_Process -Filter "Name = 'node.exe'" -ErrorAction SilentlyContinue |
  Where-Object { $_.CommandLine -match 'dsh' -and $_.CommandLine -match 'bin\.js' })
foreach ($w in $dsh) {
  if ($w.ProcessId -gt 0 -and ($ids -notcontains [int]$w.ProcessId)) { $ids += [int]$w.ProcessId }
}

$result = @()
foreach ($id in $ids) {
  $p = Get-CimInstance Win32_Process -Filter "ProcessId = $id" -ErrorAction SilentlyContinue
  if ($p) {
    $result += [pscustomobject]@{
      ProcessId    = $id
      Name         = [string]$p.Name
      CommandLine  = [string]$p.CommandLine
      CreationDate = [string]$p.CreationDate
      WorkingSet   = [int64]$p.WorkingSetSize
      KernelTime   = [int64]$p.KernelModeTime
      UserTime     = [int64]$p.UserModeTime
    }
  }
}

if ($result.Count -eq 0) {
  Write-Output '[]'
} else {
  $result | ConvertTo-Json -Compress
}
