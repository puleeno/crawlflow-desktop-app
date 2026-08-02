; CrawlFlow NSIS Hooks
; Tự động cài và start Windows Service

!define SERVICE_NAME "CrawlFlowService"

; Post-install hook - runs after files are copied
!macro NSIS_HOOK_POSTINSTALL
  ; Build service binary path
  StrCpy $0 "$INSTDIR\crawlflow-service.exe"
  
  ; Build data directory path
  StrCpy $1 "$LOCALAPPDATA\com.CrawlFlow.desktop"
  
  ; Create data directory if it doesn't exist
  CreateDirectory "$1"
  
  ; Install Windows Service using sc command
  DetailPrint "Installing Windows Service..."
  DetailPrint "Service binary: $0"
  DetailPrint "Data directory: $1"
  DetailPrint "Install directory: $INSTDIR"
  
  ; Check if service binary exists
  IfFileExists "$0" 0 +3
  DetailPrint "Service binary found"
  Goto +2
  DetailPrint "ERROR: Service binary not found at $0"
  
  ; Build command with proper quoting
  StrCpy $2 '"$0" --service --all --data-dir "$1"'
  DetailPrint "Service command: $2"
  
  nsExec::ExecToLog 'sc create ${SERVICE_NAME} binPath= $2 start= auto DisplayName= "CrawlFlow Background Service"'
  Pop $3
  
  ${If} $3 == 0
    DetailPrint "Service installed successfully"
    
    ; Wait for service to be fully registered
    Sleep 2000
    
    ; Start the service
    DetailPrint "Starting Windows Service..."
    nsExec::ExecToLog 'sc start ${SERVICE_NAME}'
    Pop $3
    
    ${If} $3 == 0
      DetailPrint "Service started successfully"
      
      ; Verify service is running
      Sleep 3000
      DetailPrint "Checking service status..."
      nsExec::ExecToLog 'sc query ${SERVICE_NAME}'
    ${Else}
      DetailPrint "Warning: Failed to start service (error code $3)"
    ${EndIf}
  ${Else}
    DetailPrint "Warning: Failed to install service (error code $3)"
    DetailPrint "Installer will continue, but service needs manual installation"
  ${EndIf}
!macroend

; Pre-uninstall hook - runs before files are removed
!macro NSIS_HOOK_PREUNINSTALL
  ; Stop and remove Windows Service
  DetailPrint "Stopping Windows Service..."
  nsExec::ExecToLog 'sc stop ${SERVICE_NAME}'
  Pop $0
  
  ; Wait for service to stop
  Sleep 2000
  
  DetailPrint "Removing Windows Service..."
  nsExec::ExecToLog 'sc delete ${SERVICE_NAME}'
  Pop $0
!macroend