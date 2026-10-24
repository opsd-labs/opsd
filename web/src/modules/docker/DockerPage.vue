<script setup lang="ts">
import { computed, onUnmounted, ref, watch } from 'vue'
import { workspace } from '../../state/workspace'
import Button from '../../components/ui/Button.vue'
import Badge from '../../components/ui/Badge.vue'
import DataTable from '../../components/data/DataTable.vue'
import Drawer from '../../components/layout/Drawer.vue'
import ModuleFrame from '../shared/ModuleFrame.vue'
import { nativeUrl, useModule, type SetupState } from '../shared/useModule'
import DockerTerminal from './DockerTerminal.vue'

interface Engine { installed: boolean; running: boolean; compose: boolean; version: string | null; error: string | null; checked_at: number }
interface Setup extends SetupState { engines: Record<string, Engine>; bindings: { node_id: string; endpoint_id: number }[] | null }
interface Resource { Id?: string | number; Name?: string; Names?: string[]; Image?: string; Status?: string | number; Labels?: Record<string, string>; Size?: number; RepoTags?: string[]; Driver?: string }
interface Inventory { containers: Resource[]; stacks: Resource[]; images: Resource[]; networks: Resource[]; volumes: { Volumes: Resource[] } }
const module = useModule('docker')
const { available, busy, error, running } = module
const setup = ref<Setup | null>(null)
const inventory = ref<Inventory | null>(null)
const inventoryError = ref('')
const loading = ref(false)
const section = ref('containers')
const selected = computed(() => workspace.selectedEntry.value)
const nodeId = computed(() => selected.value?.node.id ?? '')
const engine = computed(() => setup.value?.engines[nodeId.value])
const bound = computed(() => setup.value?.bindings?.some(item => item.node_id === nodeId.value) ?? false)
const hosts = computed(() => workspace.state.nodeEntries.filter(item => !item.node.revoked && item.node.capabilities?.plugins && item.node.capabilities.execution_mode === 'host'))
const chosen = ref<string[]>([])
const allowInstall = ref(false)
const consoleUrl = ref<string>()
const drawer = ref('')
const output = ref('')
const resourceId = ref('')
const stackId = ref<string | number>()
const name = ref('')
const spec = ref('')
const reference = ref('')
const returnToNetwork = ref(new URLSearchParams(location.search).get('return') === 'network')
let request: AbortController | undefined
let nodeRequests = new AbortController()
const inspected = new Set<string>()
const tabs = [{ key: 'containers', label: '容器' }, { key: 'stacks', label: 'Stack' }, { key: 'images', label: '镜像' }, { key: 'networks', label: '容器网络' }, { key: 'volumes', label: '卷' }]
const rows = computed(() => {
  const data = inventory.value
  const items = !data ? [] : section.value === 'volumes' ? data.volumes?.Volumes ?? [] : data[section.value as 'containers' | 'stacks' | 'images' | 'networks'] ?? []
  return items.map(item => ({ id: String(item.Id ?? item.Name), name: item.Names?.join(', ') || item.Name || item.RepoTags?.join(', ') || String(item.Id), detail: item.Image || item.Driver || (item.Size == null ? '—' : `${Math.round(item.Size / 1048576)} MiB`), status: section.value === 'stacks' ? item.Status === 1 ? '运行中' : '已停止' : item.Status ?? '—', source: item }))
})
const columns = computed(() => [{ key: 'name', label: '名称', sortable: true }, { key: 'detail', label: section.value === 'containers' ? '镜像' : '信息' }, { key: 'status', label: '状态' }, { key: 'actions', label: '操作' }])
function item(row: Record<string, unknown>) { return row.source as Resource }
function requireNode() { if (!nodeId.value) throw new Error('请先选择节点'); return nodeId.value }
async function refreshInventory() {
  request?.abort(); request = new AbortController()
  if (!nodeId.value || !bound.value) { inventory.value = null; return }
  const id = nodeId.value; const signal = request.signal
  loading.value = true; inventoryError.value = ''
  try {
    const value = await module.read<Inventory>('/inventory', { node_id: id }, signal)
    if (!signal.aborted && id === nodeId.value) inventory.value = value
  } catch (reason) { if (!signal.aborted) inventoryError.value = reason instanceof Error ? reason.message : '读取失败' }
  finally { if (!signal.aborted) loading.value = false }
}
async function refresh() {
  if (!available.value) return
  setup.value = await module.read<Setup>('/setup')
  consoleUrl.value = undefined
  if (setup.value.state === 'ready') consoleUrl.value = nativeUrl((await module.read<{ url: string }>('/console')).url)
  await refreshInventory()
}
async function inspectMissing() {
  const id = nodeId.value
  if (!id || engine.value || inspected.has(id) || running.value || !selected.value?.connected || selected.value.node.capabilities?.execution_mode !== 'host') return
  inspected.add(id)
  await module.job('/engine/inspect', { node_id: id })
  await refresh()
}
async function inspect() { await module.job('/engine/inspect', { node_id: requireNode() }); await refresh() }
async function installEngine() {
  const id = requireNode()
  if (!confirm(`为节点 ${selected.value?.node.name} 安装缺失的 Docker Engine 和 Compose？已有 Engine 不会自动升级。`)) return
  await module.job('/engine/install', { node_id: id, confirmed: true }); await refresh()
}
async function installManagement() {
  if (!chosen.value.length) throw new Error('请选择接入节点')
  if (!confirm('在指定的 Hub 宿主节点部署或复用管理服务，并接入所选节点？')) return
  await module.job('/install', { nodes: chosen.value, allow_engine_install: allowInstall.value }); await refresh()
}
async function mutate(body: Record<string, unknown>) { const id = requireNode(); await module.job('/mutate', { node_id: id, ...body }); if (id === nodeId.value) await refreshInventory() }
async function operation(row: Record<string, unknown>, action: string) {
  const resource = item(row); const id = String(resource.Id ?? resource.Name)
  if (action === 'remove' && !confirm(section.value === 'volumes' ? '删除数据卷会永久删除其中数据，确认继续？' : section.value === 'stacks' ? '删除此 Stack 及其容器？持久卷由项目配置决定。' : '确认删除所选资源？')) return
  await mutate(section.value === 'stacks' ? { resource: 'stack', operation: action, stack_id: resource.Id } : { resource: { containers: 'container', images: 'image', networks: 'network', volumes: 'volume' }[section.value], operation: action, id: section.value === 'images' ? id.replace(/^sha256:/, '') : id, confirmed: true })
}
async function details(row: Record<string, unknown>, kind: string) {
  resourceId.value = String(item(row).Id)
  drawer.value = kind
  if (kind === '终端') return
  const id = nodeId.value
  const value = kind === '日志' ? await module.read<{ text: string }>('/logs', { node_id: id, container: resourceId.value }, nodeRequests.signal) : await module.read('/detail', { node_id: id, resource: 'container', id: resourceId.value }, nodeRequests.signal)
  if (id === nodeId.value) output.value = kind === '日志' ? (value as { text: string }).text : JSON.stringify(value, null, 2)
}
function create() {
  stackId.value = undefined; name.value = ''; drawer.value = '创建'
  spec.value = section.value === 'stacks' ? 'services:\n  app:\n    image: nginx:stable\n' : JSON.stringify(section.value === 'containers' ? { Image: 'nginx:stable', HostConfig: { RestartPolicy: { Name: 'unless-stopped' } } } : section.value === 'volumes' ? { Name: 'app-data' } : { Name: 'app-network', Driver: 'bridge' }, null, 2)
}
async function editStack(row: Record<string, unknown>) {
  const stack = item(row); stackId.value = stack.Id; name.value = stack.Name ?? ''
  const value = await module.read<{ StackFileContent: string }>('/detail', { node_id: requireNode(), resource: 'stack', id: String(stack.Id) }, nodeRequests.signal)
  spec.value = value.StackFileContent; drawer.value = '编辑 Stack'
}
async function save() {
  if (section.value === 'stacks') await mutate({ resource: 'stack', operation: stackId.value == null ? 'create' : 'update', stack_id: stackId.value, name: name.value, content: spec.value })
  else await mutate({ resource: { containers: 'container', networks: 'network', volumes: 'volume' }[section.value], operation: 'create', name: name.value, spec: JSON.parse(spec.value) })
  drawer.value = ''
}
function goNetwork() {
  const url = new URL(location.href); url.searchParams.delete('return'); history.replaceState(null, '', url)
  workspace.navigate('network')
}
watch(available, value => { if (value) void module.run(async () => { await refresh(); await inspectMissing() }) }, { immediate: true })
watch(nodeId, () => { drawer.value = ''; output.value = ''; inventory.value = null; request?.abort(); nodeRequests.abort(); nodeRequests = new AbortController(); if (available.value) { void refreshInventory(); if (!busy.value) void module.run(inspectMissing) } })
watch(busy, value => { if (!value && available.value && nodeId.value && !engine.value && !running.value && !inspected.has(nodeId.value) && selected.value?.connected && selected.value.node.capabilities?.execution_mode === 'host') void module.run(inspectMissing) })
watch(() => module.tasks.value.map(task => `${task.id}:${task.status}`).join(','), () => { if (available.value && !running.value && !busy.value) void module.run(refresh) })
onUnmounted(() => { request?.abort(); nodeRequests.abort() })
</script>

<template>
  <ModuleFrame id="docker" @changed="module.run(refresh)">
    <template #description>管理节点上的容器、应用项目和 Docker 基础环境。</template>
    <p v-if="error" role="alert" class="module-error">{{ error }}</p>
    <section class="module-panel">
      <div class="module-toolbar"><h2>节点基础环境</h2><Button size="sm" :busy="busy" @click="module.run(refresh)">刷新</Button></div>
      <p v-if="!selected" class="module-muted">尚未接入节点。<Button size="sm" @click="workspace.navigate('nodes')">添加节点</Button></p>
      <template v-else>
        <p class="module-muted">{{ selected.node.name }} · {{ selected.connected ? '在线' : '离线' }}</p>
        <p v-if="engine" class="module-row"><Badge :tone="engine.running ? 'success' : 'warning'">{{ engine.running ? `Docker ${engine.version || ''} 可用` : engine.installed ? 'Docker 服务不可用' : '尚未安装 Docker' }}</Badge><Badge>{{ engine.compose ? 'Compose 可用' : '缺少 Compose' }}</Badge><span class="module-muted">检测于 {{ new Date(engine.checked_at * 1000).toLocaleString() }}</span></p>
        <p v-else class="module-muted">尚未进行现场检测；节点上报的能力仅作参考。</p>
        <p v-if="engine?.error" class="module-error">{{ engine.error }}</p>
        <div class="module-row module-actions">
          <Button :disabled="busy || running || !selected.connected || selected.node.capabilities?.execution_mode !== 'host'" @click="module.run(inspect)">检测基础环境</Button>
          <Button v-if="engine && (!engine.installed || !engine.compose)" variant="primary" :disabled="busy || running || !selected.connected" @click="module.run(installEngine)">安装 Docker 基础环境</Button>
          <Button v-if="returnToNetwork" @click="goNetwork">返回网络初始化</Button>
        </div>
      </template>
    </section>
    <section v-if="setup?.state !== 'ready'" class="module-panel"><h2>接入管理服务</h2><p :class="setup?.state === 'unavailable' ? 'module-error' : 'module-muted'">{{ setup?.message || '正在读取状态' }}</p><div class="module-row module-actions"><Button size="sm" @click="workspace.navigate('network')">前往网络</Button><Button size="sm" @click="workspace.navigate('nodes')">选择 Hub 宿主节点</Button></div></section>
    <details class="module-panel"><summary>接入节点与管理服务</summary><p class="module-muted">管理服务基于 Portainer，在指定的 Hub 宿主节点运行；其他节点使用普通管理 Agent。先完成网络关联，并在“管理”中保存凭据。</p><div class="module-checks"><label v-for="entry in hosts" :key="entry.node.id"><input v-model="chosen" type="checkbox" :value="entry.node.id" />{{ entry.node.name }}</label></div><label class="module-row module-muted"><input v-model="allowInstall" type="checkbox" />允许补装所选节点缺失的 Engine／Compose</label><Button class="module-actions" variant="primary" :disabled="busy || running" @click="module.run(installManagement)">安装并接入所选节点</Button></details>
    <template v-if="bound">
      <div class="module-toolbar"><div class="module-tabs" role="tablist" aria-label="Docker 资源"><Button v-for="tab in tabs" :key="tab.key" role="tab" :aria-selected="section === tab.key" variant="ghost" @click="section = tab.key; drawer = ''">{{ tab.label }}</Button></div><a v-if="consoleUrl" :href="consoleUrl" target="_blank" rel="noopener noreferrer">打开原生管理台 ↗</a></div>
      <p v-if="section === 'stacks'" class="module-muted">这里只列出已登记的 Stack；已有 Compose 项目不会从容器反推后自动接管。</p>
      <div v-if="section === 'images'" class="module-row"><label class="module-field"><span>镜像引用</span><input v-model="reference" placeholder="例如 nginx:stable" /></label><Button :disabled="busy || running" @click="module.run(() => mutate({ resource: 'image', operation: 'pull', reference }))">拉取镜像</Button></div>
      <Button v-else :disabled="busy || running" @click="create">{{ section === 'stacks' ? '新建 Stack' : section === 'containers' ? '创建容器' : section === 'volumes' ? '创建数据卷' : '创建容器网络' }}</Button>
      <DataTable :columns="columns" :rows="rows" :loading="loading" :error="inventoryError" empty-text="当前节点没有此类资源">
        <template #actions="{ row }"><div class="module-row">
          <template v-if="section === 'containers'"><Button v-for="action in ['start', 'stop', 'restart']" :key="action" size="sm" :disabled="busy || running || !!item(row).Labels?.['io.opsd.plugin']" @click="module.run(() => operation(row, action))">{{ { start: '启动', stop: '停止', restart: '重启' }[action] }}</Button><Button v-for="kind in ['日志', '详情', '终端']" :key="kind" size="sm" :disabled="busy || (kind === '终端' && !!item(row).Labels?.['io.opsd.plugin'])" @click="module.run(() => details(row, kind))">{{ kind }}</Button></template>
          <template v-if="section === 'stacks'"><Button size="sm" :disabled="busy || running" @click="module.run(() => editStack(row))">编辑</Button><Button v-for="action in ['start', 'stop']" :key="action" size="sm" :disabled="busy || running" @click="module.run(() => operation(row, action))">{{ action === 'start' ? '启动' : '停止' }}</Button></template>
          <Badge v-if="section === 'containers' && item(row).Labels?.['io.opsd.plugin']">受管基础服务</Badge><Button v-else size="sm" variant="danger" :disabled="busy || running" @click="module.run(() => operation(row, 'remove'))">删除</Button>
        </div></template>
      </DataTable>
    </template>
    <p v-else-if="selected" class="module-muted">当前节点尚未接入管理环境；基础环境可独立检测和安装。</p>
    <Drawer :open="!!drawer" :title="drawer" size="lg" @close="drawer = ''">
      <div class="module-page">
        <p v-if="error" role="alert" class="module-error">{{ error }}</p>
        <DockerTerminal v-if="drawer === '终端'" :key="`${nodeId}:${resourceId}`" :node-id="nodeId" :container="resourceId" />
        <form v-else-if="drawer === '创建' || drawer === '编辑 Stack'" @submit.prevent="module.run(save)"><label v-if="section === 'stacks' || section === 'containers'" class="module-field"><span>名称</span><input v-model="name" :disabled="stackId != null" required /></label><label class="module-field"><span>{{ section === 'stacks' ? 'Compose 配置' : '资源参数（JSON）' }}</span><textarea v-model="spec" rows="18" spellcheck="false" required /></label><Button type="submit" variant="primary" :disabled="busy || running">提交并等待执行</Button></form>
        <pre v-else>{{ output }}</pre>
      </div>
    </Drawer>
  </ModuleFrame>
</template>
