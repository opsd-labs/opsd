<script setup lang="ts">
import { computed, onMounted, reactive, ref, watch } from 'vue'
import { apiGet } from '../api/client'
import { workspace } from '../state/workspace'

interface FirewallRule {
  id: string
  family: 4 | 6
  direction: 'input' | 'output' | 'forward'
  protocol: 'tcp' | 'udp' | 'icmp' | 'icmpv6' | 'any'
  source: string | null
  destination: string | null
  ports: number[]
  action: 'accept' | 'drop' | 'reject'
  enabled: boolean
}

interface Policy {
  backend: string
  rules: FirewallRule[]
  peers: { version: number; addresses: string[] }
  input_policy: 'accept' | 'drop'
  output_policy: 'accept' | 'drop'
  forward_policy: 'accept' | 'drop'
  adopted: boolean
}

interface Inspection {
  backend: string
  fingerprint: string
  policy: Policy
  external_rules: Record<string, string>
  listeners: string
  maintenance_services: string[]
  docker_forwarding: string
}

interface FirewallTaskResult {
  firewall?: { state?: string; data?: Inspection; error?: string }
  state?: string
  data?: Inspection
  error?: string
  plan_id?: string
  expires?: number
  before?: Policy
  after?: Policy
  requires_witness?: boolean
  rollback_seconds?: number
  external_rules_preserved?: boolean
  operation_id?: string
}

const entries = computed(() => workspace.state.nodeEntries)
const entry = computed(() => workspace.selectedEntry.value)
const targetId = computed(() => entry.value?.node.id ?? '')
const onlineWitnesses = computed(() => entries.value.filter(item => item.connected && item.node.id !== targetId.value))
const inspection = ref<Inspection | null>(null)
const desiredPeers = ref<{ version: number; addresses: string[] } | null>(null)
const busy = ref(false)
const error = ref('')
const taskMessage = ref('')
const plan = ref<FirewallTaskResult | null>(null)
const witness = ref('')
const editing = ref(false)
const formError = ref('')
const defaultAction = reactive({ input: 'accept', output: 'accept', forward: 'accept' })
const defaultDirection = ref<'input' | 'output' | 'forward'>('input')
const draft = reactive({
  id: '', family: '4', direction: 'input', protocol: 'tcp', source: '', destination: '',
  ports: '', action: 'accept', enabled: true,
})

const capable = computed(() => (entry.value?.node.capabilities?.firewall?.length ?? 0) > 0)
const policy = computed(() => inspection.value?.policy ?? null)
const appliedPeerVersion = computed(() => {
  const inventory = entry.value?.node.inventory as { firewall?: { data?: { policy?: { peers?: { version?: number } } } } } | null | undefined
  return inventory?.firewall?.data?.policy?.peers?.version ?? null
})
const peerStatus = computed(() => desiredPeers.value == null
  ? '未知'
  : appliedPeerVersion.value === desiredPeers.value.version ? '已同步' : '待同步')
const affectedDirections = computed(() => {
  if (!plan.value?.before || !plan.value.after) return []
  const before = plan.value.before
  const after = plan.value.after
  const changed = (['input', 'output', 'forward'] as const).filter(direction =>
    before[`${direction}_policy`] !== after[`${direction}_policy`]
      || JSON.stringify(before.rules.filter(rule => rule.direction === direction)) !== JSON.stringify(after.rules.filter(rule => rule.direction === direction)),
  )
  if (JSON.stringify(before.peers) !== JSON.stringify(after.peers)) changed.push('input', 'output', 'forward')
  return [...new Set(changed)]
})

watch(targetId, () => {
  inspection.value = null
  plan.value = null
  taskMessage.value = ''
  error.value = ''
  void loadInspection()
})

onMounted(() => { void loadInspection() })

async function runTask(action: unknown, prefix: string): Promise<FirewallTaskResult> {
  const submitted = await workspace.submitAction(targetId.value, action, `${prefix}-${crypto.randomUUID()}`)
  taskMessage.value = `任务 ${submitted.task_id} 已受理`
  return new Promise((resolve, reject) => {
    let stopWatch = () => {}
    const timeout = setTimeout(() => finish(new Error('任务状态确认超时，请在任务列表查看结果')), 100_000)
    const finish = (failure?: Error, task?: (typeof workspace.state.tasks)[number]) => {
      clearTimeout(timeout)
      stopWatch()
      if (failure) reject(failure)
      else resolve((task?.result ?? {}) as FirewallTaskResult)
    }
    const observe = (task: (typeof workspace.state.tasks)[number] | undefined) => {
      if (task?.status === 'succeeded') finish(undefined, task)
      else if (task && ['failed', 'blocked', 'uncertain', 'rolled_back', 'cancelled'].includes(task.status)) {
        finish(new Error(task.error || `任务状态：${task.status}`))
      }
    }
    stopWatch = watch(() => workspace.state.tasks.find(item => item.id === submitted.task_id), observe)
    observe(workspace.state.tasks.find(item => item.id === submitted.task_id))
  })
}

async function loadInspection(): Promise<void> {
  if (!targetId.value || !entry.value?.connected || !capable.value) return
  busy.value = true
  error.value = ''
  try {
    const result = await runTask({ type: 'inspect' }, 'firewall-inspect')
    if (result.firewall?.state !== 'ok' || !result.firewall.data) {
      throw new Error(result.firewall?.error || 'Agent 未返回防火墙盘点结果')
    }
    inspection.value = result.firewall.data
    defaultAction.input = result.firewall.data.policy.input_policy
    defaultAction.output = result.firewall.data.policy.output_policy
    defaultAction.forward = result.firewall.data.policy.forward_policy
    desiredPeers.value = await apiGet<{ version: number; addresses: string[] }>('/api/v1/peer-addresses')
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : '防火墙盘点失败'
  } finally {
    busy.value = false
  }
}

function resetDraft(rule?: FirewallRule): void {
  Object.assign(draft, rule ? {
    id: rule.id, family: String(rule.family), direction: rule.direction, protocol: rule.protocol,
    source: rule.source ?? '', destination: rule.destination ?? '', ports: rule.ports.join(', '),
    action: rule.action, enabled: rule.enabled,
  } : {
    id: '', family: '4', direction: 'input', protocol: 'tcp', source: '', destination: '',
    ports: '', action: 'accept', enabled: true,
  })
  formError.value = ''
  editing.value = true
}

function makeRule(): FirewallRule {
  const ports = draft.ports.trim() ? draft.ports.split(',').map(value => Number(value.trim())) : []
  if (!draft.id.trim()) throw new Error('请填写规则编号')
  if (ports.length > 15 || ports.some(port => !Number.isInteger(port) || port < 1 || port > 65535)) {
    throw new Error('端口须为 1 至 65535 的整数，最多十五个')
  }
  return {
    id: draft.id.trim(), family: Number(draft.family) as 4 | 6,
    direction: draft.direction as FirewallRule['direction'], protocol: draft.protocol as FirewallRule['protocol'],
    source: draft.source.trim() || null, destination: draft.destination.trim() || null, ports,
    action: draft.action as FirewallRule['action'], enabled: draft.enabled,
  }
}

async function prepare(operation: Record<string, unknown>): Promise<void> {
  busy.value = true
  error.value = ''
  plan.value = null
  try {
    const result = await runTask({ type: 'firewall_plan', operation }, 'firewall-plan')
    if (!result.plan_id) throw new Error('Agent 未返回有效计划')
    plan.value = result
    witness.value = result.requires_witness ? (onlineWitnesses.value[0]?.node.id ?? '') : ''
    editing.value = false
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : '生成防火墙计划失败'
  } finally {
    busy.value = false
  }
}

async function saveRule(): Promise<void> {
  try {
    await prepare({ type: 'rule_put', rule: makeRule() })
  } catch (cause) {
    formError.value = cause instanceof Error ? cause.message : '规则无效'
  }
}

async function applyPlan(): Promise<void> {
  if (!plan.value?.plan_id) return
  if (plan.value.requires_witness && !witness.value) {
    error.value = '该计划需要选择另一台在线节点验证新连接'
    return
  }
  busy.value = true
  error.value = ''
  try {
    const result = await runTask({ type: 'firewall_apply', plan_id: plan.value.plan_id, witness: plan.value.requires_witness ? witness.value : null }, 'firewall-apply')
    const successMessage = `变更已应用，操作编号 ${result.operation_id ?? plan.value.plan_id}`
    plan.value = null
    await loadInspection()
    taskMessage.value = successMessage
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : '应用防火墙计划失败'
  } finally {
    busy.value = false
  }
}

function moveRule(index: number, offset: number): void {
  const rules = policy.value?.rules ?? []
  const ids = rules.map(rule => rule.id)
  const destination = index + offset
  if (destination < 0 || destination >= ids.length) return
  ;[ids[index], ids[destination]] = [ids[destination], ids[index]]
  void prepare({ type: 'rule_order', ids })
}

async function prepareDefaultPolicy(): Promise<void> {
  const direction = defaultDirection.value
  if (policy.value?.[`${direction}_policy`] === defaultAction[direction]) return
  await prepare({ type: 'default_policy', direction, action: defaultAction[direction] })
}
</script>

<template>
  <div class="fw-page">
    <header class="fw-toolbar">
      <label class="fw-node">节点
        <select :value="targetId" @change="workspace.selectNode(($event.target as HTMLSelectElement).value)">
          <option v-for="item in entries" :key="item.node.id" :value="item.node.id">{{ item.node.name }}{{ item.connected ? '' : '（离线）' }}</option>
        </select>
      </label>
      <button class="fw-button" type="button" :disabled="busy || !entry?.connected || !capable" @click="loadInspection">{{ busy ? '处理中…' : '刷新状态' }}</button>
    </header>

    <p v-if="error" class="fw-alert" role="alert">{{ error }}</p>
    <p v-if="taskMessage" class="fw-message" role="status">{{ taskMessage }}</p>
    <div v-if="!entry" class="fw-empty">没有可管理的节点</div>
    <div v-else-if="!entry.connected" class="fw-empty">当前节点离线，无法读取或提交防火墙任务</div>
    <div v-else-if="!capable" class="fw-empty">当前节点未上报防火墙管理能力</div>
    <div v-else-if="busy && !inspection" class="fw-empty">正在读取 Agent 防火墙状态…</div>

    <template v-if="inspection && policy">
      <section class="fw-section">
        <div class="fw-summary">
          <div><span>管理器</span><strong>{{ inspection.backend }}</strong></div>
          <div><span>受管区域</span><strong>{{ policy.adopted ? '已接管' : '未接管' }}</strong></div>
          <div><span>监听端口</span><strong>{{ inspection.listeners.split('\n').filter(Boolean).length }}</strong></div>
          <div><span>维护服务</span><strong>{{ inspection.maintenance_services.length }}</strong></div>
        </div>
        <div class="fw-peer">
          <div>
            <strong>自动节点互通</strong>
            <span>期望版本 {{ desiredPeers?.version ?? '未知' }} · 节点已应用版本 {{ appliedPeerVersion ?? '未上报' }}</span>
          </div>
          <span :class="peerStatus === '已同步' ? 'fw-good' : 'fw-pending'">{{ peerStatus }}</span>
          <p>自动互通地址 {{ desiredPeers?.addresses.length ?? 0 }} 个；由节点目录维护，手工规则不会覆盖。</p>
        </div>
        <p v-if="inspection.docker_forwarding" class="fw-note">{{ inspection.docker_forwarding }}</p>
      </section>

      <section class="fw-section">
        <div class="fw-heading"><h2>默认策略</h2><button class="fw-button" :disabled="busy || !policy.adopted" @click="prepareDefaultPolicy">生成计划</button></div>
        <label class="fw-default-direction">生成计划的方向<select v-model="defaultDirection"><option value="input">入站</option><option value="output">出站</option><option value="forward">转发</option></select></label>
        <div class="fw-defaults">
          <label v-for="direction in (['input', 'output', 'forward'] as const)" :key="direction">{{ { input: '入站', output: '出站', forward: '转发' }[direction] }}
            <select v-model="defaultAction[direction]" :disabled="!policy.adopted">
              <option value="accept">允许</option><option value="drop">丢弃</option>
            </select>
          </label>
        </div>
      </section>

      <section class="fw-section">
        <div class="fw-heading"><h2>受管规则</h2>
          <button v-if="!policy.adopted" class="fw-button" :disabled="busy" @click="prepare({ type: 'adopt' })">接管受管区域</button>
          <button v-else class="fw-button" :disabled="busy" @click="resetDraft()">新增规则</button>
        </div>
        <div v-if="!policy.adopted" class="fw-note">接管只建立 OPSD 自管区域，当前系统和外部规则保持原样。</div>
        <div v-else-if="!policy.rules.length" class="fw-empty">没有受管规则</div>
        <div v-else class="fw-table-wrap">
          <table><thead><tr><th>顺序 / 编号</th><th>方向</th><th>类型</th><th>来源 → 目标</th><th>端口</th><th>动作</th><th>状态</th><th>操作</th></tr></thead>
            <tbody><tr v-for="(rule, index) in policy.rules" :key="rule.id">
              <td>{{ index + 1 }} · {{ rule.id }}</td><td>{{ rule.direction }}</td><td>IPv{{ rule.family }} {{ rule.protocol }}</td>
              <td>{{ rule.source || '任意' }} → {{ rule.destination || '任意' }}</td><td>{{ rule.ports.join(', ') || '全部' }}</td>
              <td>{{ rule.action }}</td><td>{{ rule.enabled ? '启用' : '停用' }}</td>
              <td class="fw-actions"><button title="上移" :disabled="busy || index === 0" @click="moveRule(index, -1)">↑</button><button title="下移" :disabled="busy || index === policy.rules.length - 1" @click="moveRule(index, 1)">↓</button><button :disabled="busy" @click="resetDraft(rule)">编辑</button><button :disabled="busy" @click="prepare({ type: 'rule_delete', id: rule.id })">删除</button></td>
            </tr></tbody>
          </table>
        </div>
      </section>

      <section v-if="editing" class="fw-section">
        <div class="fw-heading"><h2>{{ draft.id && policy.rules.some(rule => rule.id === draft.id) ? '编辑规则' : '新增规则' }}</h2><button class="fw-button" @click="editing = false">关闭</button></div>
        <form class="fw-form" @submit.prevent="saveRule">
          <label>规则编号<input v-model="draft.id" maxlength="63" required /></label>
          <label>地址族<select v-model="draft.family"><option value="4">IPv4</option><option value="6">IPv6</option></select></label>
          <label>方向<select v-model="draft.direction"><option value="input">入站</option><option value="output">出站</option><option value="forward">转发</option></select></label>
          <label>协议<select v-model="draft.protocol"><option value="tcp">TCP</option><option value="udp">UDP</option><option value="icmp">ICMP</option><option value="icmpv6">ICMPv6</option><option value="any">任意</option></select></label>
          <label>来源 CIDR<input v-model="draft.source" placeholder="留空表示任意" /></label>
          <label>目标 CIDR<input v-model="draft.destination" placeholder="留空表示任意" /></label>
          <label>端口<input v-model="draft.ports" :disabled="!['tcp', 'udp'].includes(draft.protocol)" placeholder="例如 80, 443" /></label>
          <label>动作<select v-model="draft.action"><option value="accept">允许</option><option value="drop">丢弃</option><option value="reject">拒绝</option></select></label>
          <label class="fw-check"><input v-model="draft.enabled" type="checkbox" /> 启用</label>
          <p v-if="formError" class="fw-alert" role="alert">{{ formError }}</p>
          <button class="fw-button fw-primary" type="submit" :disabled="busy">生成规则计划</button>
        </form>
      </section>

      <section v-if="plan" class="fw-section fw-plan">
        <div class="fw-heading"><h2>计划差异</h2><span>有效至 {{ new Date((plan.expires ?? 0) * 1000).toLocaleTimeString() }}</span></div>
        <div class="fw-plan-meta"><span>影响方向：{{ affectedDirections.join('、') || '无方向变化' }}</span><span>外部规则：保留</span><span>失败自动回滚：{{ plan.rollback_seconds }} 秒</span></div>
        <div class="fw-diff"><div><h3>应用前</h3><pre>{{ JSON.stringify(plan.before, null, 2) }}</pre></div><div><h3>应用后</h3><pre>{{ JSON.stringify(plan.after, null, 2) }}</pre></div></div>
        <label v-if="plan.requires_witness" class="fw-witness">在线见证节点
          <select v-model="witness"><option value="">选择另一台在线节点</option><option v-for="item in onlineWitnesses" :key="item.node.id" :value="item.node.id">{{ item.node.name }}</option></select>
        </label>
        <p v-if="plan.requires_witness && !onlineWitnesses.length" class="fw-alert">当前没有其他在线节点，计划不能应用。</p>
        <div class="fw-plan-actions"><button class="fw-button" :disabled="busy" @click="plan = null">放弃计划</button><button class="fw-button fw-primary" :disabled="busy || (plan.requires_witness && (!witness || !onlineWitnesses.length))" @click="applyPlan">{{ busy ? '处理中…' : '应用计划' }}</button></div>
      </section>

      <details class="fw-section fw-details"><summary>外部规则与监听详情</summary><pre>{{ JSON.stringify(inspection.external_rules, null, 2) }}</pre><pre>{{ inspection.listeners || '无监听端口' }}</pre><p>维护服务</p><ul><li v-for="service in inspection.maintenance_services" :key="service">{{ service }}</li></ul></details>
    </template>
  </div>
</template>

<style scoped>
.fw-page { display: flex; flex-direction: column; gap: var(--sp-4); min-width: 0; color: var(--color-ink); }
.fw-toolbar, .fw-heading, .fw-plan-actions { display: flex; align-items: center; justify-content: space-between; gap: var(--sp-3); }
.fw-node, .fw-defaults label, .fw-witness { display: flex; align-items: center; gap: var(--sp-2); font-size: var(--text-sm); }
.fw-node select, .fw-defaults select, .fw-witness select, .fw-form input:not([type='checkbox']), .fw-form select { min-width: 0; padding: var(--sp-2) var(--sp-3); border: 1px solid var(--color-border); border-radius: var(--radius-sm); background: var(--color-surface); color: var(--color-ink); }
.fw-node select { min-width: 220px; }
.fw-section { min-width: 0; padding-block: var(--sp-4); border-bottom: 1px solid var(--color-border-subtle); }
.fw-summary { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: var(--sp-3); }
.fw-summary div { display: flex; flex-direction: column; gap: var(--sp-1); }
.fw-summary span, .fw-peer span, .fw-heading > span, .fw-note { color: var(--color-muted); font-size: var(--text-sm); }
.fw-peer { display: grid; grid-template-columns: 1fr auto; gap: var(--sp-2) var(--sp-4); margin-top: var(--sp-4); padding: var(--sp-3) 0; border-top: 1px solid var(--color-border-subtle); border-bottom: 1px solid var(--color-border-subtle); }
.fw-peer div { display: flex; flex-direction: column; gap: var(--sp-1); }
.fw-peer p { grid-column: 1 / -1; margin: 0; color: var(--color-muted); font-size: var(--text-sm); }
.fw-good { color: var(--color-positive); }.fw-pending { color: var(--color-warning); }
.fw-heading { margin-bottom: var(--sp-3); }.fw-heading h2 { margin: 0; font-size: var(--text-md); }
.fw-defaults { display: flex; flex-wrap: wrap; gap: var(--sp-4); }
.fw-button { min-height: 36px; padding: 0 var(--sp-3); border: 1px solid var(--color-border); border-radius: var(--radius-sm); background: var(--color-surface); color: var(--color-ink); cursor: pointer; }
.fw-button:disabled { opacity: .5; cursor: not-allowed; }.fw-primary { border-color: var(--color-accent); background: var(--color-accent); color: white; }
.fw-table-wrap { overflow-x: auto; }
table { width: 100%; border-collapse: collapse; font-size: var(--text-sm); }
th, td { padding: var(--sp-2) var(--sp-3); border-bottom: 1px solid var(--color-border-subtle); text-align: left; white-space: nowrap; }
th { color: var(--color-muted); font-weight: 600; }
.fw-actions { display: flex; gap: var(--sp-1); }.fw-actions button { min-height: 30px; border: 0; background: transparent; color: var(--color-primary); cursor: pointer; }.fw-actions button:disabled { opacity: .4; }
.fw-form { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: var(--sp-3); max-width: 900px; }
.fw-form label { display: flex; flex-direction: column; gap: var(--sp-1); font-size: var(--text-sm); }
.fw-form .fw-check { flex-direction: row; align-items: center; }.fw-form > button, .fw-form > p { grid-column: 1 / -1; justify-self: start; }
.fw-plan-meta { display: flex; flex-wrap: wrap; gap: var(--sp-4); color: var(--color-muted); font-size: var(--text-sm); }
.fw-diff { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: var(--sp-3); margin-block: var(--sp-3); }
.fw-diff h3 { margin: 0 0 var(--sp-2); font-size: var(--text-sm); }
pre { max-height: 360px; overflow: auto; padding: var(--sp-3); background: var(--color-code-bg); border-radius: var(--radius-sm); font-size: var(--text-xs); white-space: pre-wrap; overflow-wrap: anywhere; }
.fw-plan-actions { justify-content: flex-end; margin-top: var(--sp-3); }
.fw-witness { margin-top: var(--sp-3); }.fw-witness select { min-width: 240px; }
.fw-alert { color: var(--color-danger); font-size: var(--text-sm); }.fw-message { color: var(--color-success); font-size: var(--text-sm); }
.fw-empty { color: var(--color-muted); font-size: var(--text-sm); padding-block: var(--sp-5); }
.fw-details summary { cursor: pointer; }.fw-details pre { margin-top: var(--sp-3); }
.fw-default-direction { display: flex; align-items: center; gap: var(--sp-2); margin-bottom: var(--sp-3); color: var(--color-muted); font-size: var(--text-sm); }
.fw-default-direction select { padding: var(--sp-2) var(--sp-3); border: 1px solid var(--color-border); border-radius: var(--radius-sm); background: var(--color-surface); color: var(--color-ink); }
@media (max-width: 760px) {
  .fw-toolbar { align-items: stretch; flex-wrap: wrap; }.fw-node { flex: 1; align-items: stretch; flex-direction: column; }.fw-node select { width: 100%; min-width: 0; }
  .fw-summary { grid-template-columns: repeat(2, minmax(0, 1fr)); }.fw-form, .fw-diff { grid-template-columns: minmax(0, 1fr); }
  .fw-defaults { display: grid; grid-template-columns: 1fr; }.fw-defaults label { justify-content: space-between; }
}
</style>
