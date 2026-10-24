import { computed, onUnmounted, ref, watch } from 'vue'
import { apiFetch, apiGet, apiPost } from '../../api/client'
import { workspace, type PluginRecord } from '../../state/workspace'

export type ModuleId = 'docker' | 'et' | 'silo'
export const modulePages = { docker: 'docker', et: 'network', silo: 'storage' } as const
export const moduleLabels = { docker: 'Docker', et: '网络', silo: '存储' }
export const catalog = ref<PluginRecord['manifest'][]>([])
let catalogRequest: Promise<void> | null = null
export function loadCatalog() {
  if (!catalogRequest) catalogRequest = apiGet<{ modules: PluginRecord['manifest'][] }>('/api/v1/plugins/catalog')
    .then(result => { catalog.value = result.modules })
    .catch(error => { catalogRequest = null; throw error })
  return catalogRequest
}
export const pending = (status: string) => ['pending', 'accepted', 'running', 'validating'].includes(status)
export const taskLabel = (status: string) => ({ pending: '等待执行', accepted: '已受理', running: '执行中', validating: '核实中', succeeded: '执行成功', failed: '执行失败', uncertain: '结果待核实', blocked: '执行受阻', cancelled: '已取消', rolled_back: '已回退' }[status] ?? status)

export interface SetupState {
  state: string
  message: string
}

export function useModule(id: ModuleId) {
  const error = ref('')
  const busy = ref(false)
  const controller = new AbortController()
  const waits = new Set<() => void>()
  const expected = computed(() => catalog.value.find(item => item.id === id))
  const active = computed(() => workspace.state.plugins.find(record => record.manifest.id === id && record.enabled))
  const available = computed(() => !!expected.value && active.value?.manifest.version === expected.value.version)
  const tasks = computed(() => workspace.state.tasks.filter(task => task.node_id === '@hub' && task.action.type === 'plugin' && task.action.plugin === id).sort((a, b) => b.created_at - a.created_at))
  const running = computed(() => tasks.value.some(task => pending(task.status)))
  onUnmounted(() => { controller.abort(); for (const stop of waits) stop(); waits.clear() })

  async function read<T>(path: string, body?: unknown, signal = controller.signal): Promise<T> {
    const response = await apiFetch(`/api/v1/plugins/${id}/api${path}`, { method: body === undefined ? 'GET' : 'POST', body, signal })
    return response.json() as Promise<T>
  }
  async function run(action: () => Promise<unknown>) {
    if (busy.value) return
    busy.value = true
    error.value = ''
    try { await action() }
    catch (reason) { if (!controller.signal.aborted && !(reason instanceof DOMException && reason.name === 'AbortError')) error.value = reason instanceof Error ? reason.message : '操作失败' }
    finally { busy.value = false }
  }
  async function job<T = unknown>(path: string, body: Record<string, unknown>): Promise<T> {
    if (running.value) throw new Error('该功能已有任务执行中，请等待或查看当前进度')
    const accepted = await apiPost<{ task_id: string }>(`/api/v1/plugins/${id}/api${path}`, { ...body, idempotency_key: crypto.randomUUID() })
    await workspace.refreshTasks()
    if (controller.signal.aborted) throw new DOMException('页面已离开，后台任务继续执行', 'AbortError')
    return new Promise<T>((resolve, reject) => {
      let stop = () => {}
      const cancel = () => { stop(); reject(new DOMException('页面已离开，后台任务继续执行', 'AbortError')) }
      const finish = () => {
        const task = workspace.state.tasks.find(task => task.id === accepted.task_id)
        if (!task || pending(task.status)) return
        stop()
        waits.delete(cancel)
        if (task.status === 'succeeded') resolve(task.result as T)
        else reject(new Error(`${taskLabel(task.status)}：${task.error || task.id}`))
      }
      stop = watch(() => workspace.state.tasks, finish)
      waits.add(cancel)
      finish()
    })
  }
  return { id, error, busy, available, active, tasks, running, read, run, job }
}

export function nativeUrl(value: unknown): string | undefined {
  if (typeof value !== 'string' || !value) return undefined
  try { const url = new URL(value); return ['http:', 'https:'].includes(url.protocol) ? url.href : undefined } catch { return undefined }
}
