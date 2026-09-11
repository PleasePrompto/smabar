; Warm up the managed Python runtime right after install so the first plugin
; start needs no download. Best-effort and async by design: Exec returns
; immediately (the installer never waits), an offline failure only lands in
; ~/.smabar/logs/, and smabar still provisions lazily on first use.
; Exit codes of --provision: 0 ready/already present, 2 uv missing, 1 failure.
!macro NSIS_HOOK_POSTINSTALL
  Exec '"$INSTDIR\smabar.exe" --provision'
!macroend
