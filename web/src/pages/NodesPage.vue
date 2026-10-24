<script setup lang="ts">
/**
 * NodesPage — 单节点深度视图
 * 指标概览（后续扩展为曲线图 + 文件管理 + 宿主机 Shell）
 * MetricsOverviewNode.sample = MetricsSample | null
 */
import { computed, ref, nextTick } from 'vue'
import type { DeepReadonly } from 'vue'
import { workspace } from '../state/workspace'
import { apiPost, apiDelete, apiPut } from '../api/client'
import type { components } from '../api/generated'
import { formatPercent, formatBytes, formatUptime, formatUnixTime } from '../utils/format'
import Status from '../components/ui/Status.vue'
import Button from '../components/ui/Button.vue'
import Badge from '../components/ui/Badge.vue'
import Drawer from '../components/layout/Drawer.vue'

type EnrollmentToken = components['schemas']['EnrollmentToken']
type Registration = components['schemas']['Registration']
const enrollmentOpen = ref(false)
const enrollmentBusy = ref(false)
const enrollmentError = ref('')
const enrollment = ref<EnrollmentToken | null>(null)
const downloaded = ref(false)
const copyMessage = ref('')
const commandField = ref<HTMLTextAreaElement | null>(null)
const revoking = ref('')
const draft = ref({ name: '', mode: 'host' as Registration['install_mode'] })
const pending = computed(() => workspace.state.enrollments)
const registeredEntry = computed(() => workspace.state.nodeEntries.find(e => e.node.id === enrollment.value?.node_id))
const expiredEntry = computed(() => pending.value.find(p => p.node_id === enrollment.value?.node_id && p.status === 'expired'))
const oneClickCommand = computed(() => {
  const script = enrollment.value?.installation?.script
  if (!script) return ''
  const bytes = new TextEncoder().encode(script)
  const encoded = btoa(Array.from(bytes, b => String.fromCharCode(b)).join(''))
  return `printf '%s' '${encoded}' | base64 --decode | bash`
})

async function openEnrollment() {
  draft.value = { name: '', mode: 'host' }
  enrollment.value = null
  enrollmentError.value = ''
  downloaded.value = false
  copyMessage.value = ''
  enrollmentOpen.value = true
  await nextTick()
  document.getElementById('new-node-name')?.focus()
}

function closeEnrollment() {
  if (enrollmentBusy.value) return
  if (enrollment.value && !downloaded.value && !registeredEntry.value && !expiredEntry.value && !confirm('安装信息尚未复制或下载，关闭后无法重新获取。确定关闭？')) return
  enrollmentOpen.value = false
  enrollment.value = null
}

async function createEnrollment() {
  enrollmentBusy.value = true
  enrollmentError.value = ''
  try {
    const registration: Registration = {
      name: draft.value.name.trim(),
      install_mode: draft.value.mode,
    }
    enrollment.value = await apiPost<EnrollmentToken>('/api/v1/enrollment-tokens', registration)
    await workspace.refreshEnrollments()
  } catch (e: unknown) {
    enrollmentError.value = e instanceof Error ? e.message : '添加节点失败'
  } finally { enrollmentBusy.value = false }
}

function downloadFile(content: string, filename: string, type: string) {
  const url = URL.createObjectURL(new Blob([content], { type }))
  const a = document.createElement('a')
  a.href = url
  a.download = filename
  a.click()
  URL.revokeObjectURL(url)
  downloaded.value = true
}

function downloadToken() {
  if (enrollment.value) downloadFile(enrollment.value.token + '\n', 'opsd-token', 'text/plain')
}

function downloadScript() {
  if (enrollment.value?.installation) downloadFile(enrollment.value.installation.script, 'opsd-install.sh', 'text/x-shellscript')
}

async function copyInstallCommand() {
  try {
    await navigator.clipboard.writeText(oneClickCommand.value)
    downloaded.value = true
    copyMessage.value = '已复制，请在目标机的 root Bash 终端执行。'
    enrollmentError.value = ''
  } catch {
    commandField.value?.focus()
    commandField.value?.select()
    enrollmentError.value = '自动复制失败，请复制下方已选中的安装命令，或下载安装脚本。'
  }
}

async function regenerateEnrollment(p: DeepReadonly<components['schemas']['EnrollmentSummary']>) {
  await openEnrollment()
  draft.value = { name: p.name, mode: p.install_mode }
  enrollmentBusy.value = true
  try {
    await apiDelete(`/api/v1/enrollment-tokens/${encodeURIComponent(p.node_id)}`)
  } catch (e: unknown) {
    enrollmentError.value = e instanceof Error ? e.message : '取消旧注册失败，请先核实节点状态'
    await workspace.refreshEnrollments().catch(() => {})
    enrollmentOpen.value = false
    return
  } finally { enrollmentBusy.value = false }
  await createEnrollment()
}

async function revokeEnrollment(id: string) {
  if (!confirm('取消后该注册令牌立即失效，确定取消？')) return
  revoking.value = id
  enrollmentError.value = ''
  try {
    await apiDelete(`/api/v1/enrollment-tokens/${encodeURIComponent(id)}`)
    if (enrollment.value?.node_id === id) enrollment.value = null
    await workspace.refreshEnrollments()
  } catch (e: unknown) {
    enrollmentError.value = e instanceof Error ? e.message : '取消注册失败'
  } finally { revoking.value = '' }
}

// 安装信息来自正式响应，按 Shell 单参数转义；令牌只通过文件传入。
function shellQuote(value: string): string { return "'" + value.replace(/'/g, "'\\''") + "'" }
const installCommand = computed(() => {
  const e = enrollment.value
  if (!e) return ''
  const hub = shellQuote(window.location.origin)
  const agentUrl = shellQuote(e.agent_url)
  const fingerprint = shellQuote(e.ca_fingerprint)
  if (e.install_mode === 'docker') {
    return `chmod 600 opsd-token\ndocker run -d --name opsd-agent --restart unless-stopped \\\n  --network host --read-only --tmpfs /tmp:size=32m,mode=1777 \\\n  --cap-add NET_RAW --cap-add NET_ADMIN \\\n  -v /var/run/docker.sock:/var/run/docker.sock \\\n  -v /var/lib/opsd-agent:/var/lib/opsd-agent \\\n  -v "$PWD/opsd-ca.pem:/run/opsd/ca.pem:ro" \\\n  -v "$PWD/opsd-token:/run/opsd/token:ro" \\\n  -e OPSD_AGENT_MODE=container \\\n  ${shellQuote(e.agent_image)} bootstrap \\\n  --hub ${hub} --agent-url ${agentUrl} \\\n  --ca /run/opsd/ca.pem --fingerprint ${fingerprint} \\\n  --token-file /run/opsd/token`
  }
  return `chmod 600 opsd-token\nopsd-agent --data-dir /var/lib/opsd-agent enroll \\\n  --hub ${hub} --agent-url ${agentUrl} \\\n  --ca ./opsd-ca.pem --fingerprint ${fingerprint} \\\n  --token-file ./opsd-token\nopsd-agent --data-dir /var/lib/opsd-agent run`
})

const entry = computed(() => workspace.selectedEntry.value)
const node = computed(() => entry.value?.node ?? null)
const network = computed(() => workspace.state.network?.nodes.find(item => item.node_id === node.value?.id))
async function setController() {
  if (!node.value) return
  try { await apiPut('/api/v1/network', {node_id: node.value.id}); await workspace.refreshNetwork() }
  catch (reason) { enrollmentError.value = reason instanceof Error ? reason.message : '设置控制机失败' }
}
const metricsNode = computed(() =>
  node.value ? workspace.state.metrics[node.value.id] : null
)
// 便捷访问 sample 内层（避免模板里反复 .sample?.）
const m = computed(() => metricsNode.value?.sample ?? null)

function nodeAddr(): string {
  if (!node.value) return '—'
  return node.value.overlay_address ?? node.value.public_addresses[0] ?? '—'
}

const memPercent = computed(() => {
  if (!m.value || m.value.memory_total === 0) return null
  return (m.value.memory_used / m.value.memory_total) * 100
})

const diskPercent = computed(() => {
  const disk = m.value?.disks?.[0]
  if (!disk || disk.total === 0) return null
  return (disk.used / disk.total) * 100
})
</script>

<template>
  <div class="nodes-page">
    <div class="nodes-toolbar">
      <span>{{ workspace.state.nodeEntries.length }} 个已注册节点</span>
      <Button variant="primary" @click="openEnrollment">添加节点</Button>
    </div>
    <p v-if="enrollmentError && !enrollmentOpen" class="enrollment-error" role="alert">{{ enrollmentError }}</p>
    <section v-if="pending.length" class="enrollment-pending">
      <h3>待接入节点</h3>
      <div v-for="p in pending" :key="p.node_id" class="enrollment-row">
        <div>
          <strong>{{ p.name }}</strong>
          <span>{{ p.install_mode === 'docker' ? 'Docker' : '宿主机' }} · 令牌有效期至 {{ formatUnixTime(p.expires_at) }}</span>
        </div>
        <Badge :tone="p.status === 'pending' ? 'warning' : 'default'">{{ p.status === 'pending' ? '待注册' : '已过期' }}</Badge>
        <Button v-if="p.status === 'expired'" variant="default" size="sm" :disabled="enrollmentBusy" @click="regenerateEnrollment(p)">重新生成</Button>
        <Button variant="ghost" size="sm" :busy="revoking === p.node_id" @click="revokeEnrollment(p.node_id)">取消注册</Button>
      </div>
    </section>
    <div v-if="!node" class="nodes-page__empty">
      <span>{{ pending.length ? '等待 Agent 注册，完成后节点会出现在这里' : '还没有节点，点击“添加节点”开始接入' }}</span>
    </div>

    <template v-else>
      <!-- 节点头部 -->
      <div class="node-header">
        <div class="node-header__info">
          <h2 class="node-header__name">{{ node.name }}</h2>
          <code class="node-header__addr">{{ nodeAddr() }}</code>
        </div>
        <Status :tone="entry!.connected ? 'success' : 'danger'"
                :label="entry!.connected ? '在线' : '离线'" />
      </div>

      <!-- 指标卡片 -->
      <div v-if="m" class="metrics-grid">
        <div class="metric-card">
          <span class="metric-card__label">CPU</span>
          <span class="metric-card__value">{{ formatPercent(m.cpu_usage) }}</span>
        </div>
        <div class="metric-card">
          <span class="metric-card__label">内存</span>
          <span class="metric-card__value">{{ formatPercent(memPercent) }}</span>
          <span class="metric-card__sub">{{ formatBytes(m.memory_used) }} / {{ formatBytes(m.memory_total) }}</span>
        </div>
        <div class="metric-card">
          <span class="metric-card__label">磁盘</span>
          <span class="metric-card__value">{{ formatPercent(diskPercent) }}</span>
          <template v-if="m.disks[0]">
            <span class="metric-card__sub">{{ formatBytes(m.disks[0].used) }} / {{ formatBytes(m.disks[0].total) }}</span>
          </template>
        </div>
        <div class="metric-card">
          <span class="metric-card__label">运行时间</span>
          <span class="metric-card__value">{{ formatUptime(m.uptime) }}</span>
        </div>
      </div>

      <div v-else-if="!entry!.connected" class="nodes-page__offline">
        节点离线，指标不可用
      </div>
      <div v-else class="nodes-page__offline">{{ metricsNode?.error || '尚未采集指标' }}</div>
    </template>
    <section v-if="node" class="network-panel">
      <div class="nodes-toolbar"><h2>控制线路与自动互信</h2><Button size="sm" :disabled="workspace.state.network?.directory.controller_node_id === node.id" @click="setController">{{ workspace.state.network?.directory.controller_node_id === node.id ? '当前 Hub 宿主节点' : '指定为 Hub 宿主节点' }}</Button></div>
      <p>控制机 Agent 仍需手动安装。公网恢复稳定 15 秒后优先；节点间探测与 ET 直连、中继状态分别显示。</p>
      <div class="network-paths"><div v-for="path in network?.connections.paths" :key="path.path"><strong>{{ path.path === 'public' ? '公网' : 'ET' }}</strong> · {{ path.connected ? '已连接' : '未连接' }}{{ network?.connections.active_path === path.path ? '（使用中）' : '' }} · {{ path.rtt_ms == null ? '—' : path.rtt_ms + ' ms' }}<span v-if="path.error"> · {{ path.error }}</span></div></div>
      <h3>节点间双线路探测</h3>
      <p>以下结果来自节点实际探测。ICMP 无回应表示本次探测未成功，ET 直连或中继详情在 ET 插件中查看。</p>
      <div v-if="m?.peers?.length" class="network-table-wrap"><table class="network-table"><thead><tr><th>对端节点</th><th>路径</th><th>探测地址</th><th>方式</th><th>结果</th><th>采集时间</th></tr></thead><tbody><tr v-for="peer in m?.peers || []" :key="`${peer.node_id}:${peer.path}:${peer.address}`"><td>{{ workspace.state.nodeEntries.find(item => item.node.id === peer.node_id)?.node.name || peer.node_id }}</td><td>{{ peer.path === 'overlay' ? 'ET' : '公网' }}</td><td>{{ peer.address }}</td><td>{{ peer.method.toUpperCase() }}</td><td>{{ peer.reachable ? (peer.latency_ms == null ? '可达，未取得延迟' : peer.latency_ms + ' ms') : peer.error || '本次探测未成功' }}</td><td>{{ peer.probed_at == null ? '未知' : formatUnixTime(peer.probed_at) }}</td></tr></tbody></table></div>
      <p v-else>尚未取得节点间探测结果。</p>
      <p v-if="network?.applied">白名单：期望 {{ network.desired_revision ?? '未知' }} / 已应用 {{ network.applied.applied_revision }} · {{ network.applied.backend || '尚未应用' }}<span v-if="network.applied.error" class="enrollment-error"> · {{ network.applied.error }}</span></p>
      <p v-else>尚未收到白名单应用结果；旧 Agent 不支持自动互信。</p>
      <details><summary>地址观测与来源</summary><pre>{{ JSON.stringify(network?.addresses, null, 2) }}</pre></details>
    </section>
    <Drawer :open="enrollmentOpen" title="添加节点" size="lg" @close="closeEnrollment">
      <p v-if="enrollmentError" class="enrollment-error" role="alert">{{ enrollmentError }}</p>
      <form v-if="!enrollment" id="node-enrollment-form" class="enrollment-form" @submit.prevent="createEnrollment">
        <label for="new-node-name">节点名称</label>
        <input id="new-node-name" v-model="draft.name" class="enrollment-input" required maxlength="100" placeholder="例如：应用服务器" />
        <label for="new-node-mode">安装方式</label>
        <select id="new-node-mode" v-model="draft.mode" class="enrollment-input">
          <option value="host">宿主机 Agent</option>
          <option value="docker">Docker Agent</option>
        </select>
        <p class="enrollment-hint">{{ draft.mode === 'docker' ? 'Docker 模式提供只读采集；部署业务插件需要宿主机 Agent。' : '宿主机模式在 Linux 上运行，需要先安装 opsd-agent。' }}</p>
        <p class="enrollment-hint">地址由 Agent 自动发现，ET 地址通过 ET 插件关联。宿主机模式支持插件节点动作与自动互信；Docker 模式仅保留现有只读能力。</p>
        <p class="enrollment-hint">创建后生成一次性令牌，10 分钟内有效。Agent 完成注册后才进入节点列表。</p>
      </form>
      <div v-else class="enrollment-result">
        <Status v-if="registeredEntry?.connected" tone="success" label="节点已在线" />
        <Status v-else-if="registeredEntry" tone="warning" label="已注册，等待 Agent 连接" />
        <Status v-else tone="warning" label="注册信息已生成，等待 Agent 接入" />
        <p>安装命令和脚本包含一次性凭据，首次注册有效期至 {{ formatUnixTime(enrollment.expires_at) }}，关闭后无法重新获取，请勿公开分享。</p>
        <template v-if="expiredEntry">
          <p class="enrollment-error">注册令牌已过期，请重新生成安装信息。</p>
          <Button variant="primary" :busy="enrollmentBusy" @click="regenerateEnrollment(expiredEntry)">重新生成安装信息</Button>
        </template>
        <template v-else-if="enrollment.installation">
          <div class="enrollment-actions">
            <Button variant="primary" @click="copyInstallCommand">复制一键安装命令</Button>
            <Button @click="downloadScript">下载安装脚本</Button>
          </div>
          <p>在目标机的 root Bash 终端粘贴命令即可，程序、CA 和令牌会自动准备。目标机需要能访问 GitHub、Hub；Docker 模式还需已有可用 Docker 并能访问 GHCR。</p>
          <p>安装器会配置常驻服务，并等待 Hub 确认在线；下载脚本后也可执行 <code>bash opsd-install.sh</code>。</p>
          <p v-if="copyMessage" role="status">{{ copyMessage }}</p>
          <label for="node-install-command">一键安装命令</label>
          <textarea id="node-install-command" ref="commandField" class="enrollment-command enrollment-copy-field" :value="oneClickCommand" rows="4" readonly spellcheck="false" @copy="downloaded = true" />
        </template>
        <p v-else class="enrollment-error">当前 Hub 构建未提供可用的一键安装发行信息，请更新 Hub 或使用下方高级手动接入。</p>
        <details v-if="!expiredEntry" class="enrollment-advanced">
          <summary>高级：下载原始令牌与手动接入</summary>
          <Button @click="downloadToken">下载令牌文件</Button>
          <p>将令牌文件与管理员提供的 Hub CA 证书放到目标机，证书命名为 <code>opsd-ca.pem</code>。</p>
          <p v-if="enrollment.install_mode === 'host'">手动方式需要先安装 <code>opsd-agent</code>，并自行配置常驻服务。</p>
          <pre class="enrollment-command">{{ installCommand }}</pre>
        </details>
      </div>
      <template #footer>
        <Button variant="ghost" :disabled="enrollmentBusy" @click="closeEnrollment">关闭</Button>
        <Button v-if="!enrollment" variant="primary" type="submit" form="node-enrollment-form" :busy="enrollmentBusy">生成注册信息</Button>
      </template>
    </Drawer>
  </div>
</template>

<style scoped>
.network-panel { margin:24px 0;padding:20px;border:1px solid var(--color-border);border-radius:var(--radius-lg);background:var(--color-surface);font-size:13px; }
.network-panel h2 { font-size:16px;margin:0; }.network-paths { display:grid;gap:10px; }.network-panel pre { white-space:pre-wrap;overflow-wrap:anywhere;font-size:12px; }
.network-table-wrap { overflow-x:auto; }.network-table { width:100%;border-collapse:collapse;font-size:12px; }.network-table th,.network-table td { padding:10px;text-align:left;border-bottom:1px solid var(--color-border);white-space:nowrap; }
.nodes-page { display: flex; flex-direction: column; gap: var(--sp-5); }
.nodes-toolbar { display: flex; align-items: center; justify-content: space-between; color: var(--color-muted); }
.enrollment-pending { display: flex; flex-direction: column; gap: var(--sp-3); }
.enrollment-pending h3 { font-size: var(--text-md); color: var(--color-ink); }
.enrollment-row { display: flex; align-items: center; gap: var(--sp-3); padding: var(--sp-3); background: var(--color-surface); border: 1px solid var(--color-border-subtle); border-radius: var(--radius-md); }
.enrollment-row > div { display: flex; flex-direction: column; gap: var(--sp-1); flex: 1; }
.enrollment-row span, .enrollment-hint { font-size: var(--text-sm); color: var(--color-muted); }
.enrollment-form, .enrollment-result { display: flex; flex-direction: column; gap: var(--sp-3); }
.enrollment-form label { color: var(--color-ink); font-size: var(--text-sm); }
.enrollment-input { width: 100%; min-height: var(--control-md); padding: var(--sp-2) var(--sp-3); border: 1px solid var(--color-border); border-radius: var(--radius-md); color: var(--color-ink); background: var(--color-surface); font: inherit; }
.enrollment-input:focus-visible { outline: none; box-shadow: var(--shadow-focus); }
.enrollment-error { color: var(--color-danger); font-size: var(--text-sm); }
.enrollment-result { font-size: var(--text-sm); color: var(--color-muted); }
.enrollment-actions { display: flex; flex-wrap: wrap; gap: var(--sp-3); }
.enrollment-copy-field { width: 100%; border: 1px solid var(--color-border); resize: vertical; font-size: var(--text-xs); }
.enrollment-advanced { display: flex; flex-direction: column; }
.enrollment-advanced summary { cursor: pointer; margin-bottom: var(--sp-3); color: var(--color-ink); }
.enrollment-advanced p, .enrollment-advanced pre { margin-top: var(--sp-3); }
.enrollment-command { white-space: pre-wrap; overflow-wrap: anywhere; padding: var(--sp-3); border-radius: var(--radius-md); background: var(--color-bg); color: var(--color-ink); }
.nodes-page__empty, .nodes-page__offline {
  color: var(--color-muted); font-size: var(--text-sm);
  padding: var(--sp-8); text-align: center;
}

.node-header {
  display: flex; align-items: center;
  justify-content: space-between; gap: var(--sp-4);
}
.node-header__info { display: flex; flex-direction: column; gap: var(--sp-1); }
.node-header__name { font-size: var(--text-xl); font-weight: var(--weight-semibold); color: var(--color-ink); }
.node-header__addr { font-size: var(--text-sm); color: var(--color-muted); font-family: var(--font-mono); }

.metrics-grid {
  display: grid; grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
  gap: var(--sp-3);
}
.metric-card {
  display: flex; flex-direction: column; gap: var(--sp-1);
  padding: var(--sp-4); background: var(--color-surface);
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-lg); box-shadow: var(--shadow-sm);
}
.metric-card__label { font-size: var(--text-xs); color: var(--color-muted); font-weight: var(--weight-medium); }
.metric-card__value { font-size: var(--text-xl); font-weight: var(--weight-bold); color: var(--color-ink); font-family: var(--font-mono); }
.metric-card__sub { font-size: var(--text-xs); color: var(--color-muted); }
</style>
