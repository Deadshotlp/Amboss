; Amboss – Windows-Installer (NSIS 3)
;
; Installiert für den aktuellen Benutzer nach %LOCALAPPDATA%\Programs\Amboss – ohne
; Administratorrechte, und Amboss kann sich dort später selbst aktualisieren.
;
; Bauen (Linux oder Windows, NSIS 3):
;   makensis -DVERSION=0.10.0 -DSRC=../dist/pack/amboss installer/amboss.nsi
; SRC = Ordner mit amboss.exe, README.md, Lizenzen usw.

Unicode true
SetCompressor /SOLID lzma

!ifndef VERSION
  !define VERSION "0.0.0"
!endif
!ifndef SRC
  !define SRC "..\dist\pack\amboss"
!endif
!ifndef OUTFILE
  !define OUTFILE "..\dist\amboss-${VERSION}-windows-setup.exe"
!endif

!define APP "Amboss"
!define UNINST_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\Amboss"

Name "${APP} ${VERSION}"
OutFile "${OUTFILE}"
InstallDir "$LOCALAPPDATA\Programs\Amboss"
InstallDirRegKey HKCU "Software\Amboss" "InstallDir"
RequestExecutionLevel user
BrandingText "Amboss ${VERSION}"

!include "MUI2.nsh"
!define MUI_ICON "..\assets\icon\amboss.ico"
!define MUI_UNICON "..\assets\icon\amboss.ico"
!define MUI_ABORTWARNING
!define MUI_FINISHPAGE_RUN "$INSTDIR\amboss.exe"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "..\LICENSE-MIT"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "German"
!insertmacro MUI_LANGUAGE "English"
!insertmacro MUI_LANGUAGE "French"
!insertmacro MUI_LANGUAGE "Spanish"
!insertmacro MUI_LANGUAGE "Polish"
!insertmacro MUI_LANGUAGE "Russian"

LangString SecMain ${LANG_GERMAN} "Amboss (erforderlich)"
LangString SecMain ${LANG_ENGLISH} "Amboss (required)"
LangString SecMain ${LANG_FRENCH} "Amboss (requis)"
LangString SecMain ${LANG_SPANISH} "Amboss (obligatorio)"
LangString SecMain ${LANG_POLISH} "Amboss (wymagany)"
LangString SecMain ${LANG_RUSSIAN} "Amboss (обязательно)"
LangString SecDesk ${LANG_GERMAN} "Verknüpfung auf dem Desktop"
LangString SecDesk ${LANG_ENGLISH} "Desktop shortcut"
LangString SecDesk ${LANG_FRENCH} "Raccourci sur le bureau"
LangString SecDesk ${LANG_SPANISH} "Acceso directo en el escritorio"
LangString SecDesk ${LANG_POLISH} "Skrót na pulpicie"
LangString SecDesk ${LANG_RUSSIAN} "Ярлык на рабочем столе"
LangString SecVmf ${LANG_GERMAN} ".vmf-Dateien mit Amboss öffnen"
LangString SecVmf ${LANG_ENGLISH} "Open .vmf files with Amboss"
LangString SecVmf ${LANG_FRENCH} "Ouvrir les fichiers .vmf avec Amboss"
LangString SecVmf ${LANG_SPANISH} "Abrir archivos .vmf con Amboss"
LangString SecVmf ${LANG_POLISH} "Otwieraj pliki .vmf w Amboss"
LangString SecVmf ${LANG_RUSSIAN} "Открывать файлы .vmf в Amboss"

Section "!$(SecMain)" SecMainId
  SectionIn RO
  SetOutPath "$INSTDIR"
  ; laufende Instanz kann die Datei sperren
  Delete "$INSTDIR\amboss.old.exe"
  File /r "${SRC}\*.*"
  WriteUninstaller "$INSTDIR\uninstall.exe"
  WriteRegStr HKCU "Software\Amboss" "InstallDir" "$INSTDIR"
  CreateDirectory "$SMPROGRAMS\Amboss"
  CreateShortcut "$SMPROGRAMS\Amboss\Amboss.lnk" "$INSTDIR\amboss.exe"
  CreateShortcut "$SMPROGRAMS\Amboss\Amboss deinstallieren.lnk" "$INSTDIR\uninstall.exe"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayName" "Amboss"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNINST_KEY}" "Publisher" "Deadshot"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayIcon" "$INSTDIR\amboss.exe"
  WriteRegStr HKCU "${UNINST_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINST_KEY}" "URLInfoAbout" "https://github.com/Deadshotlp/Amboss"
  WriteRegStr HKCU "${UNINST_KEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoRepair" 1
SectionEnd

Section "$(SecDesk)" SecDeskId
  CreateShortcut "$DESKTOP\Amboss.lnk" "$INSTDIR\amboss.exe"
SectionEnd

Section /o "$(SecVmf)" SecVmfId
  WriteRegStr HKCU "Software\Classes\.vmf\OpenWithProgids" "Amboss.vmf" ""
  WriteRegStr HKCU "Software\Classes\Amboss.vmf" "" "Valve Map File"
  WriteRegStr HKCU "Software\Classes\Amboss.vmf\DefaultIcon" "" "$INSTDIR\amboss.exe,0"
  WriteRegStr HKCU "Software\Classes\Amboss.vmf\shell\open\command" "" '"$INSTDIR\amboss.exe" "%1"'
  WriteRegStr HKCU "Software\Classes\.vmf" "" "Amboss.vmf"
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'
SectionEnd

Section "Uninstall"
  Delete "$DESKTOP\Amboss.lnk"
  RMDir /r "$SMPROGRAMS\Amboss"
  ; nur eigene Dateien – Maps des Benutzers bleiben unberührt
  Delete "$INSTDIR\amboss.exe"
  Delete "$INSTDIR\amboss.old.exe"
  Delete "$INSTDIR\uninstall.exe"
  Delete "$INSTDIR\*.md"
  Delete "$INSTDIR\LICENSE-*"
  Delete "$INSTDIR\*-OFL.txt"
  Delete "$INSTDIR\beispiel_raum.vmf"
  Delete "$INSTDIR\funktionen.vmf"
  RMDir "$INSTDIR"
  ReadRegStr $0 HKCU "Software\Classes\.vmf" ""
  StrCmp $0 "Amboss.vmf" 0 +2
    DeleteRegValue HKCU "Software\Classes\.vmf" ""
  DeleteRegValue HKCU "Software\Classes\.vmf\OpenWithProgids" "Amboss.vmf"
  DeleteRegKey HKCU "Software\Classes\Amboss.vmf"
  DeleteRegKey HKCU "${UNINST_KEY}"
  DeleteRegKey HKCU "Software\Amboss"
SectionEnd
