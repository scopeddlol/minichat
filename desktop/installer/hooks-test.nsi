; A harness for hooks.nsh, so the hook can be tested without building the
; whole app around it.
;
; It mirrors what Tauri's generated script does with the hooks file — the
; same includes, the same execution level, the same shell context, the macro
; inserted at the same point in an install section — and nothing else. Built
; and run by the Windows job in .github/workflows/desktop.yml.
;
;   makensis hooks-test.nsi
;   hooks-test.exe /S /INSTANCE=https://chat.example.com
;
; It installs nothing: the only thing it exercises is the hook.

Unicode true
RequestExecutionLevel user

!include MUI2.nsh
!include FileFunc.nsh
!include StrFunc.nsh
${StrCase}
${StrLoc}

!include "hooks.nsh"

Name "MiniChat hook test"
OutFile "hooks-test.exe"
InstallDir "$TEMP\minichat-hook-test"

!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Function .onInit
  ; What Tauri's SetContext macro does for a currentUser install, and what
  ; makes $APPDATA the member's Roaming directory rather than the machine's.
  SetShellVarContext current
FunctionEnd

Section Install
  SetOutPath $INSTDIR

  ; What the hook has to work with, written out so a failure in CI says what
  ; the installer was actually handed rather than only that nothing happened.
  ${GetParameters} $0
  FileOpen $9 "$INSTDIR\parameters.txt" w
  FileWrite $9 "parameters: [$0]$\r$\n"
  FileWrite $9 "appdata: [$APPDATA]$\r$\n"
  FileClose $9

  !insertmacro NSIS_HOOK_POSTINSTALL

  ; And what it made of it.
  FileOpen $9 "$INSTDIR\parsed.txt" w
  FileWrite $9 "url: [$MinichatUrl]$\r$\n"
  FileClose $9
SectionEnd
