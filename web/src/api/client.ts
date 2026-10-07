/* ==========================================================================
   opsd web3 — API 客户端
   fetch 封装：basePath 推导、CSRF 头自动注入、统一错误处理
   ========================================================================== */

/**
 * 推导安全入口路径（URL 第一段始终是安全入口）
 * 支持两种部署路径格式：
 *   /{entrance}/               内置前端
 *   /{entrance}/frontend/{short}/  外部主题前端
 */
export function entrancePath(): string {
  const parts = window.location.pathname.split('/').filter(Boolean)
  if (parts.length === 0) return ''
  return '/' + parts[0]  // 第一段始终是安全入口
}

/** 生成 API 请求路径（安全入口 + API 路径） */
export function apiPath(path: string): string {
  return entrancePath() + path
}

/** 读取登录响应保存的 CSRF token */
function getCsrfToken(): string | null {
  // CSRF token 通过 GET /api/v1/auth/me 接口获取，存储于模块状态
  return _csrfToken
}

let _csrfToken: string | null = null

/** 设置 CSRF token（由 workspace.ts 在登录后调用）*/
export function setCsrfToken(token: string | null): void {
  _csrfToken = token
}

/** API 错误 */
export class ApiError extends Error {
  constructor(
    readonly status: number,
    readonly body: string,
  ) {
    // 尝试从 JSON body 中提取 "error" 字段
    let message = body
    try {
      const parsed = JSON.parse(body)
      if (typeof parsed?.error === 'string') message = parsed.error
    } catch {
      // body 不是 JSON（如框架级 403）
    }
    super(message)
    this.name = 'ApiError'
  }

  get isUnauthorized(): boolean { return this.status === 401 }
  get isForbidden(): boolean { return this.status === 403 }
}

interface FetchOptions extends Omit<RequestInit, 'body'> {
  body?: unknown
  /** 是否跳过 JSON 解析，直接返回 Response（用于文件下载等） */
  raw?: boolean
}

/**
 * 核心 fetch 包装
 * - 自动拼接 basePath
 * - 自动设置 Content-Type: application/json
 * - 自动注入 X-CSRF-Token（非 GET/HEAD 时）
 * - 统一错误处理
 */
export async function apiFetch(path: string, opts: FetchOptions = {}): Promise<Response> {
  const { body, raw: _raw, ...init } = opts
  const method = (init.method ?? 'GET').toUpperCase()

  const headers = new Headers(init.headers)

  if (body !== undefined) {
    if (body instanceof Blob) {
      ;(init as RequestInit).body = body
    } else {
      headers.set('Content-Type', 'application/json')
      ;(init as RequestInit).body = JSON.stringify(body)
    }
  }

  // 注入 CSRF token（非 GET/HEAD）
  if (method !== 'GET' && method !== 'HEAD') {
    const csrf = getCsrfToken()
    if (csrf) headers.set('X-CSRF-Token', csrf)
  }

  const url = apiPath(path)
  const response = await fetch(url, { ...init, headers, method })

  if (!response.ok) {
    const text = await response.text()
    throw new ApiError(response.status, text)
  }

  return response
}

/** GET 请求，返回解析后的 JSON */
export async function apiGet<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await apiFetch(path, { ...init, method: 'GET' })
  return res.json() as Promise<T>
}

/** POST 请求，返回解析后的 JSON */
export async function apiPost<T>(path: string, body?: unknown, init?: RequestInit): Promise<T> {
  const res = await apiFetch(path, { ...init, method: 'POST', body })
  return res.json() as Promise<T>
}

/** PUT 请求，返回解析后的 JSON */
export async function apiPut<T>(path: string, body?: unknown, init?: RequestInit): Promise<T> {
  const res = await apiFetch(path, { ...init, method: 'PUT', body })
  return res.json() as Promise<T>
}

/** DELETE 请求，返回解析后的 JSON */
export async function apiDelete<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await apiFetch(path, { ...init, method: 'DELETE' })
  return res.json() as Promise<T>
}

/** 提交节点操作任务，带幂等键 */
export async function apiSubmitAction<TBody, TRes>(
  nodeId: string,
  action: TBody,
  idempotencyKey: string,
): Promise<TRes> {
  return apiPost<TRes>(`/api/v1/nodes/${nodeId}/actions`, {
    action,
    idempotency_key: idempotencyKey,
  })
}
