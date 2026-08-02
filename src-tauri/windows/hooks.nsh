; CrawlFlow NSIS Hooks
; Tự động cài và start Windows Service

!define SERVICE_NAME "CrawlFlowService"

; Post-install hook - runs after files are copied
!macro NSIS_HOOK_POSTINSTALL
  ; Build service binary path
  StrCpy $0 "$INSTDIR\crawlflow-service.exe"
  
  ; Build data directory path
  StrCpy $1 "$LOCALAPPDATA\com.CrawlFlow.desktop"
  
  ; Install Windows Service using sc command
  DetailPrint "Installing Windows Service..."
  nsExec::ExecToLog 'sc create ${SERVICE_NAME} binPath= "$0 --service --all --data-dir $1" start= auto DisplayName= "CrawlFlow Background Service"'
  Pop $2
  
  ${If} $2 == 0
    DetailPrint "Service installed successfully"
    
    ; Start the service
    DetailPrint "Starting Windows Service..."
    nsExec::ExecToLog 'sc start ${SERVICE_NAME}'
    Pop $2
    
    ${If} $2 == 0
      DetailPrint "Service started successfully"
    ${Else}
      DetailPrint "Warning: Failed to start service (error code $2)"
    ${EndIf}
  ${Else}
    DetailPrint "Warning: Failed to install service (error code $2)"
  ${EndIf}
!macroend

; Pre-uninstall hook - runs before files are removed
!macro NSIS_HOOK_PREUNINSTALL
  ; Stop and remove Windows Service
  DetailPrint "Stopping Windows Service..."
  nsExec::ExecToLog 'sc stop ${SERVICE_NAME}'
  Pop $0
  
  DetailPrint "Removing Windows Service..."
  nsExec::ExecToLog 'sc delete ${SERVICE_NAME}'
  Pop $0
!macroend