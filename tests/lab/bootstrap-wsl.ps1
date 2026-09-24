# 创建专用 WSL2 实验发行版并安装本地实验依赖。
param(
  [string[]]$Distros = @('opsd_hub_lab', 'opsd_c001_lab', 'opsd_c052_lab', 'opsd_c061_lab'),
  [string]$Image = 'debian:bookworm',
  [string]$RootfsDir = "$env:USERPROFILE\opsd-wsl-lab"
)
$ErrorActionPreference = 'Stop'

function Assert-LabDistro([string]$Name) {
  if ($Name -notmatch '^opsd_[a-z0-9_]+$') { throw "拒绝操作非实验发行版：$Name" }
}

foreach ($d in $Distros) { Assert-LabDistro $d }
if (-not (Get-Command docker -ErrorAction SilentlyContinue)) { throw '宿主机缺少 Docker CLI；未创建任何 WSL 发行版。' }
$wslStatus = wsl --status 2>&1 | Out-String
if ($LASTEXITCODE -ne 0) { throw "无法读取 WSL 状态；未创建任何发行版。`n$wslStatus" }
$dockerInfo = docker info 2>&1 | Out-String
if ($LASTEXITCODE -ne 0) { throw "Docker Engine 不可用；未创建任何发行版。`n$dockerInfo" }

$probe = "opsd-test-bootstrap-$([guid]::NewGuid().ToString('N'))"
$probeTar = Join-Path ([IO.Path]::GetTempPath()) "$probe.tar"
$probeId = $null
try {
  docker pull $Image 2>&1 | Out-Host
  if ($LASTEXITCODE -ne 0) { throw "拉取 $Image 失败" }
  $probeId = docker create --name $probe $Image 2>&1 | Out-String
  if ($LASTEXITCODE -ne 0) { throw "Docker 镜像创建失败：$probeId" }
  docker export $probeId.Trim() -o $probeTar 2>&1 | Out-Null
  if ($LASTEXITCODE -ne 0 -or -not (Test-Path $probeTar)) { throw 'Docker 镜像导出能力检查失败。' }
} finally {
  if ($probeId) { docker rm -f $probeId.Trim() 2>&1 | Out-Null }
  Remove-Item -LiteralPath $probeTar -Force -ErrorAction SilentlyContinue
}

New-Item -ItemType Directory -Force $RootfsDir | Out-Null
$existing = @((wsl --list --quiet 2>$null) | ForEach-Object { (($_ -replace "`0", '').Trim()) } | Where-Object { $_ })
foreach ($d in $Distros) {
  Write-Host "==> $d"
  if ($existing -contains $d) {
    Write-Host '    已存在，保留并继续配置'
  } else {
    $tar = Join-Path $RootfsDir "$d.tar"
    $target = Join-Path $RootfsDir $d
    if (-not (Test-Path $tar)) {
      $containerId = docker create $Image 2>&1 | Out-String
      if ($LASTEXITCODE -ne 0) { throw "创建 $Image 临时容器失败：$containerId" }
      try {
        docker export $containerId.Trim() -o $tar 2>&1 | Out-Host
        if ($LASTEXITCODE -ne 0) { throw "导出 $Image rootfs 失败" }
      } finally {
        docker rm -f $containerId.Trim() 2>&1 | Out-Null
      }
    }
    New-Item -ItemType Directory -Force $target | Out-Null
    wsl --import $d $target $tar --version 2
    if ($LASTEXITCODE -ne 0) { throw "导入 $d 失败" }
  }

  $kernel = & wsl.exe -d $d -u root -- uname -r 2>&1 | Out-String
  if ($LASTEXITCODE -ne 0 -or $kernel -notmatch '(?i)WSL2') { throw "$d 未运行在 WSL2 内核：$kernel" }
  wsl -d $d -u root -- bash -lc 'set -e; export DEBIAN_FRONTEND=noninteractive; apt-get update -qq; apt-get install -y -qq ca-certificates curl docker.io iproute2 iptables nftables ufw firewalld iputils-ping procps gawk nodejs npm >/dev/null'
  if ($LASTEXITCODE -ne 0) { throw "为 $d 安装实验依赖失败" }
  $scriptPath = & wsl.exe -d $d -u root -- wslpath -a (Join-Path $PSScriptRoot 'configure-node.sh')
  $scriptPath = ($scriptPath | Out-String).Trim()
  if ($LASTEXITCODE -ne 0) { throw '无法将 configure-node.sh 路径转换为 WSL 路径' }
  $role = if ($d -eq 'opsd_hub_lab') { 'hub' } else { 'agent' }
  wsl -d $d -u root -- bash $scriptPath $d $role 'opsd-test-bootstrap-'
  if ($LASTEXITCODE -ne 0) { throw "$d Docker Engine 配置失败" }
}

Write-Host "实验发行版就绪：$($Distros -join ', ')"
Write-Host '限制：WSL2 共用宿主机内核，不能替代独立 Linux 内核的防火墙生产验收。'
