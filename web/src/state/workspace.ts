/**
 * opsd web3 — 全局响应式状态（workspace）
 * 节点列表、任务、指标、SSE 连接、页面路由
 */

import { reactive, readonly, computed } from 'vue'
import { apiGet, apiPost, setCsrfToken, ApiError } from '../api/client'
import type {
  Node,
  Task,
  MetricsOverview,
  EnrollmentToken,
  AuthSession,
  ActionResponse,
} from '../api/types'

// ---- 页面定义 ----

export type PageName =
  | 'console'
  | 'nodes'
  | 'docker'
  | 'firewall'
  | 'database'
  | 'storage'
  | 'settings'

export const PAGES: { name: PageName; label: string; icon: string }[] = [
  { name: 'console',  label: '控制台',   icon: 'LayoutDashboard' },
  { name: 'nodes',    label: '节点',     icon: 'Server' },
  { name: 'docker',   label: 'Docker',  icon: 'Container' },
  { name: 'firewall', label: '防火墙',   icon: 'Shield' },
  { name: 'database', label: '数据库',   icon: 'Database' },
  { name: 'storage',  label: '存储',     icon: 'HardDrive' },
  { name: 'settings', label: '设置',     icon: 'Settings' },
]

// ---- URL 路由辅助 ----

function getUrlParam(key: string): string | null {
  return new URLSearchParams(window.location.search).get(key)
}

function pushRoute(params: Record<string, string | null>): void {
  const sp = new URLSearchParams(window.location.search)
  for (const [k, v] of Object.entries(params)) {
    if (v == null) sp.delete(k)
    else sp.set(k, v)
  }
  const query = sp.toString()
  // pushState（不是 replaceState），浏览器前进/后退正常工作
  history.pushState(null, '', query ? `?${query}` : window.location.pathname)
}

// ---- 状态 ----

interface WorkspaceState {
  // 认证
  session: AuthSession | null
  loading: boolean   // 初始连接中
  error: string | null

  // 数据
  nodes: Node[]
  tasks: Task[]
  tokens: EnrollmentToken[]
  metrics: Record<string, MetricsOverview>  // node_id → 最新指标

  // 当前页
  page: PageName
  selectedNodeId: string | null

  // UI 状态
  sidebarCollapsed: boolean
  activeTaskCount: number
  activeStreams: number   // 打开的终端会话数
}

const state = reactive<WorkspaceState>({
  session: null,
  loading: true,
  error: null,

  nodes: [],
  tasks: [],
  tokens: [],
  metrics: {},

  page: (getUrlParam('page') as PageName) ?? 'console',
  selectedNodeId: getUrlParam('node'),

  sidebarCollapsed: localStorage.getItem('opsd3.sidebar') === 'collapsed',
  activeTaskCount: 0,
  activeStreams: 0,
})

// 计算属性

const connectedNodes = computed(() =>
  state.nodes.filter(n => n.connected)
)

const selectedNode = computed(() =>
  state.nodes.find(n => n.id === state.selectedNodeId) ?? state.nodes[0] ?? null
)

const pendingTasks = computed(() =>
  state.tasks.filter(t =>
    t.status === 'pending' ||
    t.status === 'accepted' ||
    t.status === 'running' ||
    t.status === 'validating'
  )
)

// ---- SSE 连接 ----

let evtSource: EventSource | null = null
let refreshDebounce: ReturnType<typeof setTimeout> | null = null
let refreshFallback: ReturnType<typeof setInterval> | null = null

function scheduleRefresh(): void {
  if (refreshDebounce) clearTimeout(refreshDebounce)
  refreshDebounce = setTimeout(() => { void loadAll() }, 250)
}

function startSSE(): void {
  if (evtSource) return
  const url = (window.location.pathname) + '/api/v1/events'
  evtSource = new EventSource(url, { withCredentials: true })

  evtSource.onmessage = () => { scheduleRefresh() }

  evtSource.onerror = () => {
    // SSE 断线时靠 fallback 轮询维持数据
    evtSource?.close()
    evtSource = null
    // 5 秒后重连
    setTimeout(startSSE, 5000)
  }

  // 15 秒轮询兜底（SSE 可能因为空闲被中间代理断开）
  if (!refreshFallback) {
    refreshFallback = setInterval(() => { void loadAll() }, 15_000)
  }
}

function stopSSE(): void {
  evtSource?.close()
  evtSource = null
  if (refreshFallback) clearInterval(refreshFallback)
  refreshFallback = null
}

// ---- 数据加载 ----

async function loadAll(): Promise<void> {
  await Promise.allSettled([
    loadNodes(),
    loadTasks(),
    loadMetrics(),
    loadTokens(),
  ])
  // 更新活跃任务数
  state.activeTaskCount = pendingTasks.value.length
}

async function loadNodes(): Promise<void> {
  try {
    state.nodes = await apiGet<Node[]>('/api/v1/nodes')
  } catch {
    // 不覆盖已有数据
  }
}

async function loadTasks(): Promise<void> {
  try {
    state.tasks = await apiGet<Task[]>('/api/v1/tasks')
  } catch {
    // 静默失败
  }
}

async function loadMetrics(): Promise<void> {
  try {
    const list = await apiGet<MetricsOverview[]>('/api/v1/metrics/overview')
    const map: Record<string, MetricsOverview> = {}
    for (const m of list) map[m.node_id] = m
    state.metrics = map
  } catch {
    // 静默失败
  }
}

async function loadTokens(): Promise<void> {
  try {
    state.tokens = await apiGet<EnrollmentToken[]>('/api/v1/enrollment-tokens')
  } catch {
    // 静默失败
  }
}

// ---- 认证 ----

async function init(): Promise<void> {
  state.loading = true
  state.error = null
  try {
    const session = await apiGet<AuthSession>('/api/v1/auth/me')
    state.session = session
    setCsrfToken(session.csrf_token)
    await loadAll()
    startSSE()
  } catch (e) {
    if (e instanceof ApiError && e.isUnauthorized) {
      state.session = null
    } else {
      state.error = e instanceof Error ? e.message : '连接失败'
    }
  } finally {
    state.loading = false
  }
}

async function login(password: string): Promise<void> {
  const session = await apiPost<AuthSession>('/api/v1/auth/login', { password })
  state.session = session
  setCsrfToken(session.csrf_token)
  state.error = null
  await loadAll()
  startSSE()
}

async function logout(): Promise<void> {
  try {
    await apiPost('/api/v1/auth/logout')
  } catch {
    // 继续
  }
  state.session = null
  setCsrfToken(null)
  stopSSE()
  state.nodes = []
  state.tasks = []
  state.metrics = {}
  state.tokens = []
}

// ---- 页面路由 ----

function navigate(page: PageName, nodeId?: string): void {
  state.page = page
  if (nodeId !== undefined) state.selectedNodeId = nodeId
  pushRoute({
    page,
    node: state.selectedNodeId,
  })
}

function selectNode(nodeId: string | null): void {
  state.selectedNodeId = nodeId
  pushRoute({ node: nodeId })
}

// 监听浏览器前进后退
window.addEventListener('popstate', () => {
  const page = (getUrlParam('page') as PageName) ?? 'console'
  const node = getUrlParam('node')
  state.page = page
  state.selectedNodeId = node
})

// ---- 侧栏 ----

function toggleSidebar(): void {
  state.sidebarCollapsed = !state.sidebarCollapsed
  localStorage.setItem('opsd3.sidebar', state.sidebarCollapsed ? 'collapsed' : 'expanded')
}

// ---- 节点操作 ----

async function submitAction(
  nodeId: string,
  action: unknown,
  idempotencyKey: string,
): Promise<ActionResponse> {
  const res = await apiPost<ActionResponse>(`/api/v1/nodes/${nodeId}/actions`, {
    action,
    idempotency_key: idempotencyKey,
  })
  scheduleRefresh()
  return res
}

// ---- 导出 ----

export const workspace = {
  // 状态（只读访问，通过 actions 修改）
  state: readonly(state),

  // 计算属性
  connectedNodes,
  selectedNode,
  pendingTasks,

  // 认证
  init,
  login,
  logout,

  // 路由
  navigate,
  selectNode,

  // UI
  toggleSidebar,

  // 数据
  loadAll,
  submitAction,
}

export type Workspace = typeof workspace
