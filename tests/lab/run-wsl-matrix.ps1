# 执行真实控制面集成、Docker smoke 和防火墙只读盘点。
param(
  [string[]]$Distros = @('opsd_hub_lab', 'opsd_c001_lab', 'opsd_c052_lab', 'opsd_c061_lab'),
  [string]$Report = (Join-Path (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path '.data\opsd-wsl-report.md'),
  [switch]$RequireNetns
)
$ErrorActionPreference = 'Stop'
$stamp = Get-Date -Format 'yyyy-MM-dd HH:mm:ss zzz'
$runId = [guid]::NewGuid().ToString('N').Substring(0, 12)
$testPrefix = "opsd-test-$runId-"
$results = [System.Collections.Generic.List[object]]::new()
$knownDistros = @((wsl.exe --list --quiet 2>$null) | ForEach-Object { (($_ -replace "`0", '').Trim()) } | Where-Object { $_ })
$repoWin = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$repoLinuxOutput = & wsl.exe -d $Distros[0] -u root -- wslpath -a $repoWin 2>&1
$repoLinux = ($repoLinuxOutput | Out-String).Trim()
$repoPathCode = $LASTEXITCODE

foreach ($d in $Distros) {
  if ($d -notmatch '^opsd_[a-z0-9_]+$') { throw "拒绝操作非 opsd_ 实验发行版：$d" }
}

function Add-Result([string]$Name, [string]$Status, [Nullable[int]]$ExitCode, [string]$Detail, [string]$Command) {
  $tail = (($Detail -split "`r?`n") | Select-Object -Last 12) -join ' | '
  $results.Add([pscustomobject]@{ Name = $Name; Status = $Status; ExitCode = $ExitCode; Detail = $tail; Command = $Command })
  Write-Host "[$Status] $Name"
  if ($tail) { Write-Host "  $tail" }
}

function Invoke-WSL([string]$Distro, [string]$Script) {
  $output = & wsl.exe -d $Distro -u root -- bash -lc $Script 2>&1
  $code = $LASTEXITCODE
  return @{ Code = $code; Output = ($output | Out-String) }
}

function Invoke-Check([string]$Name, [string]$Distro, [string]$Script, [string]$Command) {
  try {
    $result = Invoke-WSL $Distro $Script
    $status = if ($result.Code -eq 0) { 'PASS' } else { 'FAIL' }
    Add-Result $Name $status $result.Code $result.Output $Command
    return $result
  } catch {
    Add-Result $Name 'FAIL' $null $_.Exception.Message $Command
    return @{ Code = -1; Output = $_.Exception.Message }
  }
}

$lines = [System.Collections.Generic.List[string]]::new()
$lines.Add('# opsd WSL 实验报告')
$lines.Add('')
$lines.Add("- 时间：$stamp")
$lines.Add("- 运行标识：$runId")
$lines.Add("- 资源前缀：$testPrefix")
$lines.Add("- 发行版：$($Distros -join ', ')")
$lines.Add('- 限制：WSL2 共用宿主机内核；不作为独立 Linux 内核防火墙或生产就绪证明。')
$lines.Add('- 防火墙仅执行只读命令；不启用 experimental-firewall。')
$lines.Add('')

try {
  foreach ($d in $Distros) {
    if ($knownDistros -notcontains $d) {
      Add-Result "$d 环境配置" 'NOT_RUN' $null '发行版不存在；请先运行 bootstrap-wsl.ps1' 'wsl.exe --list --quiet'
      continue
    }
    $configPath = (Join-Path $PSScriptRoot 'configure-node.sh')
    $pathResult = & wsl.exe -d $d -u root -- wslpath -a $configPath 2>&1
    $pathCode = $LASTEXITCODE
    if ($pathCode -ne 0) {
      Add-Result "$d 环境配置" 'FAIL' $pathCode ($pathResult | Out-String) 'wslpath configure-node.sh'
      continue
    }
    $role = if ($d -eq 'opsd_hub_lab') { 'hub' } else { 'agent' }
    Invoke-Check "$d Docker Engine 配置" $d "bash '$($pathResult.ToString().Trim())' '$d' '$role' '$testPrefix'" 'configure-node.sh' | Out-Null
  }

  if ($knownDistros -notcontains $Distros[0]) {
    Add-Result '控制面注册/mTLS/任务/恢复集成' 'NOT_RUN' $null '主控实验发行版不存在' 'node tests/integration.mjs'
  } elseif ($repoPathCode -ne 0 -or -not $repoLinux.StartsWith('/')) {
    Add-Result '控制面注册/mTLS/任务/恢复集成' 'NOT_RUN' $repoPathCode $repoLinux 'wslpath -a <仓库路径>'
  } else {
    $controlCommand = 'cd "' + $repoLinux + '" && node -e ''if(Number(process.versions.node.split(".")[0])<22||!require("node:zlib").crc32)process.exit(20)'' || exit $?; test -x target/debug/opsd-hub || { echo Linux opsd-hub 缺失; exit 21; }; test -x target/debug/opsd-agent || { echo Linux opsd-agent 缺失; exit 22; }; node tests/integration.mjs'
    $controlResult = Invoke-WSL $Distros[0] $controlCommand
    if ($controlResult.Code -eq 20) {
      Add-Result '控制面注册/mTLS/任务/恢复集成' 'NOT_RUN' 20 '需要 Node.js 22+，并包含 zlib.crc32' $controlCommand
    } elseif ($controlResult.Code -in @(21, 22)) {
      Add-Result '控制面注册/mTLS/任务/恢复集成' 'NOT_RUN' $controlResult.Code '缺少可执行的 Linux opsd-hub/opsd-agent；请先在 WSL 构建' $controlCommand
    } elseif ($controlResult.Code -eq 127) {
      Add-Result '控制面注册/mTLS/任务/恢复集成' 'NOT_RUN' 127 '发行版缺少 Node.js' $controlCommand
    } else {
      $status = if ($controlResult.Code -eq 0) { 'PASS' } else { 'FAIL' }
      Add-Result '控制面注册/mTLS/任务/恢复集成' $status $controlResult.Code $controlResult.Output $controlCommand
    }
  }

  foreach ($d in $Distros) {
    if ($knownDistros -notcontains $d) { continue }
    $short = $d -replace '^opsd_', ''
    $name = "${testPrefix}docker-$short"
    $dockerScript = @'
set -euo pipefail
name='__NAME__'
cleanup(){ docker rm -f "$name" >/dev/null 2>&1 || true; }
trap cleanup EXIT
docker pull alpine:3.20 >/dev/null
docker run -d --name "$name" alpine:3.20 sh -c 'echo opsd-lab-ready; sleep 30' >/dev/null
logs=''
for _ in $(seq 1 10); do
  logs=$(docker logs "$name" 2>&1 || true)
  [[ "$logs" == *opsd-lab-ready* ]] && break
  sleep 1
done
[[ "$logs" == *opsd-lab-ready* ]]
id1=$(docker inspect --format '{{.Id}}' "$name")
docker restart "$name" >/dev/null
id2=$(docker inspect --format '{{.Id}}' "$name")
test "$id1" = "$id2"
echo "container_id=$id2 logs_verified=true restart_verified=true"
'@
    $dockerScript = $dockerScript.Replace('__NAME__', $name)
    Invoke-Check "$d Docker 创建/日志/重启/清理" $d $dockerScript "docker pull/run/logs/restart/rm ($name)" | Out-Null
  }

  foreach ($d in $Distros) {
    if ($knownDistros -notcontains $d) { continue }
    Invoke-Check "$d nftables 只读快照" $d 'set -euo pipefail; command -v nft; nft -nn list ruleset' 'nft -nn list ruleset' | Out-Null
    Invoke-Check "$d iptables 只读快照" $d 'set -euo pipefail; command -v iptables-save; iptables-save -c' 'iptables-save -c' | Out-Null
    Invoke-Check "$d UFW 只读状态" $d 'set -euo pipefail; command -v ufw; ufw status verbose' 'ufw status verbose' | Out-Null
    $firewalld = Invoke-WSL $d 'set -euo pipefail; command -v firewall-cmd; firewall-cmd --state; firewall-cmd --list-all-zones'
    if ($firewalld.Code -eq 0) {
      Add-Result "$d firewalld 只读状态" 'PASS' $firewalld.Code $firewalld.Output 'firewall-cmd --state --list-all-zones'
    } elseif ($firewalld.Output -match 'not running|inactive|NOT RUNNING') {
      Add-Result "$d firewalld 只读状态" 'SKIP' $firewalld.Code $firewalld.Output 'firewall-cmd --state'
    } else {
      Add-Result "$d firewalld 只读状态" 'FAIL' $firewalld.Code $firewalld.Output 'firewall-cmd --state --list-all-zones'
    }

    $netnsScript = @'
set -euo pipefail
ns='opsd-ns-__RUN_ID__'
if ! ip netns add "$ns" 2>/tmp/opsd-netns.err; then cat /tmp/opsd-netns.err; exit 77; fi
trap 'ip netns del "$ns" >/dev/null 2>&1 || true; rm -f /tmp/opsd-netns.err' EXIT
ip netns exec "$ns" ip link set lo up
ip netns exec "$ns" ping -c 1 -W 1 127.0.0.1
ip netns exec "$ns" ping -6 -c 1 -W 1 ::1
echo ipv4_loopback=ok ipv6_loopback=ok
'@
    $netnsScript = $netnsScript.Replace('__RUN_ID__', $runId)
    $netns = Invoke-WSL $d $netnsScript
    if ($netns.Code -eq 77 -and -not $RequireNetns) {
      Add-Result "$d network namespace IPv4/IPv6" 'SKIP' $netns.Code $netns.Output 'ip netns add + IPv4/IPv6 loopback'
    } elseif ($netns.Code -ne 0) {
      Add-Result "$d network namespace IPv4/IPv6" 'FAIL' $netns.Code $netns.Output 'ip netns add + IPv4/IPv6 loopback'
    } else {
      Add-Result "$d network namespace IPv4/IPv6" 'PASS' $netns.Code $netns.Output 'ip netns add + IPv4/IPv6 loopback'
    }
  }
} finally {
  New-Item -ItemType Directory -Force (Split-Path $Report -Parent) | Out-Null
  $lines.Add('## 结果')
  $lines.Add('')
  foreach ($r in $results) {
    $code = if ($null -eq $r.ExitCode) { '无' } else { "$($r.ExitCode)" }
    $lines.Add("- [$($r.Status)] $($r.Name)；退出码：$code；命令：``$($r.Command)``；输出：$($r.Detail)")
  }
  $lines.Add('')
  $lines.Add('## 限制与未覆盖')
  $lines.Add('- 未执行防火墙写入、自动验证、回滚、连接跟踪清理或 Agent 强杀故障注入。')
  $lines.Add('- 控制面场景由现有 integration.mjs 覆盖；断线对账仍需单独场景验证。')
  $lines.Add('- Docker Engine 在各发行版独立运行；镜像拉取依赖实验网络可用。')
  Set-Content -LiteralPath $Report -Value ($lines -join "`n") -Encoding utf8
  Write-Host "报告已写入 $Report"
}

$failed = @($results | Where-Object { $_.Status -eq 'FAIL' })
$incomplete = @($results | Where-Object { $_.Status -in @('SKIP', 'NOT_RUN') })
if ($failed.Count -gt 0) { exit 1 }
if ($incomplete.Count -gt 0) { exit 2 }
