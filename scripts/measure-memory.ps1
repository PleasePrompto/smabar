# Sample an existing smabar process tree as JSONL with Windows PowerShell 5.1.
# Values are bytes: WorkingSet includes shared resident pages (its sum can double-count),
# WorkingSetPrivate is private resident RAM, PrivateBytes is private committed memory.
# Example: .\scripts\measure-memory.ps1 -RootPid 1234 -IntervalSeconds 60 -Samples 61
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)] [ValidateRange(1, 2147483647)] [int]$RootPid,
    [ValidateRange(1, 86400)] [int]$IntervalSeconds = 60,
    [ValidateRange(1, 1000000)] [int]$Samples = 1
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Get-MemoryProcessTree {
    param([object[]]$Processes, [int]$MainPid)
    $byId = @{}
    foreach ($process in $Processes) { $byId[[int]$process.ProcessId] = $process }
    if (-not $byId.ContainsKey($MainPid)) { throw "Process $MainPid no longer exists." }
    $selected = @{ $MainPid = $byId[$MainPid] }
    do {
        $previousCount = $selected.Count
        foreach ($process in $Processes) {
            $parent = $selected[[int]$process.ParentProcessId]
            # Windows reuses PIDs; an older process cannot descend from a newer parent.
            if ($null -ne $parent -and $process.CreationDate -ge $parent.CreationDate) {
                $selected[[int]$process.ProcessId] = $process
            }
        }
    } while ($selected.Count -gt $previousCount)
    $selected.Values | Sort-Object ProcessId
}

function Get-MemoryProcessGroup {
    param([object]$Process, [int]$MainPid)
    if ($Process.ProcessId -eq $MainPid) { return 'app' }
    switch -Regex ($Process.Name) {
        '^msedgewebview2\.exe$' {
            if ([string]::IsNullOrWhiteSpace($Process.CommandLine)) { return 'webview-unknown' }
            if ($Process.CommandLine -match '(?:^|\s)"?--type(?:=|\s+)"?([^\s"]+)') {
                switch ($Matches[1]) {
                    'renderer' { return 'webview-renderer' }
                    'gpu-process' { return 'webview-gpu' }
                    'utility' { return 'webview-utility' }
                    default { return 'webview-other' }
                }
            }
            return 'webview-browser'
        }
        '^python(?:w|\d+(?:\.\d+)?)?\.exe$' { return 'python' }
        '^uv\.exe$' { return 'uv' }
        default { return 'other' }
    }
}

function ConvertTo-MemorySample {
    param(
        [object[]]$Before, [object[]]$Counters, [object[]]$After,
        [int]$MainPid, [string]$ExpectedStart = ''
    )
    $tree = @(Get-MemoryProcessTree $Before $MainPid)
    $root = $tree | Where-Object ProcessId -EQ $MainPid
    if ($root.Name -ne 'smabar.exe') { throw "Process $MainPid is not smabar.exe." }
    $started = $root.CreationDate.ToUniversalTime().ToString('o')
    if ($ExpectedStart -and $started -ne $ExpectedStart) {
        throw 'Main PID was reused; start a new measurement.'
    }
    $current = @{}
    foreach ($process in $After) { $current[[int]$process.ProcessId] = $process }
    if (-not $current.ContainsKey($MainPid)) { throw 'Main process exited during measurement.' }
    if ($current[$MainPid].CreationDate -ne $root.CreationDate) {
        throw 'Main PID was reused during measurement; start a new measurement.'
    }
    $metrics = @{}
    foreach ($counter in $Counters) {
        $processId = [int]$counter.IDProcess
        if ($metrics.ContainsKey($processId)) { throw "Duplicate performance counters for PID $processId." }
        $metrics[$processId] = $counter
    }
    $fields = @('WorkingSet', 'WorkingSetPrivate', 'PrivateBytes')
    $totals = @{ WorkingSet = 0L; WorkingSetPrivate = 0L; PrivateBytes = 0L }
    $groups = @{}
    $rows = @()
    $errors = @()
    foreach ($process in $tree) {
        $processId = [int]$process.ProcessId
        $reason = $null
        if (-not $current.ContainsKey($processId)) { $reason = 'process_exited' }
        elseif ($current[$processId].CreationDate -ne $process.CreationDate) { $reason = 'pid_reused' }
        elseif (-not $metrics.ContainsKey($processId)) { $reason = 'counters_missing' }
        if ($null -ne $reason) {
            if ($processId -eq $MainPid) { throw "Cannot measure main PID ${MainPid}: $reason." }
            $errors += @{ pid = $processId; reason = $reason }
            continue
        }
        $group = Get-MemoryProcessGroup $process $MainPid
        $row = @{
            pid = $processId; ppid = [int]$process.ParentProcessId
            name = $process.Name; group = $group
            started = $process.CreationDate.ToUniversalTime().ToString('o')
        }
        if (-not $groups.ContainsKey($group)) {
            $groups[$group] = @{ WorkingSet = 0L; WorkingSetPrivate = 0L; PrivateBytes = 0L }
        }
        foreach ($field in $fields) {
            $property = $metrics[$processId].PSObject.Properties[$field]
            if ($null -eq $property -or $null -eq $property.Value -or [long]$property.Value -lt 0) {
                throw "Missing or invalid $field for PID $processId; check Windows performance counters."
            }
            $value = [long]$property.Value
            $row[$field] = $value
            $totals[$field] += $value
            $groups[$group][$field] += $value
        }
        $rows += $row
    }
    @{
        timestamp = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds() / 1000.0
        root_pid = $MainPid; root_started = $started; unit = 'bytes'
        processes = $rows; groups = $groups; total = $totals; errors = $errors
    }
}

# Dot-sourcing exposes the same sampler functions to the native regression check.
if ($MyInvocation.InvocationName -eq '.') { return }

$expectedStart = ''
try {
    for ($index = 0; $index -lt $Samples; $index++) {
        $watch = [Diagnostics.Stopwatch]::StartNew()
        $before = @(Get-CimInstance Win32_Process -Property ProcessId, ParentProcessId, Name, CreationDate, CommandLine)
        $tree = @(Get-MemoryProcessTree $before $RootPid)
        $filter = ($tree | ForEach-Object { "IDProcess = $($_.ProcessId)" }) -join ' OR '
        $counters = @(Get-CimInstance Win32_PerfRawData_PerfProc_Process -Filter $filter -Property IDProcess, WorkingSet, WorkingSetPrivate, PrivateBytes)
        $filter = ($tree | ForEach-Object { "ProcessId = $($_.ProcessId)" }) -join ' OR '
        $after = @(Get-CimInstance Win32_Process -Filter $filter -Property ProcessId, CreationDate)
        $result = ConvertTo-MemorySample $before $counters $after $RootPid $expectedStart
        $expectedStart = $result.root_started
        $result['sample_duration_ms'] = $watch.ElapsedMilliseconds
        $result | ConvertTo-Json -Depth 6 -Compress
        if ($index + 1 -lt $Samples) { Start-Sleep -Seconds $IntervalSeconds }
    }
} catch {
    Write-Error "Memory measurement failed: $($_.Exception.Message)" -ErrorAction Continue
    exit 1
}
