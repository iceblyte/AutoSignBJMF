# 构建 Rust 版桌面端（Windows）
#
# 用法：
#   powershell -File build\build-rust.ps1              # 只构建 release
#   powershell -File build\build-rust.ps1 -Portable    # 额外打一个便携 zip
#
# 依赖：
#   - Rust（stable，x86_64-pc-windows-*）
#   - release 构建需要 fxc.exe：设 GPUI_FXC_PATH，或让它在 PATH 里
#     （取法见 rust/README.md，debug 构建不需要）

param(
    [switch]$Portable
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$rust = Join-Path $root "rust"

Push-Location $rust
try {
    Write-Host "== cargo build --release ==" -ForegroundColor Cyan
    cargo build --release -p bjmf-app
    if ($LASTEXITCODE -ne 0) { throw "cargo build 失败" }

    $exe = Join-Path $rust "target\release\AutoSignBJMF.exe"
    if (-not (Test-Path $exe)) { throw "找不到产物：$exe" }

    $dist = Join-Path $root "dist"
    New-Item -ItemType Directory -Force -Path $dist | Out-Null
    $target = Join-Path $dist "AutoSignBJMF-rs.exe"
    Copy-Item $exe $target -Force

    $size = [math]::Round((Get-Item $target).Length / 1MB, 2)
    Write-Host ("产物：{0}（{1} MB）" -f $target, $size) -ForegroundColor Green

    if ($Portable) {
        $stage = Join-Path $dist "AutoSignBJMF-portable"
        if (Test-Path $stage) { Remove-Item $stage -Recurse -Force }
        New-Item -ItemType Directory -Force -Path $stage | Out-Null
        # 便携模式只看 exe 同级有没有 data/；只放说明，不带任何账号数据
        New-Item -ItemType Directory -Force -Path (Join-Path $stage "data") | Out-Null
        Copy-Item $target (Join-Path $stage "AutoSignBJMF.exe") -Force
        Set-Content -Path (Join-Path $stage "data\说明.txt") -Encoding utf8 -Value @"
这个目录存在时，程序会把配置与日志写在这里（便携模式），不写用户目录。
data.json 里含 Cookie 与坐标，请勿分享给他人。
"@
        $zip = Join-Path $dist "AutoSignBJMF-rs-portable.zip"
        if (Test-Path $zip) { Remove-Item $zip -Force }
        Compress-Archive -Path (Join-Path $stage "*") -DestinationPath $zip
        Write-Host ("便携包：{0}" -f $zip) -ForegroundColor Green
    }
}
finally {
    Pop-Location
}
