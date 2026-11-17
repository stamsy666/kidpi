; kidpi installer hooks
;
; An upgrade may run the old uninstaller first. That must not touch kidpi's data
; (%ProgramData%\kidpi: zapret copy, settings, lists) or the logon task: the data
; goes only on a real uninstall where the user ticked "delete app data".

!macro KIDPI_STOP_ALL
  nsExec::Exec 'taskkill /F /IM kidpi.exe'
  nsExec::Exec 'taskkill /F /IM winws.exe'
  ; let go of WinDivert64.sys so its folder can be replaced or removed
  nsExec::Exec 'sc stop WinDivert'
  Sleep 800
!macroend

!macro NSIS_HOOK_PREINSTALL
  !insertmacro KIDPI_STOP_ALL
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro KIDPI_STOP_ALL
  ${If} $UpdateMode <> 1
    ; the "start with Windows" task
    nsExec::Exec 'schtasks /Delete /F /TN kidpi'
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $DeleteAppDataCheckboxState = 1
  ${AndIf} $UpdateMode <> 1
    SetShellVarContext all
    RMDir /r "$APPDATA\kidpi"
    SetShellVarContext current
  ${EndIf}
!macroend
