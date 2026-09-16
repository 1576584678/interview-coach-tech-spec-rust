# 组装 Windows 便携包(exe + web + 启动脚本 + 使用说明),并压成 zip。
# 用法:
#   pwsh -File tools\make-dist.ps1                      # 默认取 bin\interview-coach.exe
#   pwsh -File tools\make-dist.ps1 -ExePath target\debug\interview-coach.exe
#   pwsh -File tools\make-dist.ps1 -NoZip               # 只生成目录,不打包
[CmdletBinding()]
param(
    [string]$ExePath = "bin\interview-coach.exe",
    [string]$OutDir = "dist",
    [string]$Name = "面试教练-便携版-win64",
    [switch]$NoZip
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot

function Resolve-FromRoot([string]$p) {
    if ([System.IO.Path]::IsPathRooted($p)) { return $p }
    return (Join-Path $root $p)
}

$exe = Resolve-FromRoot $ExePath
if (-not (Test-Path -LiteralPath $exe)) {
    throw "找不到可执行文件: $exe(先执行 cargo build,或把 debug 产物拷到 bin\interview-coach.exe)"
}

$outRoot = Resolve-FromRoot $OutDir
$pkg = Join-Path $outRoot $Name
if (Test-Path -LiteralPath $outRoot) { Remove-Item -LiteralPath $outRoot -Recurse -Force }
New-Item -ItemType Directory -Force -Path $pkg | Out-Null

Copy-Item -LiteralPath $exe (Join-Path $pkg "interview-coach.exe")
Copy-Item -LiteralPath (Join-Path $root "web") (Join-Path $pkg "web") -Recurse
Copy-Item -LiteralPath (Join-Path $root "config.example.toml") (Join-Path $pkg "config.example.toml")

# 启动脚本与说明文档保持与仓库内一致,单独生成一份适配"exe 就在同级目录"的版本
$cmd = @(
    '@echo off',
    'chcp 65001 >nul',
    'cd /d "%~dp0"',
    'title Interview Coach',
    'echo Starting Interview Coach ...',
    'echo   http://127.0.0.1:8080',
    'echo   Close this window (or press Ctrl+C) to stop.',
    'echo.',
    '"%~dp0interview-coach.exe" %*',
    'echo.',
    'echo Interview Coach exited with code %errorlevel%.',
    'pause'
)
Set-Content -LiteralPath (Join-Path $pkg "启动面试教练.cmd") -Value $cmd -Encoding ASCII

$usage = Join-Path $root "tools\usage.txt"
if (Test-Path -LiteralPath $usage) {
    Copy-Item -LiteralPath $usage (Join-Path $pkg "使用说明.txt")
}
else {
    Write-Warning "未找到 tools\usage.txt,跳过使用说明"
}

$size = (Get-ChildItem -LiteralPath $pkg -Recurse -File | Measure-Object -Property Length -Sum).Sum
Write-Host ("已生成目录: {0}  ({1:N1} MB)" -f $pkg, ($size / 1MB))

if ($NoZip) { return }

$zip = Join-Path $outRoot "$Name.zip"
Add-Type -AssemblyName System.IO.Compression.FileSystem
if (Test-Path -LiteralPath $zip) { Remove-Item -LiteralPath $zip -Force }
[System.IO.Compression.ZipFile]::CreateFromDirectory(
    $pkg, $zip, [System.IO.Compression.CompressionLevel]::Optimal, $true
)
$zipSize = (Get-Item -LiteralPath $zip).Length
Write-Host ("已生成压缩包: {0}  ({1:N1} MB)" -f $zip, ($zipSize / 1MB))
