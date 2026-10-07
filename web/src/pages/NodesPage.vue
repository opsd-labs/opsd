<script setup lang="ts">
/**
 * NodesPage — 单节点深度视图
 * 指标概览（后续扩展为曲线图 + 文件管理 + 宿主机 Shell）
 * MetricsOverviewNode.sample = MetricsSample | null
 */
import { computed, ref, nextTick } from 'vue'
import { workspace } from '../state/workspace'
import { apiPost, apiDelete } from '../api/client'
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
const revoking = ref('')
const draft = ref({ name: '', addresses: '', overlay: '', sshPort: 22, mode: 'host' as Registration['install_mode'] })
const pending = computed(() => workspace.state.enrollments)
const registeredEntry = computed(() => workspace.state.nodeEntries.find(e => e.node.id === enrollment.value?.node_id))

async function openEnrollment() {
  draft.value = { name: '', addresses: '', overlay: '', sshPort: 22, mode: 'host' }
  enrollment.value = null
  enrollmentError.value = ''
  downloaded.value = false
  enrollmentOpen.value = true
  await nextTick()
  document.getElementById('new-node-name')?.focus()
}

function closeEnrollment() {
  if (enrollmentBusy.value) return
  if (enrollment.value && !downloaded.value && !registeredEntry.value && !confirm('令牌尚未下载，关闭后无法重新获取。确定关闭？')) return
  enrollmentOpen.value = false
  enrollment.value = null
}

async function createEnrollment() {
  enrollmentBusy.value = true
  enrollmentError.value = ''
  try {
    const registration: Registration = {
      name: draft.value.name.trim(),
      public_addresses: draft.value.addresses.split(/[\s,，]+/).filter(Boolean),
      overlay_address: draft.value.overlay.trim() || null,
      ssh_port: draft.value.sshPort,
      install_mode: draft.value.mode,
    }
    enrollment.value = await apiPost<EnrollmentToken>('/api/v1/enrollment-tokens', registration)
    await workspace.refreshEnrollments()
  } catch (e: unknown) {
    enrollmentError.value = e instanceof Error ? e.message : '添加节点失败'
  } finally { enrollmentBusy.value = false }
}

function downloadToken() {
  if (!enrollment.value) return
  const url = URL.createObjectURL(new Blob([enrollment.value.token + '\n'], { type: 'text/plain' }))
  const a = document.createElement('a')
  a.href = url
  a.download = 'opsd-token'
  a.click()
  URL.revokeObjectURL(url)
  downloaded.value = true
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
        <p class="enrollment-hint">{{ draft.mode === 'docker' ? 'Docker 模式提供容器管理与只读采集，不提供宿主机 systemd 和存储写入。' : '宿主机模式在 Linux 上运行，需要先安装 opsd-agent。' }}</p>
        <label for="new-node-addresses">公网 IP（选填）</label>
        <input id="new-node-addresses" v-model="draft.addresses" class="enrollment-input" placeholder="多个 IPv4 或 IPv6 地址用逗号分隔" />
        <label for="new-node-overlay">覆盖网 IP（选填）</label>
        <input id="new-node-overlay" v-model="draft.overlay" class="enrollment-input" placeholder="节点的内网或覆盖网地址" />
        <label for="new-node-ssh-port">SSH 端口</label>
        <input id="new-node-ssh-port" v-model.number="draft.sshPort" class="enrollment-input" type="number" min="1" max="65535" required />
        <p class="enrollment-hint">创建后生成一次性令牌，10 分钟内有效。Agent 完成注册后才进入节点列表。</p>
      </form>
      <div v-else class="enrollment-result">
        <Status v-if="registeredEntry?.connected" tone="success" label="节点已在线" />
        <Status v-else-if="registeredEntry" tone="warning" label="已注册，等待 Agent 连接" />
        <Status v-else tone="warning" label="注册信息已生成，等待 Agent 接入" />
        <p>令牌有效期至 {{ formatUnixTime(enrollment.expires_at) }}，仅本次可下载；关闭后无法重新获取。</p>
        <Button variant="primary" @click="downloadToken">下载令牌文件</Button>
        <p>将令牌文件与管理员提供的 Hub CA 证书放到待接入机器，证书命名为 <code>opsd-ca.pem</code>，再在该机器执行下面的命令。</p>
        <p v-if="enrollment.install_mode === 'host'">先安装 <code>opsd-agent</code>，注册后用 systemd 或进程管理器保持运行。</p>
        <h3>{{ enrollment.install_mode === 'docker' ? 'Docker 安装命令' : '宿主机注册命令' }}</h3>
        <pre class="enrollment-command">{{ installCommand }}</pre>
      </div>
      <template #footer>
        <Button variant="ghost" :disabled="enrollmentBusy" @click="closeEnrollment">关闭</Button>
        <Button v-if="!enrollment" variant="primary" type="submit" form="node-enrollment-form" :busy="enrollmentBusy">生成注册信息</Button>
      </template>
    </Drawer>
  </div>
</template>

<style scoped>
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
