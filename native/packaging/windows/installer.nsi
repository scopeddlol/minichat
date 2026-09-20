; The Windows installer for the native client.
;
; One binary, so this is short: put it somewhere, make a shortcut, register
; an uninstaller, and get out of the way. It installs for one user into
; %LOCALAPPDATA%, which is what lets it run without an administrator — a
; member of a community should not need their IT department to join a chat.
;
; Built from CI, and locally with:
;
;   makensis -DVERSION=0.6.0 -DBINARY=../../target/release/minichat-native.exe \
;            -DOUTFILE=MiniChat-Setup.exe installer.nsi
;
; Silent install is supported, with the instance address as an option:
;
;   MiniChat-0.6.0-x86_64-setup.exe /S /INSTANCE=https://chat.example.com

Unicode true
ManifestDPIAware true
; Per-user: no elevation prompt, and no administrator needed.
RequestExecutionLevel user
SetCompressor /SOLID lzma

!include "MUI2.nsh"
!include "FileFunc.nsh"
!include "LogicLib.nsh"
!include "instance.nsh"

!ifndef VERSION
  !define VERSION "0.0.0"
!endif
!ifndef BINARY
  !define BINARY "..\..\target\release\minichat-native.exe"
!endif
!ifndef OUTFILE
  !define OUTFILE "MiniChat-Setup.exe"
!endif

!define NAME "MiniChat"
!define EXE "minichat-native.exe"
!define PUBLISHER "MiniChat"
!define HOMEPAGE "https://github.com/scopeddlol/minichat"
!define UNINSTKEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${NAME}"

Name "${NAME} ${VERSION}"
OutFile "${OUTFILE}"
InstallDir "$LOCALAPPDATA\${NAME}"
InstallDirRegKey HKCU "Software\${NAME}" "InstallLocation"
ShowInstDetails show
ShowUninstDetails show

VIProductVersion "${VERSION}.0"
VIAddVersionKey "ProductName" "${NAME}"
VIAddVersionKey "FileDescription" "${NAME} setup"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "ProductVersion" "${VERSION}"
VIAddVersionKey "CompanyName" "${PUBLISHER}"
VIAddVersionKey "LegalCopyright" "MIT"

; --- appearance ---------------------------------------------------------

!define MUI_ICON "..\..\assets\icons\icon.ico"
!define MUI_UNICON "..\..\assets\icons\icon.ico"
!define MUI_HEADERIMAGE
!define MUI_HEADERIMAGE_BITMAP "installer-header.bmp"
!define MUI_HEADERIMAGE_UNBITMAP "installer-header.bmp"
!define MUI_WELCOMEFINISHPAGE_BITMAP "installer-sidebar.bmp"
!define MUI_UNWELCOMEFINISHPAGE_BITMAP "installer-sidebar.bmp"
!define MUI_ABORTWARNING

!define MUI_WELCOMEPAGE_TITLE "Install ${NAME}"
!define MUI_WELCOMEPAGE_TEXT "The MiniChat desktop client, with no browser engine in it.$\r$\n$\r$\nIt installs for you alone and needs no administrator.$\r$\n$\r$\nClick Next to continue."

!define MUI_FINISHPAGE_RUN "$INSTDIR\${EXE}"
!define MUI_FINISHPAGE_RUN_TEXT "Run ${NAME} now"
; MUI's "show readme" slot, used for the shortcut nobody should get without
; asking for it.
!define MUI_FINISHPAGE_SHOWREADME ""
!define MUI_FINISHPAGE_SHOWREADME_NOTCHECKED
!define MUI_FINISHPAGE_SHOWREADME_TEXT "Put a shortcut on the desktop"
!define MUI_FINISHPAGE_SHOWREADME_FUNCTION DesktopShortcut

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "English"

; --- installing ---------------------------------------------------------

Function .onInit
  ; Everything below is per-user: shortcuts, registry, the settings the
  ; /INSTANCE option writes.
  SetShellVarContext current
FunctionEnd

Function DesktopShortcut
  CreateShortcut "$DESKTOP\${NAME}.lnk" "$INSTDIR\${EXE}"
FunctionEnd

; Replacing a running binary fails halfway through, so ask first and keep
; asking rather than leaving a half-installed directory behind.
Function CloseRunningApp
  ${If} ${FileExists} "$INSTDIR\${EXE}"
    retry:
      ClearErrors
      ; Renaming is how you find out it is in use without a plugin: Windows
      ; refuses to move a file another process has open for execution.
      Rename "$INSTDIR\${EXE}" "$INSTDIR\${EXE}.old"
      ${If} ${Errors}
        IfSilent +2
        MessageBox MB_RETRYCANCEL|MB_ICONEXCLAMATION \
          "${NAME} is running. Close it, then choose Retry." \
          /SD IDCANCEL IDRETRY retry
        Abort "${NAME} is running."
      ${Else}
        Delete "$INSTDIR\${EXE}.old"
      ${EndIf}
  ${EndIf}
FunctionEnd

Section "Install"
  Call CloseRunningApp

  SetOutPath "$INSTDIR"
  File "/oname=${EXE}" "${BINARY}"
  File "/oname=README.md" "..\..\README.md"
  SetOutPath "$INSTDIR\licences"
  File "..\..\assets\fonts\*-LICENSE.txt"
  SetOutPath "$INSTDIR"

  CreateShortcut "$SMPROGRAMS\${NAME}.lnk" "$INSTDIR\${EXE}"
  WriteUninstaller "$INSTDIR\uninstall.exe"

  WriteRegStr HKCU "Software\${NAME}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINSTKEY}" "DisplayName" "${NAME}"
  WriteRegStr HKCU "${UNINSTKEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNINSTKEY}" "DisplayIcon" "$\"$INSTDIR\${EXE}$\""
  WriteRegStr HKCU "${UNINSTKEY}" "Publisher" "${PUBLISHER}"
  WriteRegStr HKCU "${UNINSTKEY}" "URLInfoAbout" "${HOMEPAGE}"
  WriteRegStr HKCU "${UNINSTKEY}" "InstallLocation" "$\"$INSTDIR$\""
  WriteRegStr HKCU "${UNINSTKEY}" "UninstallString" "$\"$INSTDIR\uninstall.exe$\""
  WriteRegStr HKCU "${UNINSTKEY}" "QuietUninstallString" "$\"$INSTDIR\uninstall.exe$\" /S"
  WriteRegDWORD HKCU "${UNINSTKEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINSTKEY}" "NoRepair" 1
  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  IntFmt $0 "0x%08X" $0
  WriteRegDWORD HKCU "${UNINSTKEY}" "EstimatedSize" "$0"

  Call SeedInstanceAddress
SectionEnd

; --- uninstalling -------------------------------------------------------

Function un.onInit
  SetShellVarContext current
FunctionEnd

Section "Uninstall"
  Delete "$SMPROGRAMS\${NAME}.lnk"
  Delete "$DESKTOP\${NAME}.lnk"

  Delete "$INSTDIR\${EXE}"
  Delete "$INSTDIR\README.md"
  Delete "$INSTDIR\uninstall.exe"
  RMDir /r "$INSTDIR\licences"
  RMDir "$INSTDIR"

  DeleteRegKey HKCU "${UNINSTKEY}"
  DeleteRegKey HKCU "Software\${NAME}"

  ; Uninstalling the app is not the same as wanting your sign-in gone. An
  ; unattended uninstall keeps it, so reinstalling does not cost someone
  ; their session and their instance address.
  ${If} ${FileExists} "$APPDATA\mini\${NAME}\config\native.json"
    StrCpy $R0 "keep"
    MessageBox MB_YESNO|MB_ICONQUESTION \
      "Remove your saved instance address and sign-in as well?$\r$\n$\r$\nChoose No to keep them for the next time you install ${NAME}." \
      /SD IDNO IDNO +2
    StrCpy $R0 "remove"
    ${If} $R0 == "remove"
      RMDir /r "$APPDATA\mini\${NAME}"
    ${EndIf}
  ${EndIf}
SectionEnd
