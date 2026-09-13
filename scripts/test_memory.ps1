# Deterministic native checks, no process creation or app/profile changes.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 'measure-memory.ps1') -RootPid 10

function Assert-Equal {
    param($Actual, $Expected, [string]$Reason)
    if ($Actual -cne $Expected) { throw "${Reason}: expected '$Expected', got '$Actual'." }
}
function Assert-Throws {
    param([scriptblock]$Operation, [string]$Message)
    try { & $Operation | Out-Null }
    catch {
        if ($_.Exception.Message -like $Message) { return }
        throw
    }
    throw "Expected error: $Message"
}
function New-ProcessRow {
    param([int]$Id, [int]$Parent, [string]$Name, [int]$Seconds, [string]$CommandLine = '')
    [pscustomobject]@{
        ProcessId = $Id; ParentProcessId = $Parent; Name = $Name
        CreationDate = ([datetime]'2026-01-01T00:00:00Z').AddSeconds($Seconds)
        CommandLine = $CommandLine
    }
}

$before = @(
    (New-ProcessRow 10 1 'smabar.exe' 10)
    (New-ProcessRow 20 10 'msedgewebview2.exe' 11 'webview.exe --embedded-browser-webview=1')
    (New-ProcessRow 30 20 'msedgewebview2.exe' 12 'webview.exe --type=renderer')
    (New-ProcessRow 40 20 'msedgewebview2.exe' 12 'webview.exe "--type=gpu-process"')
    (New-ProcessRow 50 20 'msedgewebview2.exe' 12 'webview.exe --type utility')
    (New-ProcessRow 60 10 'python.exe' 13)
    (New-ProcessRow 70 60 'python.exe' 14)
    (New-ProcessRow 80 1 'msedgewebview2.exe' 8 'webview.exe --type=renderer')
    (New-ProcessRow 90 10 'unrelated.exe' 9)
    (New-ProcessRow 100 90 'unrelated.exe' 10)
)
$counters = @($before | ForEach-Object {
    [pscustomobject]@{ IDProcess = $_.ProcessId; WorkingSet = 102400; WorkingSetPrivate = 20480; PrivateBytes = 61440 }
})
$result = ConvertTo-MemorySample $before $counters $before 10
Assert-Equal (($result.processes.pid | Sort-Object) -join ',') '10,20,30,40,50,60,70' 'Descendants and PID reuse'
Assert-Equal $result.total.WorkingSet 716800 'Shared working set stays distinct'
Assert-Equal $result.total.WorkingSetPrivate 143360 'Private resident total'
Assert-Equal $result.total.PrivateBytes 430080 'Private committed total'
Assert-Equal $result.groups.python.PrivateBytes 122880 'Include Python redirector and interpreter'
Assert-Equal ($result.processes.group -join ',') 'app,webview-browser,webview-renderer,webview-gpu,webview-utility,python,python' 'WebView2 classification'
Assert-Equal (($result | ConvertTo-Json -Depth 6) -match 'CommandLine|embedded-browser') $false 'No command-line payload in output'
Assert-Equal (Get-MemoryProcessGroup (New-ProcessRow 1 0 'msedgewebview2.exe' 1) 10) 'webview-unknown' 'Missing command line is not browser proof'
Assert-Equal (Get-MemoryProcessGroup (New-ProcessRow 1 0 'uv.exe' 1) 10) 'uv' 'UV classification'

$after = @($before | Where-Object ProcessId -NotIn 30, 40)
$after += New-ProcessRow 40 1 'other.exe' 50
$changed = ConvertTo-MemorySample $before ($counters | Where-Object IDProcess -NE 50) $after 10
Assert-Equal ($changed.errors.reason -join ',') 'process_exited,pid_reused,counters_missing' 'Races are explicit'
Assert-Equal $changed.total.PrivateBytes 245760 'Races excluded from totals'
Assert-Throws { ConvertTo-MemorySample $before $counters $after 10 'different-start' } '*Main PID was reused*'
Assert-Throws { ConvertTo-MemorySample $before $counters ($after | Where-Object ProcessId -NE 10) 10 } '*Main process exited*'
Assert-Throws { ConvertTo-MemorySample $before $counters $after 999 } '*no longer exists*'
$counters[0].PrivateBytes = $null
Assert-Throws { ConvertTo-MemorySample $before $counters $after 10 } '*Missing or invalid PrivateBytes*'
Assert-Throws { & (Join-Path $PSScriptRoot 'measure-memory.ps1') -RootPid 10 -Samples 0 } '*Samples*'
Assert-Throws { & (Join-Path $PSScriptRoot 'measure-memory.ps1') -RootPid 10 -IntervalSeconds 0 } '*IntervalSeconds*'
Write-Output 'Windows memory sampler checks passed.'
