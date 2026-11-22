; Java Control Centre — NSIS installer hooks.
;
; The app needs a JDK at runtime (jcmd, jfr, jstack). Rust is build-time only.
; This hook runs before files are copied and makes a best-effort attempt to
; install a JDK when the machine has none, so a user who downloaded ONLY the
; installer can use every JCC feature.

!macro NSIS_HOOK_PREINSTALL
  DetailPrint "Checking Java prerequisites (JDK 17+ with jcmd)..."

  ; 1) jcmd already on PATH.
  ExecWait 'cmd /c where jcmd >nul 2>&1' $0
  ${If} $0 == 0
    DetailPrint "jcmd found on PATH - no Java installation needed."
    Goto jcc_java_ready
  ${EndIf}

  ; 2) jcmd in JAVA_HOME\bin.
  ReadEnvStr $1 JAVA_HOME
  ${If} "$1" != ""
    ${If} ${FileExists} "$1\bin\jcmd.exe"
      DetailPrint "jcmd found in JAVA_HOME."
      Goto jcc_java_ready
    ${EndIf}
  ${EndIf}

  ; 3) winget (Windows 10 1809+ / Windows 11) — Microsoft OpenJDK 21.
  ExecWait 'cmd /c where winget >nul 2>&1' $0
  ${If} $0 == 0
    DetailPrint "Installing Microsoft OpenJDK 21 with winget..."
    ExecWait 'winget install --id Microsoft.OpenJDK.21 -e --accept-source-agreements --accept-package-agreements --disable-interactivity' $0
    ${If} $0 == 0
      DetailPrint "Microsoft OpenJDK 21 installed."
      Goto jcc_java_ready
    ${EndIf}
  ${EndIf}

  ; 4) Fallback: Eclipse Temurin 21 MSI from the Adoptium API.
  ; Write a small PowerShell script to TEMP to avoid NSIS quoting problems.
  DetailPrint "Downloading Eclipse Temurin 21 JDK from Adoptium..."
  Delete "$TEMP\jcc-temurin-jdk21.msi"
  FileOpen $2 "$TEMP\jcc-download-jdk.ps1" w
  FileWrite $2 '[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12$\r$\n'
  FileWrite $2 'Invoke-WebRequest -Uri "https://api.adoptium.net/v3/binary/latest/21/ga/windows/x64/jdk/hotspot/normal/eclipse" -OutFile "$TEMP\jcc-temurin-jdk21.msi"$\r$\n'
  FileClose $2
  ExecWait 'powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$TEMP\jcc-download-jdk.ps1"' $0
  Delete "$TEMP\jcc-download-jdk.ps1"
  ${If} $0 == 0
    DetailPrint "Installing Eclipse Temurin 21 JDK..."
    ExecWait 'msiexec /i "$TEMP\jcc-temurin-jdk21.msi" /quiet /norestart' $0
    Delete "$TEMP\jcc-temurin-jdk21.msi"
    ${If} $0 == 0
      DetailPrint "Eclipse Temurin 21 installed."
      Goto jcc_java_ready
    ${EndIf}
  ${EndIf}

  MessageBox MB_ICONEXCLAMATION "JCC could not install a JDK automatically. Please install a JDK 17 or newer from https://adoptium.net and restart the installer. JCC will still install, but JVM monitoring and heap analysis require jcmd."

  jcc_java_ready:
!macroend

!macro NSIS_HOOK_POSTINSTALL
  DetailPrint "Java Control Centre installed. Rust is not required at runtime."
!macroend
