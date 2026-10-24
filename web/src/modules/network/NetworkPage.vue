<script lang="ts">
import { reactive } from 'vue'
// 跳转 Docker 安装后保留非秘密向导输入；凭据只由管理表单提交到秘密存储。
const draft = reactive({ node_id: '', bind_address: '', api_port: 11211, config_port: 22020, protocol: 'udp', data_dir: '/srv/opsd/easytier-web', api_url: '', name: 'opsd', cidr: '10.88.0.0/24', nodes: [] as string[], config_endpoint: '', peers: '', listeners: 'tcp://0.0.0.0:11010\nudp://0.0.0.0:11010' })
</script>

<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { workspace } from '../../state/workspace'
import Button from '../../components/ui/Button.vue'
import Badge from '../../components/ui/Badge.vue'
import DataTable from '../../components/data/DataTable.vue'
import Drawer from '../../components/layout/Drawer.vue'
import ModuleFrame from '../shared/ModuleFrame.vue'
import { nativeUrl, useModule, type SetupState } from '../shared/useModule'

interface Machine { machine_id: string; info?: { hostname?: string; easytier_version?: string; report_time?: string } }
interface Deployment { spec: Record<string, unknown>; phase: string; last_error?: string; task_id?: string; members?: { node_id: string; address: string; phase: string }[] }
interface Setup extends SetupState { web: Deployment | null; network: Deployment | null }
const module = useModule('et')
const { available, busy, error, running } = module
const setup = ref<Setup | null>(null)
const machines = ref<Machine[]>([])
const machine = ref<Machine>()
const instances = ref<{ id: string; running: boolean }[]>([])
const instance = ref('')
const info = ref<unknown>()
const original = ref<Record<string, unknown>>({})
const editor = ref('')
const bindNode = ref('')
const open = ref(false)
const consoleUrl = ref<string>()
const hosts = computed(() => workspace.state.nodeEntries.filter(entry => !entry.node.revoked && entry.node.capabilities?.plugins && entry.node.capabilities.execution_mode === 'host'))
const machineRows = computed(() => machines.value.map(value => ({ id: value.machine_id, name: value.info?.hostname || value.machine_id, version: value.info?.easytier_version || '—', reported: value.info?.report_time || '—' })))
const instanceRows = computed(() => instances.value.map(value => ({ id: value.id, status: value.running ? '运行中' : '已停止', running: value.running })))
const plannedMembers = computed(() => {
  const parts = draft.cidr.split('/')[0]?.split('.').map(Number) ?? []
  if (parts.length !== 4 || parts.some(part => !Number.isInteger(part) || part < 0 || part > 255)) return []
  const base = parts.reduce((value, part) => value * 256 + part, 0)
  return draft.nodes.map((id, index) => { const ip = base + index + 1; return { id, name: hosts.value.find(entry => entry.node.id === id)?.node.name || id, address: [24, 16, 8, 0].map(shift => (ip >>> shift) & 255).join('.') } })
})
const lines = (value: string) => value.split(/\r?\n/).map(line => line.trim()).filter(Boolean)
function uuid(value: unknown): string {
  if (typeof value === 'string') return value
  const object = value as Record<string, number>
  const text = ['part1', 'part2', 'part3', 'part4'].map(key => Number(object[key]).toString(16).padStart(8, '0')).join('')
  return `${text.slice(0, 8)}-${text.slice(8, 12)}-${text.slice(12, 16)}-${text.slice(16, 20)}-${text.slice(20)}`
}
async function refresh() {
  if (!available.value) return
  setup.value = await module.read<Setup>('/setup')
  consoleUrl.value = nativeUrl(setup.value.web?.spec.api_url)
  if (setup.value.state === 'ready') {
    machines.value = (await module.read<{ machines: Machine[] }>('/machines')).machines || []
    consoleUrl.value = nativeUrl((await module.read<{ url: string }>('/console')).url)
  }
}
async function showInstances(id: string) {
  machine.value = machines.value.find(value => value.machine_id === id); instance.value = ''; open.value = false
  const value = await module.read<{ running_inst_ids?: unknown[]; disabled_inst_ids?: unknown[] }>('/instances', { machine_id: id })
  const runningIds = (value.running_inst_ids ?? []).map(uuid)
  if (machine.value?.machine_id === id) instances.value = [...new Set([...runningIds, ...(value.disabled_inst_ids ?? []).map(uuid)])].map(id => ({ id, running: runningIds.includes(id) }))
}
async function showInstance(id: string) {
  if (!machine.value) return
  instance.value = id
  const identity = { machine_id: machine.value.machine_id, instance_id: id }
  const [config, runtime] = await Promise.all([module.read<Record<string, unknown>>('/config', identity), module.read('/info', identity)])
  if (instance.value !== id || machine.value.machine_id !== identity.machine_id) return
  original.value = Object.fromEntries(Object.entries(config).filter(([key]) => !['network_secret', 'credential_file', 'instance_id', 'secure_mode'].includes(key)))
  editor.value = JSON.stringify(original.value, null, 2); info.value = runtime; bindNode.value = ''; open.value = true
}
async function saveConfig() {
  const next = JSON.parse(editor.value) as Record<string, unknown>
  const patch = Object.fromEntries(Object.entries(next).filter(([key, value]) => JSON.stringify(value) !== JSON.stringify(original.value[key])))
  if (!Object.keys(patch).length) return
  await module.job('/config/update', { machine_id: machine.value?.machine_id, instance_id: instance.value, patch })
  await showInstance(instance.value)
}
async function bind() {
  if (!bindNode.value) throw new Error('请选择实际对应的 OPSD 节点')
  await module.job('/bind', { machine_id: machine.value?.machine_id, instance_id: instance.value, node_id: bindNode.value })
  await workspace.refreshNetwork()
}
async function changeState(row: Record<string, unknown>) {
  if (!confirm(`${row.running ? '停止' : '启动'}所选网络实例？停止可能中断 ET 管理线路。`)) return
  const id = machine.value?.machine_id
  await module.job('/instance/state', { machine_id: id, instance_id: row.id, disabled: row.running })
  if (id) await showInstances(id)
}
function goDocker(node?: string) {
  const id = node || draft.node_id || draft.nodes[0]
  if (!id) { error.value = '请先选择需要安装 Docker 的节点'; return }
  workspace.navigate('docker', id)
  const url = new URL(location.href); url.searchParams.set('return', 'network'); history.replaceState(null, '', url)
}
async function deployWeb(resume = false) {
  if (!resume && !confirm(`在所选节点部署 ET Web，持久目录为 ${draft.data_dir}，访问地址为 ${draft.api_url}。确认继续？`)) return
  try { await module.job('/web/install', resume ? { resume: true } : { node_id: draft.node_id, bind_address: draft.bind_address, api_port: draft.api_port, config_port: draft.config_port, protocol: draft.protocol, data_dir: draft.data_dir, api_url: draft.api_url }) }
  finally { await refresh() }
}
async function createNetwork(resume = false) {
  if (!resume && !confirm(`创建网络 ${draft.name}（${draft.cidr}），为下列成员安装受管客户端并配置地址：\n${plannedMembers.value.map(member => `${member.name}: ${member.address}`).join('\n')}\n已有未知客户端不会自动接管。`)) return
  try { await module.job('/network/create', resume ? { resume: true } : { name: draft.name, cidr: draft.cidr, nodes: draft.nodes, config_endpoint: draft.config_endpoint, peer_urls: lines(draft.peers), listener_urls: lines(draft.listeners) }) }
  finally { await refresh(); await workspace.refreshNetwork() }
}
watch(available, value => { if (value) void module.run(refresh) }, { immediate: true })
watch(() => module.tasks.value.map(task => `${task.id}:${task.status}`).join(','), () => { if (available.value && !running.value && !busy.value) void module.run(refresh) })
</script>

<template>
  <ModuleFrame id="et" @changed="module.run(refresh)">
    <template #description>配置节点组网、网络实例与连接状态。</template>
    <p v-if="error" role="alert" class="module-error">{{ error }}</p>
    <div class="module-toolbar"><p :class="setup?.state === 'unavailable' ? 'module-error' : 'module-muted'">{{ setup?.message || '正在读取网络状态' }}</p><div class="module-row"><Button size="sm" :busy="busy" @click="module.run(refresh)">刷新</Button><a v-if="consoleUrl" :href="consoleUrl" target="_blank" rel="noopener noreferrer">打开原生管理台 ↗</a></div></div>
    <section v-if="setup?.state !== 'ready'" class="module-panel"><h2>接入网络管理服务</h2><p class="module-muted">已有 ET Web：在右上角“管理”中填写地址和账户。新部署：先完成下方部署，再打开原生管理台完成账户注册、验证码和初始密码设置，最后保存连接凭据。</p></section>
    <details class="module-panel" :open="setup?.web?.phase === 'interrupted'"><summary>部署网络管理服务</summary>
      <template v-if="setup?.web"><p class="module-muted">部署状态：{{ setup.web.phase === 'awaiting_account' ? '服务已启动，等待配置账户' : setup.web.phase === 'interrupted' ? '执行中断，需核实' : '部署中' }}</p><p v-if="setup.web.last_error" class="module-error">{{ setup.web.last_error }}</p><p class="module-muted">节点：{{ setup.web.spec.node_id }} · 地址：{{ setup.web.spec.api_url }}</p><Button :disabled="busy || running" @click="module.run(() => deployWeb(true))">核实并继续原部署</Button></template>
      <form v-else @submit.prevent="module.run(() => deployWeb())">
        <div class="module-grid"><label class="module-field"><span>服务所在节点</span><select v-model="draft.node_id" required><option value="">选择宿主机节点</option><option v-for="entry in hosts" :key="entry.node.id" :value="entry.node.id">{{ entry.node.name }} · {{ entry.connected ? '在线' : '离线' }}</option></select></label><label class="module-field"><span>绑定的本机地址</span><input v-model="draft.bind_address" placeholder="节点实际拥有的 IP" required /></label><label class="module-field"><span>持久目录</span><input v-model="draft.data_dir" required /></label><label class="module-field"><span>Hub 与浏览器可达的 Web 地址</span><input v-model="draft.api_url" placeholder="http://服务器地址:11211" required /></label><label class="module-field"><span>管理端口</span><input v-model.number="draft.api_port" type="number" min="1" max="65535" required /></label><label class="module-field"><span>配置下发端口</span><input v-model.number="draft.config_port" type="number" min="1" max="65535" required /></label><label class="module-field"><span>配置下发协议</span><select v-model="draft.protocol"><option value="udp">UDP</option><option value="tcp">TCP</option></select></label></div>
        <p class="module-muted">使用配套 EasyTier 2.6.4 镜像。监听地址和对外地址分开填写；NAT 端口映射需在提供方配置。节点必须先有可用 Docker。</p>
        <div class="module-row module-actions"><Button @click="goDocker()">前往 Docker 安装或检测</Button><Button type="submit" variant="primary" :disabled="busy || running">部署网络管理服务</Button></div>
      </form>
    </details>
    <details class="module-panel" :open="setup?.network?.phase === 'interrupted'"><summary>客户端安装与首次组网</summary>
      <template v-if="setup?.network"><p class="module-muted">{{ setup.network.phase === 'ready' ? '组网已完成实际接口和连通核对' : '首次组网尚未完成' }}</p><p v-if="setup.network.last_error" class="module-error">{{ setup.network.last_error }}</p><DataTable :columns="[{key:'node_id',label:'节点'},{key:'address',label:'计划地址'},{key:'phase',label:'进度'}]" :rows="setup.network.members || []" row-key="node_id" /><Button class="module-actions" :disabled="busy || running" @click="module.run(() => createNetwork(true))">核实并继续原组网</Button></template>
      <form v-else @submit.prevent="module.run(() => createNetwork())"><p class="module-muted">先完成 ET Web 账户配置，并在“管理”中保存组网密钥。客户端使用宿主网络，不覆盖已有客户端。</p><div class="module-checks"><label v-for="entry in hosts" :key="entry.node.id"><input v-model="draft.nodes" type="checkbox" :value="entry.node.id" />{{ entry.node.name }}<Button size="sm" variant="link" @click="goDocker(entry.node.id)">Docker 环境</Button></label></div>
        <div class="module-grid"><label class="module-field"><span>网络名称</span><input v-model="draft.name" required /></label><label class="module-field"><span>IPv4 网段</span><input v-model="draft.cidr" required /></label><label class="module-field"><span>客户端可达的配置下发地址（含账户）</span><input v-model="draft.config_endpoint" placeholder="udp://服务器地址:22020/用户名" required /></label><label class="module-field"><span>节点间组网入口，每行一个</span><textarea v-model="draft.peers" placeholder="tcp://可达节点地址:11010" rows="3" /></label><label class="module-field"><span>客户端监听入口，每行一个</span><textarea v-model="draft.listeners" rows="3" required /></label></div>
        <DataTable :columns="[{key:'name',label:'成员'},{key:'address',label:'将配置的地址'}]" :rows="plannedMembers" empty-text="请选择成员后核对地址" /><Button class="module-actions" type="submit" variant="primary" :disabled="busy || running || setup?.state !== 'ready'">安装客户端并创建网络</Button>
      </form>
    </details>
    <section class="module-panel"><h2>网络机器</h2><DataTable :columns="[{key:'name',label:'机器'},{key:'version',label:'版本'},{key:'reported',label:'上报时间'},{key:'actions',label:'操作'}]" :rows="machineRows" :error="setup?.state === 'unavailable' ? setup.message : null" empty-text="尚无上报的网络机器"><template #actions="{row}"><Button size="sm" :disabled="busy" @click="module.run(() => showInstances(String(row.id)))">查看实例</Button></template></DataTable></section>
    <section v-if="machine" class="module-panel"><h2>{{ machine.info?.hostname || machine.machine_id }} · 网络实例</h2><DataTable :columns="[{key:'id',label:'实例编号'},{key:'status',label:'状态'},{key:'actions',label:'操作'}]" :rows="instanceRows"><template #actions="{row}"><div class="module-row"><Button size="sm" :disabled="busy" @click="module.run(() => showInstance(String(row.id)))">配置与链路</Button><Button size="sm" :disabled="busy || running" @click="module.run(() => changeState(row))">{{ row.running ? '停止实例' : '启动实例' }}</Button></div></template></DataTable></section>
    <section class="module-panel"><h2>已关联节点</h2><div v-for="overlay in workspace.state.network?.directory.overlays" :key="overlay.node_id" class="module-task module-row"><Badge tone="accent">{{ overlay.address }}</Badge><span>{{ workspace.state.nodeEntries.find(entry => entry.node.id === overlay.node_id)?.node.name || overlay.node_id }}</span><span class="module-muted">{{ overlay.interface }}</span><Button size="sm" @click="workspace.navigate('nodes', overlay.node_id)">查看控制线路</Button></div><p v-if="!workspace.state.network?.directory.overlays.length" class="module-muted">只有核对实际接口后关联的实例参与节点网络目录。</p></section>
    <Drawer :open="open" title="实例配置与关联" size="lg" @close="open = false"><div class="module-page"><p v-if="error" role="alert" class="module-error">{{ error }}</p><p class="module-muted">配置保存在 ET Web；保存只更新改动字段，保留已有网络凭据。</p><label class="module-field"><span>实例配置（JSON）</span><textarea v-model="editor" rows="16" spellcheck="false" /></label><Button variant="primary" :disabled="busy || running" @click="module.run(saveConfig)">保存配置</Button><label class="module-field"><span>关联实际对应的 OPSD 节点</span><select v-model="bindNode"><option value="">请选择</option><option v-for="entry in workspace.state.nodeEntries" :key="entry.node.id" :value="entry.node.id">{{ entry.node.name }}</option></select></label><Button :disabled="busy || running" @click="module.run(bind)">核对接口并关联</Button><h2>直连、中继与运行信息</h2><pre>{{ JSON.stringify(info, null, 2) }}</pre></div></Drawer>
  </ModuleFrame>
</template>
