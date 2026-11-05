<#
Windows 开机常驻签到注册脚本（需以管理员身份运行一次）。

用法：
  .\register-windows-task.ps1                          # 注册：登录时用 signin_loop.py 每天定时签到
  .\register-windows-task.ps1 -Times 07:30,21:31       # 指定每天的时间点（本地时间）
  .\register-windows-task.ps1 -Mode web                # 注册：登录时常驻 web_signin 管理端
  .\register-windows-task.ps1 -DryRun                  # 只打印将要执行的命令，不改系统
  .\register-windows-task.ps1 -Uninstall               # 删除计划任务
#>

[CmdletBinding()]
param(
    [ValidateSet('loop', 'web')]
    [string] $Mode = 'loop',

    [string] $Times = '21:31',

    [string] $Python = '',

    [string] $TaskName = 'BJMF-AutoSignIn',

    [switch] $DryRun,

    [switch] $Uninstall
)

$ErrorActionPreference = 'Stop'

# 脚本放在 deploy/ 下，仓库根目录是它的上一级
$RepoRoot = Split-Path -Parent $PSScriptRoot

if ($Uninstall) {
    $existing = Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue
    if ($existing) {
        Unregister-ScheduledTask -TaskName $TaskName -Confirm:$false
        Write-Host "已删除计划任务 $TaskName"
    }
    else {
        Write-Host "没有找到计划任务 $TaskName"
    }
    return
}

if (-not $Python) {
    $venvPython = Join-Path $RepoRoot '.venv\Scripts\python.exe'
    $Python = if (Test-Path $venvPython) { $venvPython } else { 'python' }
}

if ($Mode -eq 'loop') {
    $script = Join-Path $RepoRoot 'signin_loop.py'
    if (-not (Test-Path $script)) {
        throw "找不到 $script，请在仓库根目录下运行本脚本。"
    }
    $argument = "`"$script`" --times $Times"
}
else {
    $script = Join-Path $RepoRoot 'web_signin\run.py'
    if (-not (Test-Path $script)) {
        throw "找不到 $script，请在仓库根目录下运行本脚本。"
    }
    # 进程常驻，内置 APScheduler 按 Asia/Shanghai 到点执行
    $argument = "web_signin\run.py"
}

if ($DryRun) {
    Write-Host "[DryRun] 将以管理员权限注册计划任务 $TaskName"
    Write-Host "[DryRun] 触发器: 用户登录时"
    Write-Host "[DryRun] 可执行文件: $Python"
    Write-Host "[DryRun] 参数: $argument"
    Write-Host "[DryRun] 工作目录: $RepoRoot"
    return
}

$action = New-ScheduledTaskAction -Execute $Python -Argument $argument -WorkingDirectory $RepoRoot

$trigger = New-ScheduledTaskTrigger -AtLogOn

# Hidden 让进程没有控制台窗口；ExecutionTimeLimit 设为 0 表示不限时，常驻进程不会被系统杀掉
$settings = New-ScheduledTaskSettingsSet -Hidden `
    -AllowStartIfOnBatteries `
    -DontStopIfGoingOnBatteries `
    -StartWhenAvailable `
    -RestartCount 3 `
    -RestartInterval (New-TimeSpan -Minutes 1) `
    -ExecutionTimeLimit (New-TimeSpan -Days 0)

Register-ScheduledTask -TaskName $TaskName -Action $action -Trigger $trigger -Settings $settings -Force | Out-Null

Write-Host "已注册计划任务 $TaskName（模式: $Mode）"
if ($Mode -eq 'loop') {
    Write-Host "每天执行时间点（本地时间）: $Times"
}
else {
    Write-Host "管理端地址: http://127.0.0.1:9988"
}
Write-Host "立即启动一次可执行： Start-ScheduledTask -TaskName `"$TaskName`""
