/**
 * opsd web3 — 全局响应式状态（workspace）
 * 节点列表、任务、指标、SSE 连接、页面路由
 *
 * API 类型来源：openapi-typescript 生成的 generated.ts
 * 关键修正：
 *   - Session.csrf（不是 csrf_token）
 *   - GET /nodes 返回 NodeEntry[]（{ connected, node }）
 *   - GET /metrics/overview 返回 { nodes: MetricsOverviewNode[] }
 *   - 注册令牌列表用 EnrollmentSummary，创建返回 EnrollmentToken
 */

import { reactive, readonly, computed } from 'vue'
import { apiGet, apiPost, setCsrfToken, entrancePath, ApiError } from '../api/client'
import type { components } from '../api/generated'

// ---- 类型别名 ----

type Session          = components['schemas']['Session']
type Node             = components['schemas']['Node']
type NodeEntry        = components['schemas']['NodeEntry']
type MetricsOverview  = components['schemas']['MetricsOverviewNode']
type EnrollmentSummary = components['schemas']['EnrollmentSummary']

type Task = components['schemas']['TaskEnvelope']

// ---- 页面定义 ----

export type PageName =
  | 'console'
  | 'nodes'
  | 'docker'
  | 'firewall'
  | 'database'
  | 'storage'
  | 'settings'

export const PAGES: { name: PageName; label: string }[] = [
  { name: 'console',  label: '控制台' },
  { name: 'nodes',    label: '节点' },
  { name: 'docker',   label: 'Docker' },
  { name: 'firewall', label: '防火墙' },
  { name: 'database', label: '数据库' },
  { name: 'storage',  label: '存储' },
  { name: 'settings', label: '设置' },
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
  session: Session | null
  loading: boolean
  error: string | null

  // 数据（entries = { connected, node } 包装结构）
  nodeEntries: NodeEntry[]
  tasks: Task[]
  enrollments: EnrollmentSummary[]
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

  nodeEntries: [],
  tasks: [],
  enrollments: [],
  metrics: {},

  page: (getUrlParam('page') as PageName) ?? 'console',
  selectedNodeId: getUrlParam('node'),

  sidebarCollapsed: localStorage.getItem('opsd3.sidebar') === 'collapsed',
  activeTaskCount: 0,
  activeStreams: 0,
})

// ---- 计算属性 ----

/** 所有节点的 Node 对象（平铺） */
const nodes = computed<Node[]>(() =>
  state.nodeEntries.map(e => e.node)
)

/** 在线节点 */
const connectedNodes = computed<Node[]>(() =>
  state.nodeEntries.filter(e => e.connected).map(e => e.node)
)

/** 当前选中的 NodeEntry（含 connected 状态） */
const selectedEntry = computed<NodeEntry | null>(() => {
  if (state.selectedNodeId) {
    const found = state.nodeEntries.find(e => e.node.id === state.selectedNodeId)
    if (found) return found
  }
  return state.nodeEntries[0] ?? null
})

/** 当前选中节点的 Node 对象 */
const selectedNode = computed<Node | null>(() =>
  selectedEntry.value?.node ?? null
)

/** 进行中的任务 */
const pendingTasks = computed<Task[]>(() =>
  state.tasks.filter(t =>
    t.status === 'pending' ||
    t.status === 'accepted' ||
    t.status === 'running' ||
    t.status === 'validating'
  )
)

// ---- SSE 连接 ----

let evtSource: EventSource | null = null
let reconnectTimer: ReturnType<typeof setTimeout> | null = null
let refreshDebounce: ReturnType<typeof setTimeout> | null = null
let refreshFallback: ReturnType<typeof setInterval> | null = null

function scheduleRefresh(): void {
  if (refreshDebounce) clearTimeout(refreshDebounce)
  refreshDebounce = setTimeout(() => { void loadAll() }, 250)
}

function startSSE(): void {
  if (evtSource || !state.session) return
  const csrf = state.session?.csrf ?? ''
  // SSE URL 从安全入口生成，带 csrf 查询参数
  const url = `${entrancePath()}/api/v1/events?csrf=${encodeURIComponent(csrf)}`
  evtSource = new EventSource(url, { withCredentials: true })

  evtSource.onmessage = () => { scheduleRefresh() }

  evtSource.onerror = () => {
    evtSource?.close()
    evtSource = null
    reconnectTimer = setTimeout(startSSE, 5000)
  }

  if (!refreshFallback) {
    refreshFallback = setInterval(() => { void loadAll() }, 15_000)
  }
}

function stopSSE(): void {
  evtSource?.close()
  evtSource = null
  if (reconnectTimer) clearTimeout(reconnectTimer)
  reconnectTimer = null
  if (refreshDebounce) clearTimeout(refreshDebounce)
  refreshDebounce = null
  if (refreshFallback) clearInterval(refreshFallback)
  refreshFallback = null
}

// ---- 数据加载 ----

async function loadAll(): Promise<void> {
  const results = await Promise.allSettled([
    loadNodes(),
    loadTasks(),
    loadMetrics(),
    loadEnrollments(),
  ])
  const failure = results.find((r): r is PromiseRejectedResult => r.status === 'rejected')
  state.error = failure ? (failure.reason instanceof Error ? failure.reason.message : '控制台数据加载失败') : null
  if (failure?.reason instanceof ApiError && failure.reason.isUnauthorized) {
    state.session = null
    setCsrfToken(null)
    stopSSE()
  }
  state.activeTaskCount = pendingTasks.value.length
}

async function loadNodes(): Promise<void> {
  state.nodeEntries = await apiGet<NodeEntry[]>('/api/v1/nodes')
}

async function loadTasks(): Promise<void> {
  state.tasks = await apiGet<Task[]>('/api/v1/tasks')
}

async function loadMetrics(): Promise<void> {
  // GET /api/v1/metrics/overview → { nodes: MetricsOverviewNode[] }
  const resp = await apiGet<{ nodes: MetricsOverview[] }>('/api/v1/metrics/overview')
  const map: Record<string, MetricsOverview> = {}
  for (const m of resp.nodes) map[m.node_id] = m
  state.metrics = map
}

async function loadEnrollments(): Promise<void> {
  state.enrollments = await apiGet<EnrollmentSummary[]>('/api/v1/enrollment-tokens')
}

// ---- 认证 ----

async function init(): Promise<void> {
  state.loading = true
  state.error = null
  try {
    // GET /api/v1/auth/me → Session { csrf, expires }
    const session = await apiGet<Session>('/api/v1/auth/me')
    state.session = session
    setCsrfToken(session.csrf)
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
  // POST /api/v1/auth/login → Session { csrf, expires }
  const session = await apiPost<Session>('/api/v1/auth/login', { password })
  state.session = session
  setCsrfToken(session.csrf)
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
  state.nodeEntries = []
  state.tasks = []
  state.metrics = {}
  state.enrollments = []
}

// ---- 页面路由 ----

function navigate(page: PageName, nodeId?: string): void {
  state.page = page
  if (nodeId !== undefined) state.selectedNodeId = nodeId
  pushRoute({ page, node: state.selectedNodeId })
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

// ---- 节点操作（提交任务） ----

async function submitAction(
  nodeId: string,
  action: unknown,
  idempotencyKey: string,
): Promise<{ task_id: string; status: string }> {
  const res = await apiPost<{ task_id: string; status: string }>(
    `/api/v1/nodes/${nodeId}/actions`,
    { action, idempotency_key: idempotencyKey },
  )
  scheduleRefresh()
  return res
}

// ---- 导出 ----

export const workspace = {
  // 状态（只读）
  state: readonly(state),

  // 计算属性
  nodes,
  connectedNodes,
  selectedEntry,
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
