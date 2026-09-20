; MiniChat's additions to the installer Tauri generates.
;
; Tauri includes this file into its own NSIS script and calls each macro at
; the point its name says. Two rules apply to everything here:
;
;   * it has to work unattended. An installer that stops on a message box
;     during a silent install is an installer that hangs a deployment, so
;     every prompt carries a /SD default.
;   * it must not be load-bearing. Nothing below is required for the app to
;     run; if a hook does nothing, the app still installs and still works.

; Both have include guards, so including them again alongside Tauri's own
; includes costs nothing.
!include "FileFunc.nsh"
!include "LogicLib.nsh"

; Where the app keeps its settings. Tauri's `app_config_dir()` on Windows is
; %APPDATA%\<identifier>, and the identifier is in tauri.conf.json.
!define MINICHAT_CONFIG "$APPDATA\chat.mini.desktop"
; Named rather than $R0 and friends, which the surrounding template also uses.
Var MinichatArgs
Var MinichatUrl
Var MinichatScratch
Var MinichatLength
Var MinichatIndex
Var MinichatChar
Var MinichatFile

!macro NSIS_HOOK_POSTINSTALL
  ; /INSTANCE=https://chat.example.com fills in the address the app would
  ; otherwise ask for on first launch. A self-hosted community hands out one
  ; address to everybody, and an admin putting this on twenty machines should
  ; not have to talk twenty people through typing it:
  ;
  ;   MiniChat_0.5.0_x64-setup.exe /S /INSTANCE=https://chat.example.com
  ;
  ; The app re-validates whatever it reads, so the worst a bad value can do
  ; is send someone to the connect screen they would have seen anyway.
  ${GetParameters} $MinichatArgs
  Call ReadInstanceOption
  ${If} $MinichatUrl != ""
    ${If} ${FileExists} "${MINICHAT_CONFIG}\settings.json"
      ; Reinstalling over an existing install, or installing beside settings
      ; someone already has. What they chose wins over the command line.
      DetailPrint "MiniChat: keeping the instance address already configured"
    ${Else}
      CreateDirectory "${MINICHAT_CONFIG}"
      ClearErrors
      FileOpen $MinichatFile "${MINICHAT_CONFIG}\settings.json" w
      ${If} ${Errors}
        DetailPrint "MiniChat: could not write the instance address"
      ${Else}
        ; Only the address. Every other setting has a default in the app,
        ; and writing them here would be a second place to keep correct.
        FileWrite $MinichatFile '{$\r$\n  "instance_url": "$MinichatUrl"$\r$\n}$\r$\n'
        FileClose $MinichatFile
        DetailPrint "MiniChat: first launch will connect to $MinichatUrl"
      ${EndIf}
    ${EndIf}
  ${EndIf}
!macroend

; Read /INSTANCE=<url> off the command line into $MinichatUrl, empty when it
; is absent or not something to hand the app.
;
; Not `${GetOptions}`, which is the obvious way to do this and is wrong here:
; it ends a value at the next switch character, and the switch character is
; "/", so it reads `/INSTANCE=https://chat.example.com` as `https:` and hands
; that back without an error. Every URL anyone would pass contains "//".
;
; A URL cannot contain a space, so the value runs to the next space or to the
; end of the line. A quoted value runs to its closing quote, because that is
; what someone who quoted it meant.
Function ReadInstanceOption
  StrCpy $MinichatUrl ""

  ; Find the switch. String comparison in NSIS ignores case, so /instance=
  ; works as well, which is what anyone typing it at a prompt will do.
  StrLen $MinichatLength $MinichatArgs
  StrCpy $MinichatIndex 0
  ${Do}
    ${If} $MinichatIndex >= $MinichatLength
      Return
    ${EndIf}
    StrCpy $MinichatScratch $MinichatArgs 10 $MinichatIndex
    ${If} $MinichatScratch == "/INSTANCE="
      ${ExitDo}
    ${EndIf}
    IntOp $MinichatIndex $MinichatIndex + 1
  ${Loop}

  ; Everything after it, for now.
  IntOp $MinichatIndex $MinichatIndex + 10
  StrCpy $MinichatUrl $MinichatArgs "" $MinichatIndex

  ; Then cut it short at whatever ends the value.
  StrCpy $MinichatChar $MinichatUrl 1
  ${If} $MinichatChar == '"'
    StrCpy $MinichatUrl $MinichatUrl "" 1
    StrCpy $MinichatScratch '"'
  ${Else}
    StrCpy $MinichatScratch " "
  ${EndIf}

  StrLen $MinichatLength $MinichatUrl
  StrCpy $MinichatIndex 0
  ${Do}
    ${If} $MinichatIndex >= $MinichatLength
      ${ExitDo}
    ${EndIf}
    StrCpy $MinichatChar $MinichatUrl 1 $MinichatIndex
    ${If} $MinichatChar == $MinichatScratch
      StrCpy $MinichatUrl $MinichatUrl $MinichatIndex
      ${ExitDo}
    ${EndIf}
    IntOp $MinichatIndex $MinichatIndex + 1
  ${Loop}

  Call CheckInstanceUrl
FunctionEnd

; Empty $MinichatUrl unless it is an http(s) address with nothing in it that
; would need escaping to sit inside JSON.
;
; The quote and backslash checks are not a security boundary — whoever runs
; the installer could write the file themselves — but a malformed
; settings.json is ignored by the app without a word, and an installer that
; quietly does nothing is worse than one that says why.
Function CheckInstanceUrl
  StrCpy $MinichatScratch $MinichatUrl 7
  ${If} $MinichatScratch != "http://"
    StrCpy $MinichatScratch $MinichatUrl 8
    ${If} $MinichatScratch != "https://"
      DetailPrint "MiniChat: ignoring /INSTANCE (http:// or https:// only)"
      StrCpy $MinichatUrl ""
      Return
    ${EndIf}
  ${EndIf}

  StrLen $MinichatLength $MinichatUrl
  StrCpy $MinichatIndex 0
  ${Do}
    ${If} $MinichatIndex >= $MinichatLength
      ${ExitDo}
    ${EndIf}
    StrCpy $MinichatChar $MinichatUrl 1 $MinichatIndex
    ${If} $MinichatChar == '"'
    ; NSIS has no escape for a backslash: it is not a special character.
    ${OrIf} $MinichatChar == "\"
      DetailPrint "MiniChat: ignoring /INSTANCE (unusable characters)"
      StrCpy $MinichatUrl ""
      ${ExitDo}
    ${EndIf}
    IntOp $MinichatIndex $MinichatIndex + 1
  ${Loop}
FunctionEnd

; There is deliberately no NSIS_HOOK_PREUNINSTALL or NSIS_HOOK_POSTUNINSTALL.
; Tauri's uninstaller already offers to delete the app's data, on its own
; page, and removes exactly these two directories when you tick it:
;
;   %APPDATA%\chat.mini.desktop        the settings above
;   %LOCALAPPDATA%\chat.mini.desktop   the WebView2 profile, and the session
;
; A hook here would be a second prompt asking the same question after the
; first one had been answered. An unattended uninstall leaves both alone,
; which is what a member reinstalling the client wants.
