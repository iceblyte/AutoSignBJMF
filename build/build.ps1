<#
一键打包桌面版。

用法（在仓库根目录）：
  powershell -ExecutionPolicy Bypass -File build\build.ps1
  powershell -ExecutionPolicy Bypass -File build\build.ps1 -SkipZip   # 只出 exe

产物：
  dist\AutoSignBJMF.exe                 单文件版（数据写到 %USERPROFILE%\AutoSignBJMF）
  dist\AutoSignBJMF\                    绿色版目录（同级 data\ 存在，数据跟着目录走）
  dist\AutoSignBJMF-便携版.zip          绿色版压缩包（**不含**任何 Cookie 配置）
#>
param(
    [switch] $SkipZip
)

$ErrorActionPreference = 'Stop'

$root = Split-Path -Parent $PSScriptRoot
$python = Join-Path $root '.venv\Scripts\python.exe'
$icon = Join-Path $PSScriptRoot 'app.ico'

if (-not (Test-Path $python)) {
    throw "找不到虚拟环境：$python（先执行 python -m venv .venv 并安装 requirements-dev.txt）"
}

Write-Host '[1/4] 检查依赖'
& $python -c "import PyInstaller, pystray, PIL, tkinter" 2>$null
if ($LASTEXITCODE -ne 0) {
    throw '缺少打包依赖，请先执行：uv pip install -r requirements-desktop.txt pyinstaller'
}

Write-Host '[2/4] 生成图标'
& $python (Join-Path $PSScriptRoot 'make_icon.py')

Write-Host '[3/4] PyInstaller 打包'
& $python -m PyInstaller --noconfirm --clean --onefile --windowed `
    --name AutoSignBJMF `
    --icon $icon `
    --paths $root `
    --add-data "$root\desktop\assets;desktop\assets" `
    --hidden-import pystray._win32 `
    --hidden-import PIL.ImageTk `
    --hidden-import PIL._tkinter_finder `
    --distpath (Join-Path $root 'dist') `
    --workpath (Join-Path $root 'build\work') `
    --specpath (Join-Path $root 'build') `
    (Join-Path $root 'desktop\main.py')

if ($SkipZip) {
    Write-Host '完成：dist\AutoSignBJMF.exe'
    return
}

Write-Host '[4/4] 组装绿色版'
$stage = Join-Path $root 'dist\AutoSignBJMF'
if (Test-Path $stage) { Remove-Item -Recurse -Force -Confirm:$false $stage }
New-Item -ItemType Directory -Path (Join-Path $stage 'data') -Force | Out-Null
Copy-Item (Join-Path $root 'dist\AutoSignBJMF.exe') $stage
Copy-Item (Join-Path $root 'distribution\使用说明.txt') $stage
Copy-Item (Join-Path $root 'distribution\data-说明.txt') (Join-Path $stage 'data\说明.txt')

$zip = Join-Path $root 'dist\AutoSignBJMF-便携版.zip'
if (Test-Path $zip) { Remove-Item -Force -Confirm:$false $zip }
Compress-Archive -Path (Join-Path $stage '*') -DestinationPath $zip

Write-Host ''
Write-Host '完成：'
Write-Host "  exe    $(Join-Path $root 'dist\AutoSignBJMF.exe')"
Write-Host "  绿色版 $stage"
Write-Host "  压缩包 $zip"
Write-Host ''
Write-Host '提示：绿色版目录里如果再放一份自己的 data\data.json 就能直接双击运行；'
Write-Host '      zip 是干净的，不含任何凭证，可以安全分发。'
