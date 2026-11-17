; ==============================================================================
; RepoShelf v1.1.4 — NSIS Multilingual Installer Language Corrective Hook
;
; Purpose:
;   Corrects language selection and Windows UI locale fallback in NSIS installers.
;   Ensures:
;     1. Trilingual support (English, SimpChinese, TradChinese) in a single setup.exe.
;     2. Pre-selection aligns with Windows UI language (zh-CN -> 2052, zh-TW/HK/MO -> 1028, en-* -> 1033).
;     3. Non-supported system UI languages reliably fall back to English (1033).
;     4. Language selection dialog is shown during interactive installs & manual upgrades,
;        even if a stale "Installer Language" registry entry exists.
;     5. User's manual selection is respected and persisted to registry.
;     6. Language dialog is strictly suppressed during passive (/P), auto-update (/UPDATE),
;        and silent (/S) operations so Tauri Updater is never blocked.
;     7. 100% non-destructive: zero modifications to user storage, application databases, or user settings.
; ==============================================================================

; --- Undefine upstream default macro from MUI2 Localization.nsh ---
!macroundef MUI_LANGDLL_DISPLAY

; --- Define corrective MUI_LANGDLL_DISPLAY macro ---
!macro MUI_LANGDLL_DISPLAY

  !verbose push
  !verbose ${MUI_VERBOSE}

  !insertmacro MUI_LANGDLL_VARIABLES

  !insertmacro MUI_DEFAULT MUI_LANGDLL_WINDOWTITLE "Installer Language"
  !insertmacro MUI_DEFAULT MUI_LANGDLL_INFO "Please select a language."

  Push $R9
  Push $0
  Push $1
  Push $2

  ; --------------------------------------------------------------------------
  ; Step 1: Detect Windows UI language using Win32 GetUserDefaultUILanguage()
  ; --------------------------------------------------------------------------
  ; Default fallback is English (1033)
  StrCpy $R9 1033

  System::Call 'kernel32::GetUserDefaultUILanguage() i .r0'
  ; Primary language = r0 & 0x03FF
  IntOp $1 $0 & 0x03FF
  ; Sublanguage = r0 >> 10
  IntOp $2 $0 >> 10

  ${If} $1 = 4 ; LANG_CHINESE
    ${If} $2 = 1 ; SUBLANG_CHINESE_TRADITIONAL (Taiwan)
    ${OrIf} $2 = 3 ; SUBLANG_CHINESE_HONGKONG (Hong Kong)
    ${OrIf} $2 = 5 ; SUBLANG_CHINESE_MACAU (Macau)
      StrCpy $R9 1028
    ${Else}
      ; Simplified Chinese (PRC, Singapore, etc.)
      StrCpy $R9 2052
    ${EndIf}
  ${ElseIf} $1 = 9 ; LANG_ENGLISH
    StrCpy $R9 1033
  ${Else}
    ; Non-supported locale -> fallback to English
    StrCpy $R9 1033
  ${EndIf}

  ; --------------------------------------------------------------------------
  ; Step 2: Ensure $PassiveMode and $UpdateMode are detected from $CMDLINE
  ; (Handles edge cases such as un.onInit where /P may not yet be parsed)
  ; --------------------------------------------------------------------------
  ${If} $PassiveMode != 1
    ${GetOptions} $CMDLINE "/P" $0
    ${IfNot} ${Errors}
      StrCpy $PassiveMode 1
    ${EndIf}
  ${EndIf}
  ${If} $PassiveMode != 1
    ${GetOptions} $CMDLINE "/passive" $0
    ${IfNot} ${Errors}
      StrCpy $PassiveMode 1
    ${EndIf}
  ${EndIf}
  ${If} $UpdateMode != 1
    ${GetOptions} $CMDLINE "/UPDATE" $0
    ${IfNot} ${Errors}
      StrCpy $UpdateMode 1
    ${EndIf}
  ${EndIf}

  ; --------------------------------------------------------------------------
  ; Step 3: Read saved installer language from registry if present
  ; --------------------------------------------------------------------------
  !ifdef MUI_LANGDLL_REGISTRY_VARIABLES
    ReadRegStr $mui.LangDLL.RegistryLanguage "${MUI_LANGDLL_REGISTRY_ROOT}" "${MUI_LANGDLL_REGISTRY_KEY}" "${MUI_LANGDLL_REGISTRY_VALUENAME}"
  !endif

  ; --------------------------------------------------------------------------
  ; Step 4: Branch by execution mode
  ; --------------------------------------------------------------------------
  ${If} $PassiveMode = 1
  ${OrIf} $UpdateMode = 1
  ${OrIf} ${Silent}
    ; Automated / passive / silent mode: NEVER DISPLAY DIALOG
    ; In this mode, prefer existing saved registry language if valid,
    ; otherwise fall back to detected Windows UI language ($R9).
    !ifdef MUI_LANGDLL_REGISTRY_VARIABLES
      ${If} $mui.LangDLL.RegistryLanguage != ""
        StrCpy $LANGUAGE $mui.LangDLL.RegistryLanguage
      ${Else}
        StrCpy $LANGUAGE $R9
      ${EndIf}
    !else
      StrCpy $LANGUAGE $R9
    !endif

  ${Else}
    ; Interactive mode (Fresh install or manual upgrade):
    ; ALWAYS display dialog. Default pre-selection follows Windows UI language ($R9).
    StrCpy $LANGUAGE $R9

    !ifdef MUI_LANGDLL_ALLLANGUAGES
      LangDLL::LangDialog "${MUI_LANGDLL_WINDOWTITLE}" "${MUI_LANGDLL_INFO}" A ${MUI_LANGDLL_LANGUAGES} ""
    !else
      LangDLL::LangDialog "${MUI_LANGDLL_WINDOWTITLE}" "${MUI_LANGDLL_INFO}" AC ${MUI_LANGDLL_LANGUAGES_CP} ""
    !endif

    Pop $LANGUAGE
    ${If} $LANGUAGE == "cancel"
      Abort
    ${EndIf}

    ; Persist user's choice to registry for uninstaller and future sessions
    !ifdef MUI_LANGDLL_REGISTRY_VARIABLES
      WriteRegStr "${MUI_LANGDLL_REGISTRY_ROOT}" "${MUI_LANGDLL_REGISTRY_KEY}" "${MUI_LANGDLL_REGISTRY_VALUENAME}" $LANGUAGE
    !endif

  ${EndIf}

  Pop $2
  Pop $1
  Pop $0
  Pop $R9

  !verbose pop

!macroend

; --- Standard No-Op Lifecycle Hooks (Ensuring 100% data safety) ---
!macro NSIS_HOOK_PREINSTALL
  ; Safe no-op: no file deletion, no data alterations
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ; Safe no-op: no file deletion, no data alterations
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; Safe no-op: no file deletion, no data alterations
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; Safe no-op: no file deletion, no data alterations
!macroend
