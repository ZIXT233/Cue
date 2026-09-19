; GNU/GNULLVM builds load WebView2Loader.dll dynamically. Tauri may omit
; this file from the NSIS payload. Resolve it beside the actual executable
; so custom target directories, profiles and architectures work as well.
; MSVC builds can link the loader statically and need no companion DLL.
!macro NSIS_HOOK_POSTINSTALL
  !searchreplace QUE_WEBVIEW2_LOADER "${MAINBINARYSRCPATH}" "${MAINBINARYNAME}.exe" "WebView2Loader.dll"
  !if /FileExists "${QUE_WEBVIEW2_LOADER}"
    SetOutPath "$INSTDIR"
    File /oname=WebView2Loader.dll "${QUE_WEBVIEW2_LOADER}"
  !endif
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  Delete "$INSTDIR\WebView2Loader.dll"
!macroend
