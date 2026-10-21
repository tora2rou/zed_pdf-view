<#
.SYNOPSIS
    install.ps1 でインストールした Chihayaburu を取り除く。

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File packaging\windows\uninstall.ps1
#>
$ErrorActionPreference = 'Stop'

$AppName = 'Chihayaburu'
$ProgId = 'Chihayaburu.pdf'
$InstallDir = Join-Path $env:LOCALAPPDATA "Programs\$AppName"
$ShortcutPath = Join-Path ([Environment]::GetFolderPath('Programs')) "$AppName.lnk"
$Classes = 'HKCU:\Software\Classes'

function Remove-IfExists([string]$Path) {
    if (Test-Path -LiteralPath $Path) {
        Remove-Item -LiteralPath $Path -Recurse -Force
        Write-Host "削除しました: $Path"
    }
}

function Remove-ValueIfExists([string]$Path, [string]$Name) {
    if ((Test-Path -LiteralPath $Path) -and ($null -ne (Get-ItemProperty -LiteralPath $Path -Name $Name -ErrorAction SilentlyContinue))) {
        Remove-ItemProperty -LiteralPath $Path -Name $Name -Force
        Write-Host "削除しました: $Path\$Name"
    }
}

# 関連付け(install.ps1 で作ったものだけ)
Remove-IfExists "$Classes\$ProgId"
Remove-ValueIfExists "$Classes\.pdf\OpenWithProgids" $ProgId
Remove-IfExists "$Classes\Applications\$AppName.exe"
Remove-IfExists "HKCU:\Software\$AppName"
Remove-ValueIfExists 'HKCU:\Software\RegisteredApplications' $AppName

# ショートカットと本体
Remove-IfExists $ShortcutPath
Remove-IfExists $InstallDir

# 最近開いたファイルの記録(%APPDATA%\pdf_view_app)は、残しておく(再インストールしたときに使える)。

Add-Type -Namespace Chihayaburu -Name Shell -MemberDefinition @'
[System.Runtime.InteropServices.DllImport("shell32.dll")]
public static extern void SHChangeNotify(int eventId, uint flags, System.IntPtr item1, System.IntPtr item2);
'@
[Chihayaburu.Shell]::SHChangeNotify(0x08000000, 0, [IntPtr]::Zero, [IntPtr]::Zero)

Write-Host 'アンインストールが終わりました。'
