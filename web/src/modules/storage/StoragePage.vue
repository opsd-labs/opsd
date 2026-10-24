<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { workspace } from '../../state/workspace'
import Button from '../../components/ui/Button.vue'
import Badge from '../../components/ui/Badge.vue'
import DataTable from '../../components/data/DataTable.vue'
import Drawer from '../../components/layout/Drawer.vue'
import ModuleFrame from '../shared/ModuleFrame.vue'
import { nativeUrl, useModule, type SetupState } from '../shared/useModule'

interface Disk { path: string; name: string; kind: string; size: number; serial: string | null }
interface Inventory { devices: Disk[]; risks: ({ device: string; reasons: string[] } & Record<string, unknown>)[]; gaps: string[] }
interface Choice { mode: 'new' | 'existing'; device: string; mount: string; path: string }
interface Member { node_id: string; inventory: Inventory; choices: Choice[] }
interface Cluster { name: string; phase: string; console_url?: string; last_error?: string; prepared?: Record<string, { mounts?: string[] }>; containers?: Record<string, string>; spec?: { members: { node_id: string }[] } }
interface Readiness { summary: string; nodes: { name: string; suitability: string; reasons: string[] }[]; topology: Record<string, unknown> | null; notes?: string[] }
const module = useModule('silo')
const { available, busy, error, running } = module
const setup = ref<SetupState | null>(null)
const clusters = ref<Cluster[]>([])
const readiness = ref<Readiness>()
const name = ref('')
const node = ref('')
const members = ref<Member[]>([])
const confirmed = ref(false)
const drawer = ref('')
const active = ref<Cluster>()
const output = ref('')
const bucket = ref('')
const user = ref('')
const secret = ref('')
const policy = ref('readonly')
const hosts = computed(() => workspace.state.nodeEntries.filter(entry => !entry.node.revoked && entry.node.capabilities?.plugins && entry.node.capabilities.execution_mode === 'host'))
const nodeName = (id: string) => workspace.state.nodeEntries.find(entry => entry.node.id === id)?.node.name || id
const stateLabel = (phase: string) => ({ preparing: '准备节点', containers: '创建成员', waiting: '等待健康', ready: '已就绪', failed: '执行失败', interrupted: '执行中断' }[phase] ?? phase)
const suitability = (state: string) => ({ suitable: '适合', needs_work: '待处理', unsuitable: '不适合', pending: '待采集' }[state] ?? state)
const readinessRows = computed(() => readiness.value?.nodes.map((item, index) => ({ id: String(index), name: item.name, status: suitability(item.suitability), reasons: item.reasons.join('；') })) ?? [])
const progressRows = computed(() => active.value?.spec?.members.map(member => ({ id: member.node_id, name: nodeName(member.node_id), mounts: active.value?.prepared?.[member.node_id]?.mounts?.join('、') || '未准备', container: active.value?.containers?.[member.node_id] || '未创建' })) ?? [])
function candidates(inventory: Inventory) {
  return inventory.devices.filter(disk => disk.kind === 'disk' && inventory.risks.some(risk => risk.device === disk.name && !Object.entries(risk).some(([key, value]) => !['device', 'reasons'].includes(key) && value === true)))
}
function diskRows(inventory: Inventory) {
  return inventory.devices.filter(disk => disk.kind === 'disk').map(disk => ({ id: disk.path, path: disk.path, size: `${(disk.size / 1073741824).toFixed(1)} GiB`, serial: disk.serial || '—', reasons: inventory.risks.find(risk => risk.device === disk.name)?.reasons.join('；') || '未发现使用痕迹' }))
}
async function refresh() {
  if (!available.value) return
  const [state, data] = await Promise.all([module.read<SetupState>('/setup'), module.read<{ clusters: Cluster[] }>('/clusters')])
  setup.value = state; clusters.value = data.clusters
  if (active.value) active.value = clusters.value.find(cluster => cluster.name === active.value?.name)
}
async function inspect() {
  const id = node.value
  if (!id) throw new Error('请选择成员节点')
  if (members.value.some(member => member.node_id === id)) throw new Error('此成员已经加入向导')
  const inventory = await module.job<Inventory>('/inventory', { node_id: id })
  members.value.push({ node_id: id, inventory, choices: [] })
}
function add(member: Member, mode: Choice['mode']) { member.choices.push({ mode, device: '', mount: '', path: '' }) }
async function deploy() {
  if (members.value.length < 4) throw new Error('分布式部署至少选择四个节点')
  const selected = members.value.map(member => {
    if (!member.choices.length) throw new Error(`${nodeName(member.node_id)} 尚未选择数据目录`)
    return { node_id: member.node_id, storage: member.choices.map(choice => {
      if (choice.mode === 'existing') { if (!choice.path) throw new Error('请填写已有目录路径'); return { mode: 'existing', path: choice.path } }
      const device = candidates(member.inventory).find(disk => disk.path === choice.device)
      if (!device || !choice.mount) throw new Error('请明确选择新盘和挂载路径')
      return { mode: 'new', device: device.path, expected_size: device.size, serial: device.serial, mount: choice.mount }
    }) }
  })
  const destructive = selected.flatMap(member => member.storage.filter(storage => storage.mode === 'new').map(storage => `${nodeName(member.node_id)}：${storage.device}，${storage.expected_size} 字节 → ${storage.mount}`))
  if (destructive.length && !confirmed.value) throw new Error('清空新盘需要明确确认')
  if (!confirm(destructive.length ? `将清空下列新盘：\n${destructive.join('\n')}\n确认执行？` : '使用所选已有目录，不格式化，确认部署？')) return
  try { await module.job('/install', { name: name.value, members: selected, acknowledge_destructive: confirmed.value }) }
  finally { await refresh() }
}
async function resume(cluster: Cluster) {
  if (!confirm('先核对已准备目录、成员容器和网络绑定，再继续剩余部署步骤？')) return
  try { await module.job('/install', { name: cluster.name, resume: true }) }
  finally { await refresh() }
}
function show(cluster: Cluster, mode: string) { active.value = cluster; drawer.value = mode; output.value = '' }
async function manage(path: string, body: Record<string, unknown>) {
  if (!active.value) return
  output.value = JSON.stringify(await module.job(path, { cluster: active.value.name, ...body }), null, 2)
}
async function remove(path: string, body: Record<string, unknown>, message: string) { if (confirm(message)) await manage(path, body) }
watch(available, value => { if (value) void module.run(refresh) }, { immediate: true })
watch(() => module.tasks.value.map(task => `${task.id}:${task.status}`).join(','), () => { if (available.value && !running.value && !busy.value) void module.run(refresh) })
</script>

<template>
  <ModuleFrame id="silo" @changed="module.run(refresh)">
    <template #description>管理分布式存储集群、数据目录、桶和访问用户。</template>
    <p v-if="error" role="alert" class="module-error">{{ error }}</p>
    <div class="module-toolbar"><p :class="setup?.state === 'unavailable' ? 'module-error' : 'module-muted'">{{ setup?.message || '正在读取存储状态' }}</p><div class="module-row"><Button size="sm" :busy="busy" @click="module.run(refresh)">刷新</Button><Button size="sm" :disabled="busy" @click="module.run(async () => { readiness = await module.read<Readiness>('/readiness') })">查看部署适合度</Button></div></div>
    <div v-if="setup?.state === 'needs_network' || setup?.state === 'needs_docker'" class="module-row"><Button @click="workspace.navigate('network')">前往网络</Button><Button @click="workspace.navigate('docker')">前往 Docker</Button></div>
    <section v-if="readiness" class="module-panel"><h2>存储适合度</h2><p class="module-muted">{{ readiness.summary }}</p><DataTable :columns="[{key:'name',label:'节点'},{key:'status',label:'结论'},{key:'reasons',label:'原因'}]" :rows="readinessRows" /><pre v-if="readiness.topology">{{ JSON.stringify(readiness.topology, null, 2) }}</pre><p v-else class="module-muted">信息不完整时不生成推荐拓扑。</p><p v-for="note in readiness.notes" :key="note" class="module-muted">{{ note }}</p></section>
    <div class="module-grid"><section v-for="cluster in clusters" :key="cluster.name" class="module-panel"><div class="module-toolbar"><h2>{{ cluster.name }}</h2><Badge :tone="cluster.phase === 'ready' ? 'success' : 'warning'">{{ stateLabel(cluster.phase) }}</Badge></div><p v-if="cluster.last_error" class="module-error">{{ cluster.last_error }}</p><p class="module-muted">{{ Object.keys(cluster.prepared || {}).length }} 个节点已准备 · {{ Object.keys(cluster.containers || {}).length }} 个成员容器已创建</p><div class="module-row module-actions"><Button size="sm" @click="show(cluster, '部署进度')">进度</Button><Button size="sm" :disabled="busy || running" @click="show(cluster, '管理状态'); module.run(() => manage('/status', {}))">读取管理状态</Button><Button v-if="cluster.phase === 'ready'" size="sm" @click="show(cluster, '桶与用户')">桶与用户</Button><Button v-else size="sm" :disabled="busy || running" @click="module.run(() => resume(cluster))">核实后恢复</Button><a v-if="nativeUrl(cluster.console_url)" :href="nativeUrl(cluster.console_url)" target="_blank" rel="noopener noreferrer">打开原生管理台 ↗</a></div></section></div>
    <p v-if="!clusters.length" class="module-muted">尚未部署集群。完成网络、Docker 管理环境和本页管理凭据配置后，可创建集群。</p>
    <details class="module-panel"><summary>部署新集群</summary><form @submit.prevent="module.run(deploy)"><label class="module-field"><span>集群名称</span><input v-model="name" placeholder="例如 objects" required /></label><div class="module-row"><label class="module-field"><span>添加成员</span><select v-model="node"><option value="">选择节点</option><option v-for="entry in hosts" :key="entry.node.id" :value="entry.node.id">{{ entry.node.name }}</option></select></label><Button :disabled="busy || running" @click="module.run(inspect)">盘点并选择数据目录</Button></div>
      <section v-for="(member, memberIndex) in members" :key="member.node_id" class="module-panel"><div class="module-toolbar"><h2>{{ nodeName(member.node_id) }}</h2><Button size="sm" @click="members.splice(memberIndex, 1)">移除此节点</Button></div><p v-if="member.inventory.gaps?.length" class="module-error">盘点缺口：{{ member.inventory.gaps.join('；') }}</p><DataTable :columns="[{key:'path',label:'设备'},{key:'size',label:'容量'},{key:'serial',label:'序列号'},{key:'reasons',label:'使用情况'}]" :rows="diskRows(member.inventory)" />
        <div v-for="(choice, index) in member.choices" :key="index" class="module-panel module-actions"><template v-if="choice.mode === 'new'"><label class="module-field"><span>新盘（将清空）</span><select v-model="choice.device" required><option value="">明确选择设备</option><option v-for="disk in candidates(member.inventory)" :key="disk.path" :value="disk.path">{{ disk.path }} · {{ (disk.size / 1073741824).toFixed(1) }} GiB</option></select></label><label class="module-field"><span>挂载路径</span><input v-model="choice.mount" placeholder="/data/disk1" required /></label></template><label v-else class="module-field"><span>已有路径（不格式化）</span><input v-model="choice.path" placeholder="/srv/silo/data" required /></label><Button size="sm" @click="member.choices.splice(index, 1)">移除此项</Button></div>
        <div class="module-row module-actions"><Button size="sm" @click="add(member, 'new')">添加新盘选择</Button><Button size="sm" @click="add(member, 'existing')">添加已有路径</Button></div>
      </section>
      <label class="module-row module-error module-actions"><input v-model="confirmed" type="checkbox" />我已核对下列新盘设备与容量，确认允许清空所选新盘。已有路径不格式化。</label><Button class="module-actions" type="submit" variant="primary" :disabled="busy || running || setup?.state !== 'ready'">准备全部节点并部署集群</Button>
    </form></details>
    <Drawer :open="!!drawer" :title="`${active?.name || ''} · ${drawer}`" size="lg" @close="drawer = ''"><div class="module-page"><p v-if="error" role="alert" class="module-error">{{ error }}</p><template v-if="drawer === '部署进度'"><DataTable :columns="[{key:'name',label:'节点'},{key:'mounts',label:'目录'},{key:'container',label:'成员容器'}]" :rows="progressRows" /><p class="module-muted">先准备全部节点，再创建所有成员，最后等待健康；失败进度会持久保存。</p></template><template v-else-if="drawer === '桶与用户'"><section class="module-panel"><h2>桶</h2><label class="module-field"><span>桶名称</span><input v-model="bucket" /></label><div class="module-row"><Button :disabled="busy || running" @click="module.run(() => manage('/bucket', {operation:'list'}))">列出桶</Button><Button :disabled="busy || running" @click="module.run(() => manage('/bucket', {operation:'create',bucket}))">创建桶</Button><Button variant="danger" :disabled="busy || running" @click="module.run(() => remove('/bucket', {operation:'remove',bucket}, `删除空桶 ${bucket}？`))">删除空桶</Button></div></section><section class="module-panel"><h2>访问用户</h2><label class="module-field"><span>用户名</span><input v-model="user" /></label><label class="module-field"><span>访问密钥引用（在本页“管理”中保存）</span><input v-model="secret" /></label><label class="module-field"><span>权限</span><select v-model="policy"><option value="readonly">只读</option><option value="readwrite">读写</option><option value="writeonly">只写</option></select></label><div class="module-row"><Button :disabled="busy || running" @click="module.run(() => manage('/user', {operation:'list'}))">列出用户</Button><Button :disabled="busy || running" @click="module.run(() => manage('/user', {operation:'create',user,secret_name:secret,policy}))">创建并绑定策略</Button><Button variant="danger" :disabled="busy || running" @click="module.run(() => remove('/user', {operation:'remove',user}, `删除用户 ${user}？`))">删除用户</Button></div></section></template><pre v-if="output">{{ output }}</pre></div></Drawer>
  </ModuleFrame>
</template>
