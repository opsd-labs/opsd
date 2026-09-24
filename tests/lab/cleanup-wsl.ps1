# 回收单次运行产生的 opsd-test- 资源；发行版注销必须显式指定。
param(
  [string[]]$Distros = @('opsd_hub_lab', 'opsd_c001_lab', 'opsd_c052_lab', 'opsd_c061_lab'),
  [string]$RunId,
  [switch]$RemoveDistro,
  [string]$RootfsDir = "$env:USERPROFILE\opsd-wsl-lab"
)
$ErrorActionPreference = 'Stop'

foreach ($d in $Distros) {
  if ($d -notmatch '^opsd_[a-z0-9_]+$') { throw "拒绝操作非 opsd_ 实验发行版：$d" }
}
if ($RunId -and $RunId -notmatch '^[a-zA-Z0-9-]+$') { throw 'RunId 仅允许字母、数字和连字符。' }
if (-not $RunId -and -not $RemoveDistro) { throw '清理测试资源必须指定 -RunId；不会按宽泛前缀删除其他运行的资源。' }

$known = @((wsl.exe --list --quiet 2>$null) | ForEach-Object { (($_ -replace "`0", '').Trim()) } | Where-Object { $_ })
$failed = $false
foreach ($d in $Distros) {
  if ($known -notcontains $d) { Write-Host "[SKIP] $d 不存在"; continue }
  Write-Host "==> $d"
  if ($RunId) {
    $prefix = "opsd-test-$RunId-"
    $script = @'
set -euo pipefail
prefix='__PREFIX__'
containers=$(docker ps -a --format '{{.ID}} {{.Names}}' | awk -v prefix="$prefix" 'index($2, prefix) == 1 { print $1 }')
if [[ -n "$containers" ]]; then docker rm -f $containers; fi
volumes=$(docker volume ls -q | awk -v prefix="$prefix" 'index($0, prefix) == 1')
if [[ -n "$volumes" ]]; then docker volume rm $volumes; fi
networks=$(docker network ls --format '{{.ID}} {{.Name}}' | awk -v prefix="$prefix" 'index($2, prefix) == 1 { print $1 }')
if [[ -n "$networks" ]]; then docker network rm $networks; fi
rm -rf -- "/tmp/$prefix"
echo "removed resources with prefix $prefix"
'@
    $script = $script.Replace('__PREFIX__', $prefix)
    $output = & wsl.exe -d $d -u root -- bash -lc $script 2>&1
    if ($LASTEXITCODE -ne 0) {
      $failed = $true
      Write-Host "[FAIL] 清理 $d：$($output | Out-String)"
    } else {
      Write-Host "[PASS] $($output | Out-String)"
    }
  }

  if ($RemoveDistro) {
    Write-Host "将注销专用发行版 $d"
    & wsl.exe --unregister $d
    if ($LASTEXITCODE -ne 0) { $failed = $true; Write-Host "[FAIL] 注销 $d" }
    $target = Join-Path $RootfsDir $d
    $parent = [IO.Path]::GetFullPath((Split-Path $target -Parent)).TrimEnd('\')
    $expectedParent = [IO.Path]::GetFullPath($RootfsDir).TrimEnd('\')
    if ($parent -ne $expectedParent -or (Split-Path $target -Leaf) -ne $d) {
      throw "拒绝删除超出实验目录的路径：$target"
    }
    Remove-Item -LiteralPath $target -Recurse -Force -ErrorAction SilentlyContinue
  }
}
if ($failed) { exit 1 }
Write-Host '清理完成。发行版及其他运行的资源均予以保留。'
