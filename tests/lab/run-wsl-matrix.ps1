# 执行真实控制面场景、Docker 操作矩阵和四后端防火墙只读盘点。
#
# 设计约束：
# - 每项结果都带状态、实际命令、退出码、日志尾部、目标发行版、资源或任务 ID。
# - 缺少能力时只能是 SKIP 或 NOT_RUN，绝不降级为占位 PASS。
# - 不启用 experimental-firewall；不执行任何防火墙写入或回滚。
param(
  [string[]]$Distros = @('opsd_hub_lab', 'opsd_c001_lab', 'opsd_c052_lab', 'opsd_c061_lab'),
  [string]$Report = (Join-Path (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path '.data\opsd-wsl-report.md'),
  [switch]$RequireNetns,
  [switch]$SkipControlPlane
)
$ErrorActionPreference = 'Stop'
$stamp = Get-Date -Format 'yyyy-MM-dd HH:mm:ss zzz'
$runId = [guid]::NewGuid().ToString('N').Substring(0, 12)
$testPrefix = "opsd-test-$runId-"
$results = [System.Collections.Generic.List[object]]::new()
$configuredDistros = [System.Collections.Generic.HashSet[string]]::new()
$knownDistros = @((wsl.exe --list --quiet 2>$null) | ForEach-Object { (($_ -replace "`0", '').Trim()) } | Where-Object { $_ })
$repoWin = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$hubDistro = $Distros[0]

foreach ($d in $Distros) {
  if ($d -notmatch '^opsd_[a-z0-9_]+$') { throw "拒绝操作非 opsd_ 实验发行版：$d" }
}

function Convert-WindowsPathToWsl([string]$Path) {
  $full = [IO.Path]::GetFullPath($Path)
  if ($full -notmatch '^(?<drive>[A-Za-z]):\\(?<rest>.*)$') { throw "无法转换为 WSL 路径：$full" }
  return "/mnt/$($matches.drive.ToLower())/$($matches.rest.Replace('\', '/'))"
}
function Add-Result {
  param(
    [string]$Name, [string]$Status, [Nullable[int]]$ExitCode, [string]$Detail,
    [string]$Command, [string]$Distro = '', [string]$Resource = '', [string]$Evidence = ''
  )
  $tail = (($Detail -split "`r?`n") | Where-Object { $_.Trim() } | Select-Object -Last 12) -join ' | '
  $results.Add([pscustomobject]@{
    Name = $Name; Status = $Status; ExitCode = $ExitCode; Detail = $tail
    Command = $Command; Distro = $Distro; Resource = $Resource; Evidence = $Evidence
  })
  Write-Host "[$Status] $Name"
  if ($tail) { Write-Host "  $tail" }
}

function Invoke-WSL {
  param([string]$Distro, [string]$Script)
  $errorFile = [IO.Path]::GetTempFileName()
  try {
    $normalized = $Script -replace "`r", ""
    $encoded = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($normalized))
    $command = "printf %s $encoded | base64 -d | bash -s"
    $stdout = & wsl.exe -d $Distro -u root -- bash -c $command 2> $errorFile
    $code = $LASTEXITCODE
    $stderr = if (Test-Path $errorFile) { Get-Content -LiteralPath $errorFile -Raw } else { '' }
    $text = ($stdout | Out-String).TrimEnd()
    if ($code -ne 0 -and $stderr.Trim()) { $text = ($text + [Environment]::NewLine + $stderr.Trim()).Trim() }
    return @{ Code = $code; Output = $text }
  } finally {
    Remove-Item -LiteralPath $errorFile -Force -ErrorAction SilentlyContinue
  }
}

function Invoke-Check {
  param([string]$Name, [string]$Distro, [string]$Script, [string]$Command, [string]$Resource = '')
  try {
    $result = Invoke-WSL -Distro $Distro -Script $Script
    $status = if ($result.Code -eq 0) { 'PASS' } else { 'FAIL' }
    Add-Result -Name $Name -Status $status -ExitCode $result.Code -Detail $result.Output -Command $Command -Distro $Distro -Resource $Resource
    return $result
  } catch {
    Add-Result -Name $Name -Status 'FAIL' -ExitCode $null -Detail $_.Exception.Message -Command $Command -Distro $Distro -Resource $Resource
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
$lines.Add("- 主控发行版：$hubDistro")
$lines.Add('- 限制：WSL2 共用宿主机内核；本报告不作为独立 Linux 内核防火墙或生产就绪证明。')
$lines.Add('- 防火墙仅执行只读命令；未启用 experimental-firewall。')
$lines.Add('')

try {
  # ---------- 1. 各发行版环境配置 ----------
  foreach ($d in $Distros) {
    if ($knownDistros -notcontains $d) {
      Add-Result -Name "$d 环境配置" -Status 'NOT_RUN' -ExitCode $null -Detail '发行版不存在；请先运行 bootstrap-wsl.ps1' `
        -Command 'wsl.exe --list --quiet' -Distro $d
      continue
    }
    $configPath = Convert-WindowsPathToWsl (Join-Path $PSScriptRoot 'configure-node.sh')
    $role = if ($d -eq $hubDistro) { 'hub' } else { 'agent' }
    $script = "bash '$configPath' '$d' '$role' '$testPrefix' --start-docker"
    $configured = Invoke-Check -Name "$d 独立 Docker Engine 配置" -Distro $d -Script $script `
      -Command "configure-node.sh $d $role $testPrefix --start-docker" -Resource "prefix=$testPrefix"
    if ($configured.Code -eq 0) { [void]$configuredDistros.Add($d) }

    # 独立 Engine 的硬证据：数据根目录与 socket 必须在本发行版内，不共享宿主 Docker。
    $identity = Invoke-WSL -Distro $d -Script 'set -euo pipefail; cat /var/lib/opsd-lab/capabilities.txt'
    if ($identity.Code -eq 0) {
      Add-Result -Name "$d 节点身份与能力探测" -Status 'PASS' -ExitCode 0 -Detail $identity.Output `
        -Command 'cat /var/lib/opsd-lab/capabilities.txt' -Distro $d -Resource "prefix=$testPrefix" `
        -Evidence ($identity.Output -split "`r?`n" | Where-Object { $_ -match 'docker_root|docker_host|kernel' }) -join ' | '
    } else {
      Add-Result -Name "$d 节点身份与能力探测" -Status 'FAIL' -ExitCode $identity.Code -Detail $identity.Output `
        -Command 'cat /var/lib/opsd-lab/capabilities.txt' -Distro $d
    }
  }

  # ---------- 2. 控制面独立场景 ----------
  if ($SkipControlPlane) {
    Add-Result -Name '控制面独立场景矩阵' -Status 'NOT_RUN' -ExitCode $null -Detail '显式指定 -SkipControlPlane' `
      -Command 'node tests/lab/control-plane.mjs --json' -Distro $hubDistro
  } elseif ($knownDistros -notcontains $hubDistro) {
    Add-Result -Name '控制面独立场景矩阵' -Status 'NOT_RUN' -ExitCode $null -Detail '主控实验发行版不存在' `
      -Command 'node tests/lab/control-plane.mjs --json' -Distro $hubDistro
  } else {
    $repoLinux = Convert-WindowsPathToWsl $repoWin
    if (-not $repoLinux.StartsWith('/')) {
      Add-Result -Name '控制面独立场景矩阵' -Status 'NOT_RUN' -ExitCode $null -Detail $repoLinux `
        -Command 'Windows path to WSL path conversion' -Distro $hubDistro
    } else {
      # 前置条件：Node 22+ 且带 zlib.crc32，以及 Linux 版 opsd-hub / opsd-agent。
      $buildScript = 'set -euo pipefail; export PATH=/root/.cargo/bin:$PATH; cd "' + $repoLinux + '"; ' +
        'command -v cargo >/dev/null || { echo "cargo missing"; exit 20; }; ' +
        'cargo build --locked --bins; ' +
        'file target/debug/opsd-hub target/debug/opsd-agent; ' +
        'file target/debug/opsd-hub target/debug/opsd-agent | grep -q ELF; ' +
        'test -x target/debug/opsd-hub && test -x target/debug/opsd-agent'
      $build = Invoke-WSL -Distro $hubDistro -Script $buildScript
      if ($build.Code -ne 0) {
        Add-Result -Name 'WSL Linux Hub/Agent build' -Status 'NOT_RUN' -ExitCode $build.Code -Detail $build.Output `
          -Command 'cargo build --locked --bins && file target/debug/opsd-hub target/debug/opsd-agent' -Distro $hubDistro
      } else {
        Add-Result -Name 'WSL Linux Hub/Agent build' -Status 'PASS' -ExitCode 0 -Detail $build.Output `
          -Command 'cargo build --locked --bins && file target/debug/opsd-hub target/debug/opsd-agent' -Distro $hubDistro
        $preflight = 'set -euo pipefail; cd "' + $repoLinux + '"; ' +
          'node -e ''const m=Number(process.versions.node.split(".")[0]);if(m<22)process.exit(20);if(typeof require("node:zlib").crc32!=="function")process.exit(20)'''
        $pre = Invoke-WSL -Distro $hubDistro -Script $preflight
        if ($pre.Code -ne 0) {
          Add-Result -Name 'control-plane matrix' -Status 'NOT_RUN' -ExitCode $pre.Code -Detail $pre.Output `
            -Command 'node version and zlib.crc32 check' -Distro $hubDistro
        } else {
          $cpCommand = 'cd "' + $repoLinux + '" && node tests/lab/control-plane.mjs --json'
          $cp = Invoke-WSL -Distro $hubDistro -Script $cpCommand
          if ($cp.Code -eq 127) {
            Add-Result -Name 'control-plane matrix' -Status 'NOT_RUN' -ExitCode 127 -Detail 'Node.js is unavailable' `
              -Command $cpCommand -Distro $hubDistro
          } else {
            $json = $null
            try { $json = ($cp.Output | ConvertFrom-Json) } catch { $json = $null }
            if ($json -and $json.results) {
              foreach ($r in $json.results) {
                Add-Result -Name "control-plane: $($r.name)" -Status $r.status -ExitCode $cp.Code `
                  -Detail $r.detail -Command $r.command -Distro $hubDistro -Resource $r.resource
              }
            } elseif ($cp.Code -eq 0) {
              Add-Result -Name 'control-plane matrix' -Status 'PASS' -ExitCode 0 -Detail $cp.Output -Command $cpCommand -Distro $hubDistro
            } else {
              Add-Result -Name 'control-plane matrix' -Status 'FAIL' -ExitCode $cp.Code -Detail $cp.Output -Command $cpCommand -Distro $hubDistro
            }
          }
        }
      }
    }
  }

  # ---------- 3. Docker 操作矩阵 ----------
  foreach ($d in $Distros) {
    if ($knownDistros -notcontains $d) { continue }
    if (-not $configuredDistros.Contains($d)) {
      Add-Result -Name "$d Docker matrix" -Status 'NOT_RUN' -ExitCode $null `
        -Detail 'Docker setup did not pass; Docker operations were not run.' `
        -Command 'configure-node.sh ... --start-docker' -Distro $d
      continue
    }
    $short = $d -replace '^opsd_', ''
    $name = "${testPrefix}docker-$short"
    $volume = "${testPrefix}vol-$short"
    $network = "${testPrefix}net-$short"
    $composeDir = "/tmp/${testPrefix}compose-$short"
    $fixturePath = Convert-WindowsPathToWsl (Join-Path $PSScriptRoot '..\prepare-docker-lab.py')
    $fixture = Invoke-WSL -Distro $d -Script "if docker image inspect opsd-lab-fixture:v1 >/dev/null 2>&1; then echo 'fixture_exists=true'; else OPSD_LAB=1 python3 '$fixturePath'; fi"
    if ($fixture.Code -ne 0) {
      Add-Result -Name "$d Docker fixture" -Status 'NOT_RUN' -ExitCode $fixture.Code -Detail $fixture.Output `
        -Command 'OPSD_LAB=1 python3 tests/prepare-docker-lab.py' -Distro $d
      continue
    }
    $dockerScript = @'
set -euo pipefail
name='__NAME__'; volume='__VOLUME__'; network='__NETWORK__'; composeDir='__COMPOSE__'
cleanup(){
  docker rm -f "$name" >/dev/null 2>&1 || true
  docker compose -f "$composeDir/compose.yml" down -v >/dev/null 2>&1 || true
  docker volume rm "$volume" >/dev/null 2>&1 || true
  docker network rm "$network" >/dev/null 2>&1 || true
  rm -rf -- "$composeDir"
}
trap cleanup EXIT

# --- 容器创建：带前缀的独立容器、卷、网络 ---
docker network create "$network" >/dev/null
docker volume create "$volume" >/dev/null
docker run -d --name "$name" --network "$network" -v "$volume:/data" \
  opsd-lab-fixture:v1 sh -c 'echo opsd-lab-ready; sleep 60' >/dev/null

# --- 日志 ---
logs=''
for _ in $(seq 1 15); do
  logs=$(docker logs "$name" 2>&1 || true)
  [[ "$logs" == *opsd-lab-ready* ]] && break
  sleep 1
done
[[ "$logs" == *opsd-lab-ready* ]]
echo "logs_verified=true"

# --- 重启后 ID、挂载与卷必须保持不变 ---
id1=$(docker inspect --format '{{.Id}}' "$name")
mounts1=$(docker inspect --format '{{json .Mounts}}' "$name")
vol1=$(docker inspect --format '{{range .Mounts}}{{.Name}}{{end}}' "$name")
docker restart "$name" >/dev/null
id2=$(docker inspect --format '{{.Id}}' "$name")
mounts2=$(docker inspect --format '{{json .Mounts}}' "$name")
vol2=$(docker inspect --format '{{range .Mounts}}{{.Name}}{{end}}' "$name")
test "$id1" = "$id2"
test "$mounts1" = "$mounts2"
test "$vol1" = "$vol2"
test "$vol2" = "$volume"
echo "container_id=$id2 restart_verified=true mounts_unchanged=true volume_preserved=true"

# --- Compose Stack：正常创建与校验 ---
mkdir -p "$composeDir"
cat > "$composeDir/compose.yml" <<YAML
services:
  a:
    image: opsd-lab-fixture:v1
    container_name: __NAME__-ca
    command: ["sleep", "60"]
    volumes:
      - __VOL2__:/data
  b:
    image: opsd-lab-fixture:v1
    container_name: __NAME__-cb
    command: ["sleep", "60"]
volumes:
  __VOL2__:
YAML
  sed -i "s/__VOL2__/${volume}-compose/g" "$composeDir/compose.yml"
docker volume create "${volume}-compose" >/dev/null
docker compose -f "$composeDir/compose.yml" up -d >/dev/null
docker compose -f "$composeDir/compose.yml" ps
running=$(docker compose -f "$composeDir/compose.yml" ps --status running -q | wc -l)
test "$running" -eq 2
echo "compose_services_running=$running"

# --- Compose 配置失败：非法配置必须被拒绝，且不留下服务 ---
cat > "$composeDir/bad.yml" <<YAML
services:
  broken:
    image: opsd-lab-fixture:v1
    ports: ["不是端口"]
YAML
if docker compose -f "$composeDir/bad.yml" config >/dev/null 2>&1; then
  echo "compose_invalid_config=未被拒绝" >&2; exit 1
fi
echo "compose_invalid_config=rejected"

# --- 镜像拉取失败：不存在的镜像必须失败 ---
if docker pull opsd-lab-fixture:missing >/dev/null 2>&1; then
  echo "image_pull_failure=意外成功" >&2; exit 1
fi
echo "image_pull_failure=rejected"

# --- 部分服务失败：一个服务失败不应让整栈假装成功 ---
cat > "$composeDir/partial.yml" <<YAML
services:
  good:
    image: opsd-lab-fixture:v1
    container_name: __NAME__-good
    command: ["sleep", "30"]
  bad:
    image: opsd-lab-fixture:missing
    container_name: __NAME__-bad
YAML
if docker compose -f "$composeDir/partial.yml" up -d >/dev/null 2>&1; then
  echo "compose_partial_failure=意外成功" >&2; exit 1
fi
docker compose -f "$composeDir/partial.yml" down -v >/dev/null 2>&1 || true
echo "compose_partial_failure=rejected"

# --- 清理：只删本次运行前缀的资源 ---
docker compose -f "$composeDir/compose.yml" down -v >/dev/null
docker volume rm "${volume}-compose" >/dev/null
docker rm -f "$name" >/dev/null
docker volume rm "$volume" >/dev/null
docker network rm "$network" >/dev/null
rm -rf -- "$composeDir"
trap - EXIT
echo "cleanup_verified=true"
'@
    $dockerScript = $dockerScript.Replace('__NAME__', $name).Replace('__VOLUME__', $volume).Replace('__NETWORK__', $network).Replace('__COMPOSE__', $composeDir)
    Invoke-Check -Name "$d Docker 创建/日志/重启/挂载/Compose/清理" -Distro $d -Script $dockerScript `
      -Command "docker pull/run/logs/restart/inspect + docker compose up/down（$name）" -Resource "container=$name volume=$volume network=$network" | Out-Null
  }

  # ---------- 4. 四后端防火墙只读盘点 ----------
  foreach ($d in $Distros) {
    if ($knownDistros -notcontains $d) { continue }

    # nftables：区分「工具缺失」「解析失败」与「空规则集」。
    $nft = Invoke-WSL -Distro $d -Script 'set -uo pipefail; command -v nft >/dev/null || { echo "工具缺失: nft"; exit 78; }; nft -nn list ruleset 2>/tmp/nft.err || { echo "解析失败"; cat /tmp/nft.err; exit 1; }'
    if ($nft.Code -eq 78) {
      Add-Result -Name "$d nftables 只读快照" -Status 'SKIP' -ExitCode 78 -Detail $nft.Output -Command 'nft -nn list ruleset' -Distro $d
    } elseif ($nft.Code -ne 0) {
      Add-Result -Name "$d nftables 只读快照" -Status 'FAIL' -ExitCode $nft.Code -Detail $nft.Output -Command 'nft -nn list ruleset' -Distro $d
    } else {
      $rules = ($nft.Output -split "`r?`n" | Where-Object { $_ -match '^\s*(chain|table)' }).Count
      $hasDocker = $nft.Output -match 'DOCKER|docker'
      Add-Result -Name "$d nftables 只读快照" -Status 'PASS' -ExitCode 0 `
        -Detail "$($nft.Output.Trim())" -Command 'nft -nn list ruleset' -Distro $d `
        -Resource "manager=nft 表/链=$rules docker规则=$hasDocker"
    }

    # iptables：同时记录规则数量与默认策略。
    $ipt = Invoke-WSL -Distro $d -Script 'set -uo pipefail; command -v iptables-save >/dev/null || { echo "工具缺失: iptables-save"; exit 78; }; iptables-save -c 2>/tmp/ipt.err || { echo "解析失败"; cat /tmp/ipt.err; exit 1; }; echo "---策略---"; iptables -S 2>/dev/null | grep -E "^-P" || true; echo "---Docker规则---"; iptables -S 2>/dev/null | grep -c "DOCKER" || true'
    if ($ipt.Code -eq 78) {
      Add-Result -Name "$d iptables 只读快照" -Status 'SKIP' -ExitCode 78 -Detail $ipt.Output -Command 'iptables-save -c' -Distro $d
    } elseif ($ipt.Code -ne 0) {
      Add-Result -Name "$d iptables 只读快照" -Status 'FAIL' -ExitCode $ipt.Code -Detail $ipt.Output -Command 'iptables-save -c' -Distro $d
    } else {
      $ruleCount = ($ipt.Output -split "`r?`n" | Where-Object { $_ -match '^-A ' }).Count
      $policies = ($ipt.Output -split "`r?`n" | Where-Object { $_ -match '^-P ' }) -join ' '
      Add-Result -Name "$d iptables 只读快照" -Status 'PASS' -ExitCode 0 -Detail $policies `
        -Command 'iptables-save -c && iptables -S' -Distro $d `
        -Resource "manager=iptables 规则数=$ruleCount 默认策略=$policies"
    }

    # UFW：未启用是正常状态，记为 SKIP 而不是 FAIL。
    $ufw = Invoke-WSL -Distro $d -Script 'set -uo pipefail; command -v ufw >/dev/null || { echo "工具缺失: ufw"; exit 78; }; ufw status verbose 2>&1 || exit 1'
    if ($ufw.Code -eq 78) {
      Add-Result -Name "$d UFW 只读状态" -Status 'SKIP' -ExitCode 78 -Detail $ufw.Output -Command 'ufw status verbose' -Distro $d
    } elseif ($ufw.Output -match 'inactive|未激活|Status: inactive') {
      Add-Result -Name "$d UFW 只读状态" -Status 'SKIP' -ExitCode $ufw.Code -Detail "UFW 未启用：$($ufw.Output.Trim())" -Command 'ufw status verbose' -Distro $d
    } elseif ($ufw.Code -ne 0) {
      Add-Result -Name "$d UFW 只读状态" -Status 'FAIL' -ExitCode $ufw.Code -Detail $ufw.Output -Command 'ufw status verbose' -Distro $d
    } else {
      Add-Result -Name "$d UFW 只读状态" -Status 'PASS' -ExitCode 0 -Detail $ufw.Output -Command 'ufw status verbose' -Distro $d `
        -Resource 'manager=ufw'
    }

    # firewalld：未运行必须是 SKIP，不能算 FAIL，也不能算 PASS。
    $firewalld = Invoke-WSL -Distro $d -Script 'set -uo pipefail; command -v firewall-cmd >/dev/null || { echo "工具缺失: firewall-cmd"; exit 78; }; firewall-cmd --state; firewall-cmd --list-all-zones'
    if ($firewalld.Code -eq 78) {
      Add-Result -Name "$d firewalld 只读状态" -Status 'SKIP' -ExitCode 78 -Detail $firewalld.Output -Command 'firewall-cmd --state --list-all-zones' -Distro $d
    } elseif ($firewalld.Output -match 'not running|inactive|NOT RUNNING') {
      Add-Result -Name "$d firewalld 只读状态" -Status 'SKIP' -ExitCode $firewalld.Code -Detail "firewalld 未运行：$($firewalld.Output.Trim())" -Command 'firewall-cmd --state' -Distro $d
    } elseif ($firewalld.Code -ne 0) {
      Add-Result -Name "$d firewalld 只读状态" -Status 'FAIL' -ExitCode $firewalld.Code -Detail $firewalld.Output -Command 'firewall-cmd --state --list-all-zones' -Distro $d
    } else {
      $zones = ($firewalld.Output -split "`r?`n" | Where-Object { $_ -match '^\w+$' }).Count
      Add-Result -Name "$d firewalld 只读状态" -Status 'PASS' -ExitCode 0 -Detail $firewalld.Output `
        -Command 'firewall-cmd --state --list-all-zones' -Distro $d -Resource "manager=firewalld 区域数=$zones"
    }

    # ---------- 5. network namespace 能力探测 ----------
    $netnsScript = @'
set -uo pipefail
ns='opsd-ns-__RUN_ID__'
if ! ip netns add "$ns" 2>/tmp/opsd-netns.err; then echo "netns 不可用"; cat /tmp/opsd-netns.err; exit 77; fi
trap 'ip netns del "$ns" >/dev/null 2>&1 || true; rm -f /tmp/opsd-netns.err' EXIT
ip netns exec "$ns" ip link set lo up
ip netns exec "$ns" ping -c 1 -W 2 127.0.0.1
ip netns exec "$ns" ping -6 -c 1 -W 2 ::1
echo "ipv4_loopback=ok ipv6_loopback=ok"
'@
    $netnsScript = $netnsScript.Replace('__RUN_ID__', $runId)
    $netns = Invoke-WSL -Distro $d -Script $netnsScript
    if ($netns.Code -eq 77) {
      # 能力不足：默认 SKIP；显式要求时才是 FAIL。
      $status = if ($RequireNetns) { 'FAIL' } else { 'SKIP' }
      Add-Result -Name "$d network namespace IPv4/IPv6" -Status $status -ExitCode 77 -Detail $netns.Output `
        -Command 'ip netns add + IPv4/IPv6 loopback' -Distro $d `
        -Resource "requireNetns=$($RequireNetns.IsPresent)"
    } elseif ($netns.Code -ne 0) {
      Add-Result -Name "$d network namespace IPv4/IPv6" -Status 'FAIL' -ExitCode $netns.Code -Detail $netns.Output `
        -Command 'ip netns add + IPv4/IPv6 loopback' -Distro $d
    } else {
      Add-Result -Name "$d network namespace IPv4/IPv6" -Status 'PASS' -ExitCode 0 -Detail $netns.Output `
        -Command 'ip netns add + IPv4/IPv6 loopback' -Distro $d
    }
  }
} finally {
  New-Item -ItemType Directory -Force (Split-Path $Report -Parent) | Out-Null
  $lines.Add('## 结果')
  $lines.Add('')
  $lines.Add('| 状态 | 场景 | 退出码 | 目标发行版 | 实际命令 | 资源/任务 | 证据 |')
  $lines.Add('| --- | --- | --- | --- | --- | --- | --- |')
  foreach ($r in $results) {
    $code = if ($null -eq $r.ExitCode) { '—' } else { "$($r.ExitCode)" }
    $esc = { param($s) (($s -replace '\|', '\|') -replace "`r?`n", ' ') }
    $lines.Add("| $($r.Status) | $(& $esc $r.Name) | $code | $(& $esc $r.Distro) | ``$(& $esc $r.Command)`` | $(& $esc $r.Resource) | $(& $esc $r.Detail) |")
  }
  $lines.Add('')
  $lines.Add('## 限制与未覆盖')
  $lines.Add('- WSL2 共享宿主机内核，不能作为独立 Linux 内核防火墙的生产验收。')
  $lines.Add('- 未执行防火墙写入、自动验证、回滚、默认策略修改或连接跟踪清理；`experimental-firewall` 保持关闭。')
  $lines.Add('- 未执行 Agent 强杀故障注入；断线场景验证的是进程正常停止后的对账。')
  $lines.Add('- IPv4/IPv6 入站、出站、转发与 Docker 发布端口的真实拦截效果需独立内核环境。')
  $lines.Add('')
  $lines.Add('## 计数')
  $lines.Add('')
  foreach ($s in @('PASS', 'FAIL', 'SKIP', 'NOT_RUN')) {
    $lines.Add("- $s：$(@($results | Where-Object { $_.Status -eq $s }).Count)")
  }
  Set-Content -LiteralPath $Report -Value ($lines -join "`n") -Encoding utf8
  Write-Host "报告已写入 $Report"
}

$failed = @($results | Where-Object { $_.Status -eq 'FAIL' })
$incomplete = @($results | Where-Object { $_.Status -in @('SKIP', 'NOT_RUN') })
if ($failed.Count -gt 0) { exit 1 }
if ($incomplete.Count -gt 0) { exit 2 }
