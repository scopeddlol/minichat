; /INSTANCE=<url>, so a community can hand out a configured installer.
;
; Everyone on one instance types the same address, and an admin putting the
; client on twenty machines should not have to talk twenty people through
; typing it:
;
;   MiniChat-0.6.1-x86_64-setup.exe /S /INSTANCE=https://chat.example.com
;
; The address is written only when there are no settings already, so a
; reinstall never overrides what someone chose, and the client re-validates
; whatever it reads — the worst a bad value can do is show the connect
; screen it would have shown anyway.

!include "FileFunc.nsh"
!include "LogicLib.nsh"

; directories' Windows layout for ProjectDirs::from("chat", "mini",
; "MiniChat"), which is what src/settings.rs asks for.
!define MINICHAT_CONFIG "$APPDATA\mini\MiniChat\config"

Var MinichatArgs
Var MinichatUrl
Var MinichatScratch
Var MinichatLength
Var MinichatIndex
Var MinichatChar
Var MinichatFile

Function SeedInstanceAddress
  ${GetParameters} $MinichatArgs
  Call ReadInstanceOption
  ${If} $MinichatUrl == ""
    Return
  ${EndIf}

  ${If} ${FileExists} "${MINICHAT_CONFIG}\native.json"
    DetailPrint "MiniChat: keeping the instance address already configured"
    Return
  ${EndIf}

  CreateDirectory "${MINICHAT_CONFIG}"
  ClearErrors
  FileOpen $MinichatFile "${MINICHAT_CONFIG}\native.json" w
  ${If} ${Errors}
    DetailPrint "MiniChat: could not write the instance address"
    Return
  ${EndIf}
  ; Only the address: every other setting has a default in the client, and
  ; writing them here would be a second place to keep correct.
  FileWrite $MinichatFile '{$\r$\n  "instance_url": "$MinichatUrl"$\r$\n}$\r$\n'
  FileClose $MinichatFile
  DetailPrint "MiniChat: first launch will connect to $MinichatUrl"
FunctionEnd

; Read /INSTANCE=<url> off the command line into $MinichatUrl, empty when it
; is absent or not something to hand the client.
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

  ; String comparison in NSIS ignores case, so /instance= works as well,
  ; which is what anyone typing it at a prompt will write.
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

  IntOp $MinichatIndex $MinichatIndex + 10
  StrCpy $MinichatUrl $MinichatArgs "" $MinichatIndex

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
