param(
  [Parameter(Mandatory=$true)][string]$C001Host,
  [Parameter(Mandatory=$true)][string]$C091Host,
  [Parameter(Mandatory=$true)][string]$C101Host,
  [string]$KeyPath,
  [string]$C001KeyPath,
  [string]$C091KeyPath,
  [string]$C101KeyPath,
  [string]$AgentBinary,
  [string]$AdminPasswordFile,
  [string]$TokenDirectory,
  [string]$Image = 'ghcr.io/opsd-labs/opsd:latest',
  [int]$HubConsolePort = 65535,
  [int]$HubAgentPort = 8444,
  [string]$RunId = ('deploy-' + (Get-Date -Format 'yyyyMMdd-HHmmss')),
  [switch]$Execute
)

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
if ([string]::IsNullOrWhiteSpace($C001KeyPath)) { $C001KeyPath = $KeyPath }
if ([string]::IsNullOrWhiteSpace($C091KeyPath)) { $C091KeyPath = $KeyPath }
if ([string]::IsNullOrWhiteSpace($C101KeyPath)) { $C101KeyPath = $KeyPath }
$nodes = [ordered]@{
  c001 = @{ Id = 'C001'; Host = $C001Host; Key = $C001KeyPath }
  c091 = @{ Id = 'C091'; Host = $C091Host; Key = $C091KeyPath }
  c101 = @{ Id = 'C101'; Host = $C101Host; Key = $C101KeyPath }
}
if ($RunId -notmatch '^[A-Za-z0-9][A-Za-z0-9_.-]{2,63}$') { throw 'RunId 格式错误' }
foreach ($node in $nodes.Values) {
  if ([string]::IsNullOrWhiteSpace($node.Key) -or -not (Test-Path -LiteralPath $node.Key -PathType Leaf)) { throw "SSH 私钥不存在：$($node.Key)" }
}
if ($Image -notmatch '^ghcr\.io/opsd-labs/opsd:[A-Za-z0-9_.-]+$') { throw '镜像地址必须是 ghcr.io/opsd-labs/opsd:<tag>' }
foreach ($port in @($HubConsolePort, $HubAgentPort)) { if ($port -lt 1024 -or $port -gt 65535) { throw "端口不在允许范围：$port" } }

$runDir = Join-Path $repo ".data/deploy/$RunId"
New-Item -ItemType Directory -Force -Path $runDir | Out-Null
$preflight = Join-Path $repo 'deploy/remote-preflight.sh'
$stopScript = Join-Path $repo 'deploy/remote-stop-onepanel.sh'
$restoreScript = Join-Path $repo 'deploy/remote-restore-onepanel.sh'
$hubScript = Join-Path $repo 'deploy/remote-hub-deploy.sh'
$agentScript = Join-Path $repo 'deploy/remote-agent-install.sh'
$service = Join-Path $repo 'deploy/opsd-agent.service'

function Select-NodeKey {
  param([string]$HostName)
  foreach ($node in $nodes.Values) { if ($node.Host -eq $HostName) { $script:KeyPath = $node.Key; return } }
  throw "主机未登记：$HostName"
}
function Invoke-SshScript {
  param([string]$HostName, [string]$ScriptPath, [string[]]$Arguments = @())
  Select-NodeKey $HostName
  $sshArgs = @('-o','BatchMode=yes','-o','ConnectTimeout=10','-o','StrictHostKeyChecking=accept-new','-i',$KeyPath,"root@$HostName",'bash','-s','--') + $Arguments
  $text = (Get-Content -LiteralPath $ScriptPath -Raw) -replace "`r`n", "`n"
  $output = $text | & ssh @sshArgs 2>&1
  $code = $LASTEXITCODE
  if ($code -ne 0) { throw "远端命令失败（$HostName，退出码 $code）：$($output -join [Environment]::NewLine)" }
  return ($output -join [Environment]::NewLine)
}
function Copy-Remote {
  param([string]$HostName, [string]$LocalPath, [string]$RemotePath)
  Select-NodeKey $HostName
  $args = @('-o','BatchMode=yes','-o','ConnectTimeout=10','-o','StrictHostKeyChecking=accept-new','-i',$KeyPath,$LocalPath,"root@${HostName}:$RemotePath")
  $temporaryUpload = $null
  if ($LocalPath -match '\.sh$') {
    $temporaryUpload = Join-Path $runDir ((Split-Path $LocalPath -Leaf) + '.lf')
    $content = (Get-Content -LiteralPath $LocalPath -Raw) -replace "`r`n", "`n"
    [System.IO.File]::WriteAllText($temporaryUpload, $content, [System.Text.UTF8Encoding]::new($false))
    $args[8] = $temporaryUpload
  }
  & scp @args 2>&1 | Out-File -FilePath (Join-Path $runDir 'scp.log') -Append -Encoding utf8
  if ($temporaryUpload) { Remove-Item -LiteralPath $temporaryUpload -Force -ErrorAction SilentlyContinue }
  if ($LASTEXITCODE -ne 0) { throw "上传失败（$HostName）：$LocalPath" }
}
function Run-RemoteCommand {
  param([string]$HostName, [string[]]$CommandArgs)
  Select-NodeKey $HostName
  $args = @('-o','BatchMode=yes','-o','ConnectTimeout=10','-o','StrictHostKeyChecking=accept-new','-i',$KeyPath,"root@$HostName") + $CommandArgs
  $output = & ssh @args 2>&1
  if ($LASTEXITCODE -ne 0) { throw "远端命令失败（$HostName）：$($output -join [Environment]::NewLine)" }
  return ($output -join [Environment]::NewLine)
}
Write-Host "运行编号：$RunId"
Write-Host '阶段 1/2：只读预检（不会停止服务）'
foreach ($name in $nodes.Keys) {
  $node = $nodes[$name]
  try {
    $output = Invoke-SshScript $node.Host $preflight @($node.Id, $RunId)
    $output | Set-Content -LiteralPath (Join-Path $runDir "$name-preflight.log") -Encoding utf8
    Write-Host "[$name] 预检通过"
  } catch {
    $_ | Out-String | Set-Content -LiteralPath (Join-Path $runDir "$name-preflight.failed.log") -Encoding utf8
    throw "[$name] 预检失败；未停止任何 1Panel 服务。详见 $runDir"
  }
}
if (-not $Execute) {
  Write-Host '预检已完成。默认不执行停止服务或部署；若确认快照无误，请使用 -Execute。'
  exit 0
}

foreach ($required in @($AgentBinary, $AdminPasswordFile, $TokenDirectory)) {
  if ([string]::IsNullOrWhiteSpace($required)) { throw '-Execute 需要 -AgentBinary、-AdminPasswordFile 和 -TokenDirectory' }
}
if (-not (Test-Path -LiteralPath $AgentBinary -PathType Leaf)) { throw "Agent 二进制不存在：$AgentBinary" }
if (-not (Test-Path -LiteralPath $AdminPasswordFile -PathType Leaf)) { throw "管理员密码文件不存在：$AdminPasswordFile" }
if (-not (Test-Path -LiteralPath $TokenDirectory -PathType Container)) { throw "令牌目录不存在：$TokenDirectory" }
foreach ($name in $nodes.Keys) {
  $token = Join-Path $TokenDirectory "$name.token"
  if (-not (Test-Path -LiteralPath $token -PathType Leaf)) { throw "缺少一次性令牌：$token" }
}

$stopped = @()
try {
  Write-Host '阶段 2/2：停止已识别 1Panel 并部署灰度环境'
  foreach ($name in $nodes.Keys) {
    $node = $nodes[$name]
    $output = Invoke-SshScript $node.Host $stopScript @($RunId,$node.Id)
    $output | Set-Content -LiteralPath (Join-Path $runDir "$name-onepanel-stop.log") -Encoding utf8
    $stopped += $name
  }

  $hubRoot = '/opt/opsd-test'
  Run-RemoteCommand $C091Host @('mkdir','-p',"$hubRoot/data","$hubRoot/backups","$hubRoot/secrets") | Out-Null
  Copy-Remote $C091Host (Join-Path $repo 'compose.yml') "$hubRoot/compose.yml"
  Copy-Remote $C091Host $hubScript '/tmp/opsd-remote-hub-deploy.sh'
  $envFile = @(
    "OPSD_IMAGE=$Image",
    "OPSD_CONSOLE_PORT=$HubConsolePort",
    "OPSD_AGENT_PORT=$HubAgentPort",
    'OPSD_CONSOLE_BIND=0.0.0.0',
    'OPSD_AGENT_BIND=0.0.0.0',
    "OPSD_ORIGIN=https://${C091Host}:$HubConsolePort"
  ) -join "`n"
  $envLocal = Join-Path $runDir 'deployment.env'
  $envFile | Set-Content -LiteralPath $envLocal -Encoding ascii
  Copy-Remote $C091Host $envLocal "$hubRoot/deployment.env"
  Copy-Remote $C091Host $AdminPasswordFile "$hubRoot/secrets/admin-password"
  $hubOutput = Invoke-SshScript $C091Host $hubScript @($hubRoot,$Image)
  $hubOutput | Set-Content -LiteralPath (Join-Path $runDir 'hub-deploy.log') -Encoding utf8

  Select-NodeKey $C091Host
  $caLocal = Join-Path $runDir 'ca.pem'
  $scpArgs = @('-o','BatchMode=yes','-o','ConnectTimeout=10','-o','StrictHostKeyChecking=accept-new','-i',$KeyPath,"root@${C091Host}:$hubRoot/data/pki/ca.pem",$caLocal)
  & scp @scpArgs 2>&1 | Out-File -FilePath (Join-Path $runDir 'scp.log') -Append -Encoding utf8
  if ($LASTEXITCODE -ne 0) { throw '无法取回 Hub CA' }
  $fingerprint = [regex]::Match($hubOutput, 'CA 指纹：([0-9a-fA-F:]+)').Groups[1].Value
  if ([string]::IsNullOrWhiteSpace($fingerprint)) { throw '无法从 Hub 初始化输出解析 CA 指纹' }

  foreach ($name in $nodes.Keys) {
    $node = $nodes[$name]
    $remoteTmp = "/tmp/opsd-$RunId"
    Run-RemoteCommand $node.Host @('mkdir','-p',$remoteTmp) | Out-Null
    Copy-Remote $node.Host $AgentBinary "$remoteTmp/opsd-agent"
    Copy-Remote $node.Host $caLocal "$remoteTmp/ca.pem"
    Copy-Remote $node.Host (Join-Path $TokenDirectory "$name.token") "$remoteTmp/token"
    Copy-Remote $node.Host $service "$remoteTmp/opsd-agent.service"
    Copy-Remote $node.Host $agentScript "$remoteTmp/install.sh"
    $hubUrl = "https://${C091Host}:$HubConsolePort"
    $agentUrl = "wss://${C091Host}:$HubAgentPort"
    $output = Invoke-SshScript $node.Host $agentScript @("$remoteTmp/opsd-agent",$hubUrl,$agentUrl,"$remoteTmp/ca.pem",$fingerprint,"$remoteTmp/token",$node.Id)
    $output | Set-Content -LiteralPath (Join-Path $runDir "$name-agent-install.log") -Encoding utf8
    Run-RemoteCommand $node.Host @('rm','-rf',$remoteTmp) | Out-Null
  }
  Write-Host "灰度部署完成；日志和镜像摘要保存在 $runDir"
} catch {
  Write-Warning "部署失败，开始恢复已停止的 1Panel 服务：$($_.Exception.Message)"
  foreach ($name in $stopped) {
    try { Invoke-SshScript $nodes[$name].Host $restoreScript @($RunId,$nodes[$name].Id) | Set-Content -LiteralPath (Join-Path $runDir "$name-onepanel-restore.log") -Encoding utf8 }
    catch { Write-Warning "[$name] 恢复失败：$($_.Exception.Message)" }
  }
  throw
}
