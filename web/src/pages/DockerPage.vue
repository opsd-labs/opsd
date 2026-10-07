<script setup lang="ts">
/** Docker 盘点来自 Agent 上报，操作通过正式任务接口提交。 */
import { ref, computed } from 'vue'
import { workspace } from '../state/workspace'
import { formatBytes } from '../utils/format'
import Button from '../components/ui/Button.vue'
import Badge from '../components/ui/Badge.vue'
import Skeleton from '../components/ui/Skeleton.vue'
import type { BadgeTone } from '../components/ui/Badge.vue'

type DockerTab = 'containers' | 'stacks' | 'images' | 'networks' | 'volumes'
// Node.inventory 是正式契约中的开放 JSON，以下结构对应 Agent 的盘点输出。
type RawContainer = {
  id: string; names: string[]; image: string; state: string; status: string
  project: string | null; protected: boolean
  ports?: { PublicPort?: number; PrivatePort: number; Type: string }[]
}
type Inventory = {
  docker?: { state: string; error?: string; data?: {
    containers: RawContainer[]
    images: { Id: string; RepoTags?: string[]; Size: number }[]
    networks: { Id: string; Name: string; Driver: string; Scope: string; IPAM?: { Config?: { Subnet?: string }[] } }[]
    volumes: { Name: string; Driver: string; Mountpoint: string }[] | null
  } }
}
const tabs: { key: DockerTab; label: string }[] = [
  { key: 'containers', label: '容器' }, { key: 'stacks', label: 'Stack' },
  { key: 'images', label: '镜像' }, { key: 'networks', label: '网络' },
  { key: 'volumes', label: '卷' },
]
const active = ref<DockerTab>('containers')
const entry = computed(() => workspace.selectedEntry.value)
const node = computed(() => entry.value?.node ?? null)
const inventory = computed(() => node.value?.inventory as Inventory | null)
const data = computed(() => inventory.value?.docker?.data)
const containers = computed(() => (data.value?.containers ?? []).map(c => ({
  ...c, name: c.names?.map(n => n.replace(/^\//, '')).join(', ') || c.id.slice(0, 12),
  ports: c.ports?.map(p => ({ host_port: p.PublicPort, container_port: p.PrivatePort, protocol: p.Type })),
})))
const stacks = computed(() => {
  const projects = new Map<string, { name: string; service_count: number; status: string }>()
  for (const c of containers.value) {
    if (!c.project) continue
    const p = projects.get(c.project) ?? { name: c.project, service_count: 0, status: '运行中' }
    p.service_count++
    if (c.state !== 'running') p.status = '有容器未运行'
    projects.set(c.project, p)
  }
  return Array.from(projects.values())
})
const images = computed(() => (data.value?.images ?? []).map(i => ({ id: i.Id, tags: i.RepoTags, size: i.Size })))
const networks = computed(() => (data.value?.networks ?? []).map(n => ({
  id: n.Id, name: n.Name, driver: n.Driver, scope: n.Scope,
  subnet: n.IPAM?.Config?.map(c => c.Subnet).filter(Boolean).join(', '),
})))
const volumes = computed(() => (data.value?.volumes ?? []).map(v => ({ name: v.Name, driver: v.Driver, mountpoint: v.Mountpoint })))
const loading = ref(false)
const error = ref('')
const notice = ref('')
const containersLoading = loading, stacksLoading = loading, imagesLoading = loading, networksLoading = loading, volumesLoading = loading
const containersError = computed(() => error.value || (inventory.value?.docker?.state === 'error' ? inventory.value.docker.error : !data.value ? '尚未采集 Docker 盘点，请提交刷新任务' : ''))
const stacksError = containersError, imagesError = containersError, networksError = containersError, volumesError = containersError
const actingContainer = ref('')
async function refreshInventory() {
  if (!node.value) return
  loading.value = true
  error.value = ''
  try {
    await workspace.submitAction(node.value.id, { type: 'inspect' }, crypto.randomUUID())
    notice.value = '盘点任务已提交，完成后列表将自动更新'
  } catch (e: unknown) { error.value = e instanceof Error ? e.message : '刷新提交失败' }
  finally { loading.value = false }
}
const loadContainers = refreshInventory, loadStacks = refreshInventory, loadImages = refreshInventory, loadNetworks = refreshInventory, loadVolumes = refreshInventory
async function containerAction(id: string, operation: 'start' | 'stop' | 'restart') {
  if (!node.value || containers.value.find(c => c.id === id)?.protected) return
  actingContainer.value = id
  error.value = ''
  try {
    await workspace.submitAction(node.value.id, { type: 'docker', container: id, operation }, crypto.randomUUID())
    notice.value = '容器操作已提交，执行结果以后台任务为准'
  } catch (e: unknown) { error.value = e instanceof Error ? e.message : '操作提交失败' }
  finally { actingContainer.value = '' }
}
function containerStatusTone(state: string): BadgeTone {
  return state === 'running' ? 'success' : state === 'paused' ? 'warning' : 'default'
}
function onTabChange(tab: DockerTab) { active.value = tab }
</script>

<template>
  <div class="docker-page">
    <!-- 离线/不支持 -->
    <div v-if="!entry?.connected || !node?.capabilities?.docker" class="docker-unavail">
      <span>{{ !entry?.connected ? '节点离线' : '当前节点不支持 Docker' }}</span>
    </div>

    <template v-else>
      <p v-if="notice" role="status">{{ notice }}</p>
      <!-- Tab 导航 -->
      <div class="docker-tabs" role="tablist">
        <button
          v-for="t in tabs"
          :key="t.key"
          class="docker-tab"
          :class="{ 'docker-tab--active': active === t.key }"
          role="tab"
          :aria-selected="active === t.key"
          @click="onTabChange(t.key)"
        >{{ t.label }}</button>
      </div>

      <!-- ========== 容器 ========== -->
      <div v-if="active === 'containers'" class="docker-content" role="tabpanel">
        <div class="docker-toolbar">
          <span class="docker-count">{{ containers.length }} 个容器</span>
          <Button variant="ghost" size="sm" :busy="containersLoading" @click="loadContainers">刷新</Button>
        </div>
        <p v-if="containersError" class="docker-error">{{ containersError }}</p>

        <div v-if="containersLoading && containers.length === 0" class="docker-skeleton">
          <Skeleton v-for="i in 5" :key="i" height="44px" style="border-radius:var(--radius-sm)" />
        </div>

        <table v-else-if="containers.length > 0" class="docker-table">
          <thead>
            <tr>
              <th>名称</th>
              <th>镜像</th>
              <th>状态</th>
              <th>端口</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="c in containers" :key="c.id">
              <td class="container-name">
                <span>{{ c.name }}</span>
                <code class="container-id">{{ c.id.slice(0, 12) }}</code>
              </td>
              <td class="container-image">{{ c.image }}</td>
              <td>
                <Badge :tone="containerStatusTone(c.state)">{{ c.status }}</Badge>
                <Badge v-if="c.protected" tone="warning">受保护</Badge>
              </td>
              <td class="container-ports">
                <span v-if="c.ports?.length" class="port-list">
                  {{ c.ports.map(p => `${p.host_port ?? ''}→${p.container_port}/${p.protocol}`).join(', ') }}
                </span>
                <span v-else class="text-muted">—</span>
              </td>
              <td class="container-actions">
                <Button
                  v-if="c.state !== 'running'"
                  variant="ghost" size="sm"
                  :busy="actingContainer === c.id"
                  :disabled="c.protected"
                  @click="containerAction(c.id, 'start')"
                >启动</Button>
                <Button
                  v-else
                  variant="ghost" size="sm"
                  :busy="actingContainer === c.id"
                  :disabled="c.protected"
                  @click="containerAction(c.id, 'stop')"
                >停止</Button>
                <Button
                  variant="ghost" size="sm"
                  :busy="actingContainer === c.id"
                  :disabled="c.protected"
                  @click="containerAction(c.id, 'restart')"
                >重启</Button>
              </td>
            </tr>
          </tbody>
        </table>
        <div v-else class="docker-empty">暂无容器</div>
      </div>

      <!-- ========== Stack ========== -->
      <div v-else-if="active === 'stacks'" class="docker-content" role="tabpanel">
        <div class="docker-toolbar">
          <span class="docker-count">{{ stacks.length }} 个 Stack</span>
          <Button variant="ghost" size="sm" :busy="stacksLoading" @click="loadStacks">刷新</Button>
        </div>
        <p v-if="stacksError" class="docker-error">{{ stacksError }}</p>
        <div v-if="stacksLoading && stacks.length === 0" class="docker-skeleton">
          <Skeleton v-for="i in 3" :key="i" height="44px" style="border-radius:var(--radius-sm)" />
        </div>
        <table v-else-if="stacks.length > 0" class="docker-table">
          <thead><tr><th>名称</th><th>容器数</th><th>状态</th></tr></thead>
          <tbody>
            <tr v-for="s in stacks" :key="s.name">
              <td>{{ s.name }}</td>
              <td>{{ s.service_count }}</td>
              <td><Badge tone="default">{{ s.status }}</Badge></td>
            </tr>
          </tbody>
        </table>
        <div v-else class="docker-empty">暂无 Stack</div>
      </div>

      <!-- ========== 镜像 ========== -->
      <div v-else-if="active === 'images'" class="docker-content" role="tabpanel">
        <div class="docker-toolbar">
          <span class="docker-count">{{ images.length }} 个镜像</span>
          <Button variant="ghost" size="sm" :busy="imagesLoading" @click="loadImages">刷新</Button>
        </div>
        <p v-if="imagesError" class="docker-error">{{ imagesError }}</p>
        <div v-if="imagesLoading && images.length === 0" class="docker-skeleton">
          <Skeleton v-for="i in 4" :key="i" height="44px" style="border-radius:var(--radius-sm)" />
        </div>
        <table v-else-if="images.length > 0" class="docker-table">
          <thead><tr><th>标签</th><th>ID</th><th>大小</th></tr></thead>
          <tbody>
            <tr v-for="img in images" :key="img.id">
              <td>{{ img.tags?.join(', ') || '&lt;无标签&gt;' }}</td>
              <td><code class="container-id">{{ img.id.replace('sha256:', '').slice(0, 12) }}</code></td>
              <td>{{ formatBytes(img.size) }}</td>
            </tr>
          </tbody>
        </table>
        <div v-else class="docker-empty">暂无镜像</div>
      </div>

      <!-- ========== 网络 ========== -->
      <div v-else-if="active === 'networks'" class="docker-content" role="tabpanel">
        <div class="docker-toolbar">
          <span class="docker-count">{{ networks.length }} 个网络</span>
          <Button variant="ghost" size="sm" :busy="networksLoading" @click="loadNetworks">刷新</Button>
        </div>
        <p v-if="networksError" class="docker-error">{{ networksError }}</p>
        <div v-if="networksLoading && networks.length === 0" class="docker-skeleton">
          <Skeleton v-for="i in 3" :key="i" height="44px" style="border-radius:var(--radius-sm)" />
        </div>
        <table v-else-if="networks.length > 0" class="docker-table">
          <thead><tr><th>名称</th><th>驱动</th><th>范围</th><th>子网</th></tr></thead>
          <tbody>
            <tr v-for="n in networks" :key="n.id">
              <td>{{ n.name }}</td>
              <td>{{ n.driver }}</td>
              <td>{{ n.scope }}</td>
              <td class="text-mono">{{ n.subnet ?? '—' }}</td>
            </tr>
          </tbody>
        </table>
        <div v-else class="docker-empty">暂无网络</div>
      </div>

      <!-- ========== 卷 ========== -->
      <div v-else-if="active === 'volumes'" class="docker-content" role="tabpanel">
        <div class="docker-toolbar">
          <span class="docker-count">{{ volumes.length }} 个卷</span>
          <Button variant="ghost" size="sm" :busy="volumesLoading" @click="loadVolumes">刷新</Button>
        </div>
        <p v-if="volumesError" class="docker-error">{{ volumesError }}</p>
        <div v-if="volumesLoading && volumes.length === 0" class="docker-skeleton">
          <Skeleton v-for="i in 3" :key="i" height="44px" style="border-radius:var(--radius-sm)" />
        </div>
        <table v-else-if="volumes.length > 0" class="docker-table">
          <thead><tr><th>名称</th><th>驱动</th><th>挂载点</th></tr></thead>
          <tbody>
            <tr v-for="v in volumes" :key="v.name">
              <td>{{ v.name }}</td>
              <td>{{ v.driver }}</td>
              <td class="text-mono">{{ v.mountpoint }}</td>
            </tr>
          </tbody>
        </table>
        <div v-else class="docker-empty">暂无卷</div>
      </div>
    </template>
  </div>
</template>

<style scoped>
.docker-page { display: flex; flex-direction: column; gap: var(--sp-4); }
.docker-unavail { color: var(--color-muted); font-size: var(--text-sm); padding: var(--sp-8); text-align: center; }

.docker-tabs {
  display: flex;
  border-bottom: 1px solid var(--color-border-subtle);
}
.docker-tab {
  padding: var(--sp-2) var(--sp-4);
  font-size: var(--text-sm); font-weight: var(--weight-medium);
  color: var(--color-muted); background: transparent;
  border: none; border-bottom: 2px solid transparent;
  cursor: pointer; transition: color var(--duration-fast), border-color var(--duration-fast);
  margin-bottom: -1px;
}
.docker-tab:hover { color: var(--color-ink); }
.docker-tab--active { color: var(--color-accent); border-bottom-color: var(--color-accent); }
.docker-tab:focus-visible { outline: none; box-shadow: var(--shadow-focus); border-radius: var(--radius-sm); }

.docker-content { display: flex; flex-direction: column; gap: var(--sp-3); }

.docker-toolbar {
  display: flex; align-items: center; justify-content: space-between; gap: var(--sp-3);
}
.docker-count { font-size: var(--text-sm); color: var(--color-muted); }
.docker-error { font-size: var(--text-sm); color: var(--color-danger); }
.docker-empty { font-size: var(--text-sm); color: var(--color-muted); padding: var(--sp-6); text-align: center; }
.docker-skeleton { display: flex; flex-direction: column; gap: var(--sp-1); }

.docker-table {
  width: 100%; border-collapse: collapse; font-size: var(--text-sm);
}
.docker-table th, .docker-table td {
  padding: var(--sp-2) var(--sp-3);
  text-align: left; border-bottom: 1px solid var(--color-border-subtle);
  white-space: nowrap;
}
.docker-table th {
  font-weight: var(--weight-medium); color: var(--color-muted);
  background: var(--color-surface); position: sticky; top: 0;
}
.docker-table tr:hover td { background: var(--color-hover-bg); }

.container-name { display: flex; flex-direction: column; gap: 2px; }
.container-id { font-family: var(--font-mono); font-size: var(--text-xs); color: var(--color-muted); }
.container-image { max-width: 240px; overflow: hidden; text-overflow: ellipsis; }
.container-ports { font-size: var(--text-xs); font-family: var(--font-mono); }
.container-actions { display: flex; gap: var(--sp-1); }
.text-muted { color: var(--color-muted); }
.text-mono { font-family: var(--font-mono); font-size: var(--text-xs); }
.port-list { font-family: var(--font-mono); font-size: var(--text-xs); }
</style>
