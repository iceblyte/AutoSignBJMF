# 构建 Rust 版桌面端（Windows / MSVC 目标，静态 CRT 单文件）
#
# 用法：
#   powershell -File build\build-rust.ps1              # 只构建，产出 dist\AutoSignBJMF-rs.exe
#   powershell -File build\build-rust.ps1 -Release     # 额外产出 dist\release\ 下的
#                                                       #   AutoSignBJMF-v<版本>.exe
#                                                       #   AutoSignBJMF-portable-v<版本>.zip
#
# 版本号唯一来源是 rust\Cargo.toml 的 workspace version；app\app.rc 里的
# FILEVERSION 必须同步。脚本会核对两者，不一致直接报错（防止出现「资源里是旧版本」）。
#
# 依赖（都不需要安装 Visual Studio）：
#   - rustup 工具链 stable-x86_64-pc-windows-msvc
#   - LLVM 的 clang-cl / lld-link（scoop install llvm）
#   - MSVC 的 CRT 与 Windows SDK 头/库（由 cargo-xwin 拉到 D:\Tools\xwin-cache）
#     路径写在 rust\.cargo\config.toml 里
#   - release 构建还需要 fxc.exe：设 GPUI_FXC_PATH，或让它在 PATH 里
#     （取法见 rust/README.md，debug 构建不需要）
#
# 注意：`-crt-static` 是关键。不加的话 exe 依赖 vcruntime140.dll，
#       就不再是「单文件绿色版」了。

param(
    [switch]$Release
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$rust = Join-Path $root "rust"
$triple = "x86_64-pc-windows-msvc"

# ---- 版本号：Cargo.toml 与 app.rc 必须一致 ----
$cargoText = Get-Content (Join-Path $rust "Cargo.toml") -Raw
if ($cargoText -notmatch '(?m)^version\s*=\s*"([\d.]+)"') {
    throw "rust\Cargo.toml 里找不到 workspace version"
}
$version = $Matches[1]
$parts = @($version.Split('.'))
while ($parts.Count -lt 4) { $parts += "0" }
$rcVersion = ($parts[0..3] -join ',')
$rcText = Get-Content (Join-Path $rust "app\app.rc") -Raw
if ($rcText -notmatch ("FILEVERSION\s+" + [regex]::Escape($rcVersion))) {
    throw "app\app.rc 的 FILEVERSION 与 Cargo.toml 的 $version 不一致（应为 $rcVersion），先同步再打包"
}
Write-Host "版本：v$version" -ForegroundColor Cyan

# ---- 前置检查：缺什么就说清楚缺什么，而不是等链接时报一堆错 ----
$missing = @()
foreach ($tool in @("clang-cl", "lld-link", "llvm-lib")) {
    if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) { $missing += $tool }
}
if ($missing.Count -gt 0) {
    throw "PATH 里找不到 $($missing -join ', ')。先执行：scoop install llvm（装完要重开终端）"
}
if (-not $env:GPUI_FXC_PATH) {
    Write-Host "提示：未设置 GPUI_FXC_PATH，release 构建会在编译着色器时失败。" -ForegroundColor Yellow
    Write-Host "      取值方法见 rust\README.md「release 需要 fxc」一节。" -ForegroundColor Yellow
}

Push-Location $rust
try {
    # 静态 CRT + 独立 target 目录：单文件产物，且与 debug 构建互不踩
    $env:RUSTFLAGS = "-C target-feature=+crt-static"
    $env:CARGO_TARGET_DIR = "target-static"

    Write-Host "== cargo build --release --target $triple ==" -ForegroundColor Cyan
    # 注意：cargo 把进度写到 stderr，而 $ErrorActionPreference="Stop" 会把原生命令的
    # stderr 当成异常直接中断（NativeCommandError）。这里临时放宽，只看退出码。
    $previous = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    & cargo build --release -p bjmf-app --target $triple 2>&1 | ForEach-Object { Write-Host $_ }
    $code = $LASTEXITCODE
    $ErrorActionPreference = $previous
    if ($code -ne 0) { throw "cargo build 失败（exit $code）" }

    $exe = Join-Path $rust "target-static\$triple\release\AutoSignBJMF.exe"
    if (-not (Test-Path $exe)) { throw "找不到产物：$exe" }

    $dist = Join-Path $root "dist"
    New-Item -ItemType Directory -Force -Path $dist | Out-Null
    $target = Join-Path $dist "AutoSignBJMF-rs.exe"
    Copy-Item $exe $target -Force
    Write-Host ("产物：{0}（{1:N1} MB）" -f $target, ((Get-Item $target).Length / 1MB)) -ForegroundColor Green

    if ($Release) {
        $rel = Join-Path $dist "release"
        New-Item -ItemType Directory -Force -Path $rel | Out-Null
        $single = Join-Path $rel "AutoSignBJMF-v$version.exe"
        Copy-Item $target $single -Force

        # 便携包：只看 exe 同级有没有 data/，所以这里只放说明，不带任何账号数据
        $stage = Join-Path $rel "AutoSignBJMF-portable-v$version"
        if (Test-Path $stage) { Remove-Item $stage -Recurse -Force }
        New-Item -ItemType Directory -Force -Path (Join-Path $stage "data") | Out-Null
        Copy-Item $target (Join-Path $stage "AutoSignBJMF.exe") -Force
        Set-Content -Path (Join-Path $stage "data\说明.txt") -Encoding utf8 -Value @"
这个目录存在时，程序会把配置与日志写在这里（便携模式），不写用户目录。

- data.json     账号与坐标
- settings.json 界面设置（时间点、重试、开机自启等）
- logs\         每天一个日志文件

开机自启是**可选项**，默认关闭。在设置里打开后，会在本机写一条启动项
（HKCU\Software\Microsoft\Windows\CurrentVersion\Run\AutoSignBJMF），
不装服务、不写 Program Files；关掉开关即删除。其余配置始终只在本目录。

注意：data.json 里含 Cookie 与坐标，请勿分享给他人。
整个文件夹可以拷到 U 盘带走。
"@
        $zip = Join-Path $rel "AutoSignBJMF-portable-v$version.zip"
        if (Test-Path $zip) { Remove-Item $zip -Force }
        Compress-Archive -Path (Join-Path $stage "*") -DestinationPath $zip -CompressionLevel Optimal

        Write-Host ("单文件：{0}（{1:N1} MB）" -f $single, ((Get-Item $single).Length / 1MB)) -ForegroundColor Green
        Write-Host ("便携包：{0}（{1:N1} MB）" -f $zip, ((Get-Item $zip).Length / 1MB)) -ForegroundColor Green
        Write-Host "别忘了补 dist\release\notes-v$version.md" -ForegroundColor Yellow
    }
}
finally {
    Pop-Location
}
