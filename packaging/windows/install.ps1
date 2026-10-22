<#
.SYNOPSIS
    Chihayaburu(PDFビューア)を、今のユーザーにインストールする。管理者権限は要らない。

.DESCRIPTION
    - exe を %LOCALAPPDATA%\Programs\Chihayaburu\ にコピーする
    - スタートメニューにショートカットを作る
    - PDF の「プログラムから開く」と、設定の「既定のアプリ」に Chihayaburu を登録する
      (Windows 10/11 では、アプリが自分で既定のアプリになることはできない。最初の1回だけ、
       PDF を右クリック →「プログラムから開く」→ Chihayaburu →「常に使う」を選ぶ)

.PARAMETER Exe
    インストールする exe。省略すると、スクリプトの隣の pdf_view_app.exe(配布のzip)、なければリポジトリの target\release\pdf_view_app.exe。

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File packaging\windows\install.ps1
#>
param(
    [string]$Exe = $(
        # 配布のzip(exeがスクリプトの隣にある)ならそれを、リポジトリ内ならビルド先を使う。
        $beside = Join-Path $PSScriptRoot 'pdf_view_app.exe'
        if (Test-Path -LiteralPath $beside) { $beside }
        else { Join-Path $PSScriptRoot '..\..\target\release\pdf_view_app.exe' }
    )
)

$ErrorActionPreference = 'Stop'

$AppName = 'Chihayaburu'
$ProgId = 'Chihayaburu.pdf'
$InstallDir = Join-Path $env:LOCALAPPDATA "Programs\$AppName"
$InstalledExe = Join-Path $InstallDir "$AppName.exe"
$ShortcutPath = Join-Path ([Environment]::GetFolderPath('Programs')) "$AppName.lnk"

if (-not (Test-Path -LiteralPath $Exe)) {
    throw "exe が見つかりません: $Exe`n先に 'cargo build --release -p pdf_view_app' でビルドしてください。"
}

# 1. exe をコピーする(起動中なら上書きできないので、先に閉じてもらう)
New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
Copy-Item -LiteralPath $Exe -Destination $InstalledExe -Force
Write-Host "コピーしました: $InstalledExe"

# 2. スタートメニューのショートカット
$shell = New-Object -ComObject WScript.Shell
$shortcut = $shell.CreateShortcut($ShortcutPath)
$shortcut.TargetPath = $InstalledExe
$shortcut.WorkingDirectory = $InstallDir
$shortcut.IconLocation = "$InstalledExe,0"
$shortcut.Description = 'Chihayaburu PDF Viewer'
$shortcut.Save()
Write-Host "スタートメニューに追加しました: $ShortcutPath"

# 3. ファイルの関連付け(すべて HKCU = 今のユーザーだけ)
function Set-RegistryValue([string]$Path, [string]$Name, [string]$Value) {
    if (-not (Test-Path -LiteralPath $Path)) {
        New-Item -Path $Path -Force | Out-Null
    }
    if ($Name -eq '') {
        Set-Item -LiteralPath $Path -Value $Value
    } else {
        New-ItemProperty -LiteralPath $Path -Name $Name -Value $Value -PropertyType String -Force | Out-Null
    }
}

$Classes = 'HKCU:\Software\Classes'
$OpenCommand = "`"$InstalledExe`" `"%1`""

# 3-1. ProgID: Chihayaburu で開く PDF の種類
Set-RegistryValue "$Classes\$ProgId" '' 'PDF Document (Chihayaburu)'
Set-RegistryValue "$Classes\$ProgId\DefaultIcon" '' "`"$InstalledExe`",0"
Set-RegistryValue "$Classes\$ProgId\shell\open\command" '' $OpenCommand

# 3-2. .pdf の「プログラムから開く」の候補に加える
Set-RegistryValue "$Classes\.pdf\OpenWithProgids" $ProgId ''

# 3-3. アプリとしての登録(「プログラムから開く」の一覧での名前)
Set-RegistryValue "$Classes\Applications\$AppName.exe" 'FriendlyAppName' $AppName
Set-RegistryValue "$Classes\Applications\$AppName.exe\SupportedTypes" '.pdf' ''
Set-RegistryValue "$Classes\Applications\$AppName.exe\shell\open\command" '' $OpenCommand

# 3-4. 設定の「既定のアプリ」に出す
Set-RegistryValue "HKCU:\Software\$AppName\Capabilities" 'ApplicationName' $AppName
Set-RegistryValue "HKCU:\Software\$AppName\Capabilities" 'ApplicationDescription' 'PDF Viewer'
Set-RegistryValue "HKCU:\Software\$AppName\Capabilities\FileAssociations" '.pdf' $ProgId
Set-RegistryValue 'HKCU:\Software\RegisteredApplications' $AppName "Software\$AppName\Capabilities"

# 4. エクスプローラーに、関連付けが変わったことを知らせる(アイコンの表示などを更新)
Add-Type -Namespace Chihayaburu -Name Shell -MemberDefinition @'
[System.Runtime.InteropServices.DllImport("shell32.dll")]
public static extern void SHChangeNotify(int eventId, uint flags, System.IntPtr item1, System.IntPtr item2);
'@
[Chihayaburu.Shell]::SHChangeNotify(0x08000000, 0, [IntPtr]::Zero, [IntPtr]::Zero)

Write-Host ''
Write-Host 'インストールが終わりました。'
Write-Host 'PDF をダブルクリックで開くには、最初の1回だけ:'
Write-Host '  PDF を右クリック →「プログラムから開く」→「別のプログラムを選択」→ Chihayaburu →「常に使う」'
