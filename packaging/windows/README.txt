Tuzuri - Typstなどの組版のためのエディタ (Zedのfork)

使い方:
  tuzuri.exe をダブルクリックして起動する。インストールは不要。
  好きな場所 (例: %LOCALAPPDATA%\Programs\Tuzuri) に置き、ショートカットを作ってください。

PDFへの書き出し (ウィンドウを出さない):
  start /wait tuzuri.exe export doc.typ          (コマンドプロンプト)
  Start-Process -Wait tuzuri.exe 'export','doc.typ'   (PowerShell)
  tuzuri.exe はウィンドウのアプリなので、start /wait などを付けないと、
  書き出しが終わるのを待たずにプロンプトが戻ります。

設定とデータは %APPDATA%\Tuzuri と %LOCALAPPDATA%\Tuzuri に置かれます。

https://github.com/tora2rou/zed_pdf-view (ブランチ Tuzuri)
