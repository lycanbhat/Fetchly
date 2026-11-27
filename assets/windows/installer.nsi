; Fetchly Windows installer (NSIS). Built by scripts/build-windows.sh.
; Per-user install: no administrator rights required.
Unicode true
!include "MUI2.nsh"

!ifndef VERSION
  !define VERSION "0.1.0"
!endif
!ifndef EXE
  !define EXE "fetchly.exe"
!endif
!ifndef LOADER
  !define LOADER "WebView2Loader.dll"
!endif
!ifndef OUTFILE
  !define OUTFILE "Fetchly-Setup.exe"
!endif

Name "Fetchly"
OutFile "${OUTFILE}"
InstallDir "$LOCALAPPDATA\Programs\Fetchly"
InstallDirRegKey HKCU "Software\Fetchly" "InstallDir"
RequestExecutionLevel user
SetCompressor /SOLID lzma
BrandingText "Fetchly ${VERSION}"

VIProductVersion "${VERSION}.0"
VIAddVersionKey "ProductName" "Fetchly"
VIAddVersionKey "FileDescription" "Fetchly Installer"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "ProductVersion" "${VERSION}"
VIAddVersionKey "CompanyName" "Fetchly"
VIAddVersionKey "LegalCopyright" "MIT OR Apache-2.0"

!define MUI_ICON "fetchly.ico"
!define MUI_UNICON "fetchly.ico"
!define MUI_ABORTWARNING
!define MUI_FINISHPAGE_RUN "$INSTDIR\Fetchly.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Launch Fetchly"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

; Fetchly renders its UI with Microsoft Edge WebView2 (built into Windows 11 and most Windows 10 PCs).
!define WV2_GUID "{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"
Function CheckWebView2
  ReadRegStr $0 HKLM "SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\${WV2_GUID}" "pv"
  StrCmp $0 "" 0 found
  ReadRegStr $0 HKLM "SOFTWARE\Microsoft\EdgeUpdate\Clients\${WV2_GUID}" "pv"
  StrCmp $0 "" 0 found
  ReadRegStr $0 HKCU "Software\Microsoft\EdgeUpdate\Clients\${WV2_GUID}" "pv"
  StrCmp $0 "" 0 found
  MessageBox MB_YESNO|MB_ICONINFORMATION "Fetchly needs the Microsoft Edge WebView2 Runtime, which isn't installed on this PC.$\r$\n$\r$\nOpen the download page now? (Install it, then start Fetchly.)" IDNO found
  ExecShell "open" "https://developer.microsoft.com/microsoft-edge/webview2/#download"
  found:
FunctionEnd

!define UNINST_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\Fetchly"

Section "Fetchly (required)" SecMain
  SectionIn RO
  ; Close a running copy so the exe can be replaced on upgrade.
  nsExec::Exec 'taskkill /IM Fetchly.exe /F'
  SetOutPath "$INSTDIR"
  File "/oname=Fetchly.exe" "${EXE}"
  File "/oname=WebView2Loader.dll" "${LOADER}"
  File "fetchly.ico"
  WriteUninstaller "$INSTDIR\Uninstall.exe"

  CreateDirectory "$SMPROGRAMS"
  CreateShortcut "$SMPROGRAMS\Fetchly.lnk" "$INSTDIR\Fetchly.exe" "" "$INSTDIR\Fetchly.exe" 0

  WriteRegStr HKCU "Software\Fetchly" "InstallDir" "$INSTDIR"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayName" "Fetchly"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNINST_KEY}" "Publisher" "Fetchly"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayIcon" "$INSTDIR\Fetchly.exe"
  WriteRegStr HKCU "${UNINST_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINST_KEY}" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegStr HKCU "${UNINST_KEY}" "QuietUninstallString" '"$INSTDIR\Uninstall.exe" /S'
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoRepair" 1
SectionEnd

Section "-WebView2 check"
  Call CheckWebView2
SectionEnd

Section "Desktop shortcut" SecDesktop
  CreateShortcut "$DESKTOP\Fetchly.lnk" "$INSTDIR\Fetchly.exe" "" "$INSTDIR\Fetchly.exe" 0
SectionEnd

Section "Uninstall"
  nsExec::Exec 'taskkill /IM Fetchly.exe /F'
  Delete "$INSTDIR\Fetchly.exe"
  Delete "$INSTDIR\WebView2Loader.dll"
  Delete "$INSTDIR\fetchly.ico"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"
  Delete "$SMPROGRAMS\Fetchly.lnk"
  Delete "$DESKTOP\Fetchly.lnk"
  DeleteRegKey HKCU "${UNINST_KEY}"
  DeleteRegKey HKCU "Software\Fetchly"
  ; User data (collections, environments) in %APPDATA% is intentionally kept.
SectionEnd
