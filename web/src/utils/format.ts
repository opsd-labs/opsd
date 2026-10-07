/**
 * opsd web3 — 格式化工具函数
 */

/** 字节转可读字符串（B / KB / MB / GB / TB） */
export function formatBytes(bytes: number, decimals = 1): string {
  if (bytes === 0) return '0 B'
  const k = 1024
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB', 'PB']
  const i = Math.floor(Math.log(bytes) / Math.log(k))
  return `${parseFloat((bytes / Math.pow(k, i)).toFixed(decimals))} ${sizes[i]}`
}

/** 字节每秒转可读字符串 */
export function formatBytesPerSec(bps: number): string {
  return `${formatBytes(bps)}/s`
}

/** 百分比，保留 1 位小数 */
export function formatPercent(value: number, decimals = 1): string {
  return `${value.toFixed(decimals)}%`
}

/** 秒数转运行时间字符串（如 3d 14h 22m） */
export function formatUptime(secs: number): string {
  if (secs < 60) return `${Math.floor(secs)}s`
  const days = Math.floor(secs / 86400)
  const hours = Math.floor((secs % 86400) / 3600)
  const mins = Math.floor((secs % 3600) / 60)
  const parts: string[] = []
  if (days > 0) parts.push(`${days}d`)
  if (hours > 0) parts.push(`${hours}h`)
  if (mins > 0 && days === 0) parts.push(`${mins}m`)
  return parts.join(' ')
}

/** 时间戳转相对时间（如"3 分钟前"） */
export function formatRelativeTime(isoString: string): string {
  const diff = (Date.now() - new Date(isoString).getTime()) / 1000
  if (diff < 5) return '刚刚'
  if (diff < 60) return `${Math.floor(diff)} 秒前`
  if (diff < 3600) return `${Math.floor(diff / 60)} 分钟前`
  if (diff < 86400) return `${Math.floor(diff / 3600)} 小时前`
  return `${Math.floor(diff / 86400)} 天前`
}

/** 时间戳转本地日期时间字符串 */
export function formatDateTime(isoString: string): string {
  return new Date(isoString).toLocaleString('zh-CN', {
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  })
}

/** Load average 格式化 */
export function formatLoad(load: number): string {
  return load.toFixed(2)
}

/** 生成幂等键（随机 hex 字符串） */
export function generateIdempotencyKey(): string {
  return Array.from(crypto.getRandomValues(new Uint8Array(8)))
    .map(b => b.toString(16).padStart(2, '0'))
    .join('')
}
