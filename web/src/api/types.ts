/**
 * opsd web3 — 业务类型别名
 * 从 openapi-typescript 生成的 generated.ts 导出干净的类型
 * 运行 `npm run api:generate` 更新 generated.ts
 */

// 在生成文件之前先用手写的基础类型，确保项目可以编译
// 待 `npm run api:generate` 运行后，这里改为从 generated.ts 重导出

export interface Node {
  id: string
  name: string
  address: string
  ssh_port: number
  connected: boolean
  capabilities: NodeCapabilities
  region?: string
  group?: string
  tags?: string[]
  hidden?: boolean
}

export interface NodeCapabilities {
  docker: boolean
  compose: boolean
  systemd: boolean
  firewall: string | null  // 'nftables' | 'iptables' | 'ufw' | 'firewalld' | null
  metrics: boolean
  hostinfo: boolean
  storage: boolean
  database: boolean
  probe: boolean
}

export interface Task {
  id: string
  node_id: string
  task_key: string
  status: TaskStatus
  action: string
  created_at: string
  updated_at: string
  result?: unknown
  error?: string
}

export type TaskStatus =
  | 'pending'
  | 'accepted'
  | 'running'
  | 'validating'
  | 'succeeded'
  | 'failed'
  | 'uncertain'
  | 'rollback_pending'
  | 'rolled_back'
  | 'blocked'
  | 'cancelled'

export interface EnrollmentToken {
  node_id: string
  token: string
  ca_fingerprint: string
  created_at: string
  expires_at: string
  status: 'pending' | 'used' | 'expired'
}

export interface MetricsOverview {
  node_id: string
  at: string
  cpu_percent: number
  mem_percent: number
  mem_used_bytes: number
  mem_total_bytes: number
  disk_percent: number
  disk_used_bytes: number
  disk_total_bytes: number
  net_rx_bytes_per_sec: number
  net_tx_bytes_per_sec: number
  load_1: number
  load_5: number
  load_15: number
  uptime_secs: number
  error?: string
}

export interface AuditEntry {
  id: string
  at: string
  actor: string
  action: string
  target: string
  detail?: string
}

export interface ShareSettings {
  enabled: boolean
  description?: string
}

export interface ShareToken {
  id: string
  label?: string
  created_at: string
  last_used_at?: string
}

export interface Theme {
  short: string
  name: string
  description?: string
  active: boolean
}

export interface AuthSession {
  user: string
  csrf_token: string
}

export interface ActionResponse {
  task_id: string
  status: TaskStatus
}
