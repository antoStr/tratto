; Windows uninstaller: removes everything Tratto left on this PC. The boards are the user's work,
; so it asks first (No by default, and in silent mode). Updates run the uninstaller too, with
; --updated: then nothing here runs.
!macro customUnInstall
  ${ifNot} ${isUpdated}
    SetShellVarContext current
    MessageBox MB_YESNO|MB_ICONQUESTION "Vuoi eliminare anche le lavagne salvate su questo PC?$\r$\n$\r$\nScegli No per tenerle: le ritroverai se reinstalli Tratto." /SD IDNO IDNO trattoKeepBoards
      RMDir /r "$APPDATA\${APP_FILENAME}"
      !ifdef APP_PRODUCT_FILENAME
        RMDir /r "$APPDATA\${APP_PRODUCT_FILENAME}"
      !endif
      Goto trattoCachesDone
    trattoKeepBoards:
      ; Keep tratto.db (the boards) and the preferences, drop the browser caches.
      RMDir /r "$APPDATA\${APP_FILENAME}\Cache"
      RMDir /r "$APPDATA\${APP_FILENAME}\Code Cache"
      RMDir /r "$APPDATA\${APP_FILENAME}\GPUCache"
      RMDir /r "$APPDATA\${APP_FILENAME}\DawnGraphiteCache"
      RMDir /r "$APPDATA\${APP_FILENAME}\DawnWebGPUCache"
      RMDir /r "$APPDATA\${APP_FILENAME}\Crashpad"
    trattoCachesDone:
    ; Downloaded updates (electron-updater).
    RMDir /r "$LOCALAPPDATA\tratto-updater"
  ${endIf}
!macroend
