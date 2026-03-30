!macro NSIS_HOOK_POSTINSTALL
  ; Register URL protocol handler: floatinglyrics://
  ; Use HKCU so it works for per-user installs without admin.
  WriteRegStr HKCU "Software\Classes\floatinglyrics" "" "URL:Floating Lyrics Protocol"
  WriteRegStr HKCU "Software\Classes\floatinglyrics" "URL Protocol" ""
  WriteRegStr HKCU "Software\Classes\floatinglyrics\DefaultIcon" "" "$INSTDIR\${MAINBINARYNAME}.exe,1"
  WriteRegStr HKCU "Software\Classes\floatinglyrics\shell\open\command" "" '"$INSTDIR\${MAINBINARYNAME}.exe" "%1"'

  ; Optional autostart prompt.
  MessageBox MB_ICONQUESTION|MB_YESNO "Run ${PRODUCTNAME} on Windows startup?" IDNO no_autostart
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "${PRODUCTNAME}" '"$INSTDIR\${MAINBINARYNAME}.exe" --windows-startup'
  no_autostart:
!macroend
