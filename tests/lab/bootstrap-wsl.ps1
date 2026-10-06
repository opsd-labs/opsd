# 创建专用 WSL2 实验发行版并安装本地实验依赖。
#
# 设计约束：
# - 不依赖宿主机 Docker。模板 rootfs 直接来自现有 WSL 发行版的 wsl --export。
# - 每个实验发行版内独立安装 Docker Engine、Compose 与防火墙工具。
# - 只操作匹配 ^opsd_ 的发行版；现有 Debian / Debian-HyperOS 一律不停止、不注销、不修改。
param(
  [string[]]$Distros = @('opsd_hub_lab', 'opsd_c001_lab', 'opsd_c052_lab', 'opsd_c061_lab'),
  [string]$TemplateDistro = 'Debian',
  [string]$RootfsDir = "$env:USERPROFILE\opsd-wsl-lab",
  [string]$NodeVersion = '22.20.0'
)
$ErrorActionPreference = 'Stop'

function Assert-LabDistro([string]$Name) {
  if ($Name -notmatch '^opsd_[a-z0-9_]+$') { throw "拒绝操作非实验发行版：$Name" }
}

function Invoke-Wsl([string]$Distro, [string[]]$Command) {
  $errorFile = [IO.Path]::GetTempFileName()
  try {
    $stdout = & wsl.exe -d $Distro -u root -- @Command 2> $errorFile
    $code = $LASTEXITCODE
    $stderr = if (Test-Path $errorFile) { Get-Content -LiteralPath $errorFile -Raw } else { '' }
    $text = ($stdout | Out-String).TrimEnd()
    if ($code -ne 0 -and $stderr.Trim()) { $text = ($text + [Environment]::NewLine + $stderr.Trim()).Trim() }
    return @{ Code = $code; Output = $text; Error = $stderr }
  } finally {
    Remove-Item -LiteralPath $errorFile -Force -ErrorAction SilentlyContinue
  }
}

function Convert-WindowsPathToWsl([string]$Path) {
  $full = [IO.Path]::GetFullPath($Path)
  if ($full -notmatch '^(?<drive>[A-Za-z]):\\(?<rest>.*)$') { throw "无法转换为 WSL 路径：$full" }
  return "/mnt/$($matches.drive.ToLower())/$($matches.rest.Replace('\', '/'))"
}

function Get-DistroNames {
  return @((wsl.exe --list --quiet 2>$null) | ForEach-Object { (($_ -replace [char]0, '').Trim()) } | Where-Object { $_ })
}

foreach ($d in $Distros) { Assert-LabDistro $d }
if ($TemplateDistro -notmatch '^[A-Za-z0-9._-]+$') { throw "模板发行版名称非法：$TemplateDistro" }

$wslStatus = wsl --status 2>&1 | Out-String
if ($LASTEXITCODE -ne 0) { throw "无法读取 WSL 状态；未创建任何发行版。`n$wslStatus" }

$existing = Get-DistroNames
if ($existing -notcontains $TemplateDistro) {
  throw "模板发行版 $TemplateDistro 不存在；现有：$($existing -join ', ')。本脚本不下载镜像，请先准备模板发行版。"
}
# 模板发行版必须是可用的 WSL2；否则导出出来的 rootfs 不完整。
$templateKernel = Invoke-Wsl $TemplateDistro @('uname', '-r')
if ($templateKernel.Code -ne 0) { throw "模板发行版 $TemplateDistro 无法执行 root 命令：$($templateKernel.Output)" }
if ($templateKernel.Output -notmatch '(?i)WSL2') { throw "模板发行版 $TemplateDistro 不在 WSL2 内核上：$($templateKernel.Output)" }
$templateCodename = (Invoke-Wsl $TemplateDistro @('bash', '-lc', "grep -E '^(ID|VERSION_CODENAME)=' /etc/os-release | cut -d= -f2 | tr '\n' ' '")).Output.Trim()
if (-not $templateCodename) { throw "无法读取模板发行版 $TemplateDistro 的 Debian 版本" }
Write-Host "模板发行版：$TemplateDistro（$templateCodename，内核 $($templateKernel.Output.Trim())）"

New-Item -ItemType Directory -Force $RootfsDir | Out-Null
$templateTar = Join-Path $RootfsDir "$TemplateDistro-template.tar"
if (-not (Test-Path $templateTar)) {
  Write-Host "==> 导出模板 rootfs 到 $templateTar"
  & wsl.exe --export $TemplateDistro $templateTar
  if ($LASTEXITCODE -ne 0 -or -not (Test-Path $templateTar)) { throw "导出 $TemplateDistro 失败" }
  $size = [math]::Round((Get-Item $templateTar).Length / 1MB, 1)
  Write-Host "    模板大小 ${size} MB"
} else {
  Write-Host "==> 复用已有模板 $templateTar"
}

foreach ($d in $Distros) {
  Write-Host "==> $d"
  if ((Get-DistroNames) -contains $d) {
    Write-Host '    已存在，保留并继续配置'
  } else {
    $target = Join-Path $RootfsDir $d
    New-Item -ItemType Directory -Force $target | Out-Null
    & wsl.exe --import $d $target $templateTar --version 2
    if ($LASTEXITCODE -ne 0) { throw "导入 $d 失败" }
  }

  # 发行版身份校验：名称、WSL2、架构、Debian 版本、root 可执行。
  Assert-LabDistro $d
$version = (Invoke-Wsl $d @('bash', '-lc', "env | grep '^WSL_DISTRO_NAME=' | cut -d= -f2")).Output.Trim()
  if ($version -ne $d) { throw "$d 内 WSL_DISTRO_NAME 为 $version，与预期不符" }
  $arch = (Invoke-Wsl $d @('uname', '-m')).Output.Trim()
  if ($arch -ne 'x86_64') { throw "$d 架构为 $arch，预期 x86_64" }
  $kernel = (Invoke-Wsl $d @('uname', '-r')).Output.Trim()
  if ($kernel -notmatch '(?i)WSL2') { throw "$d 未运行在 WSL2 内核：$kernel" }
$codename = (Invoke-Wsl $d @('bash', '-lc', "grep -E '^(ID|VERSION_CODENAME)=' /etc/os-release | cut -d= -f2 | tr '\n' ' '")).Output.Trim()
  if ($codename -ne $templateCodename) { throw "$d 的 Debian 版本为 $codename，与模板 $templateCodename 不一致" }
  $idWho = (Invoke-Wsl $d @('id', '-u')).Output.Trim()
  if ($idWho -ne '0') { throw "$d 无法以 root 执行命令（id -u = $idWho）" }
  Write-Host "    校验通过：WSL2 / $arch / $codename"
}

# 启用 systemd：写 /etc/wsl.conf 后需要重启发行版才生效。
foreach ($d in $Distros) {
  $script = Join-Path $PSScriptRoot 'configure-node.sh'
  $wslScript = Convert-WindowsPathToWsl $script
  Invoke-Wsl $d @('bash', $wslScript, '-o', 'lab', '--enable-systemd') | Out-Null

  # terminate 后必须等到实例真正停止再继续，否则后续命令可能撞上正在关闭的实例。
  & wsl.exe --terminate $d
  if ($LASTEXITCODE -ne 0) { throw "重启 $d 以使 systemd 生效失败" }
  for ($i = 0; $i -lt 30; $i++) {
    $state = (wsl.exe --list --verbose 2>$null | ForEach-Object { $_ -replace "`0", '' }) -join "`n"
    if ($state -notmatch "(?m)^\s*\*?\s*$([regex]::Escape($d))\s+Running") { break }
    Start-Sleep -Milliseconds 500
  }

  Write-Host "==> $d 安装实验依赖"
  $install = Invoke-Wsl $d @('bash', $wslScript, '-o', 'lab', '--install-deps', '--node-version', $NodeVersion)
  if ($install.Code -ne 0) { throw "为 $d 安装实验依赖失败：`n$($install.Output)" }
  Write-Host ($install.Output.Trim() -split "`r?`n" | ForEach-Object { "    $_" }) -Separator "`n"

  # systemd 若在当前 WSL 环境不可用，configure-node.sh 会回退独立 dockerd，
  # 并把实际启动方式写进 capabilities.txt，这里如实记录而不假设一定是 systemd。
  $role = if ($d -eq $Distros[0]) { 'hub' } else { 'agent' }
  $configure = Invoke-Wsl $d @('bash', $wslScript, $d, $role, 'opsd-test-bootstrap-', '--start-docker')
  if ($configure.Code -ne 0) { throw "$d Docker Engine 配置失败：`n$($configure.Output)" }
  Write-Host ($configure.Output.Trim() -split "`r?`n" | ForEach-Object { "    $_" }) -Separator "`n"

  $mode = (Invoke-Wsl $d @('cat', '/var/lib/opsd-lab/docker_start_mode')).Output.Trim()
  if (-not $mode) { throw "$d 未记录 Docker 启动方式" }
  Write-Host "    Docker 启动方式：$mode"
}

Write-Host "实验发行版就绪：$($Distros -join ', ')"
Write-Host '限制：WSL2 共用宿主机内核，不能替代独立 Linux 内核的防火墙生产验收。'
