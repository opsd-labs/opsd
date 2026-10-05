import type { components } from './generated/api';

type FreeJson = any;

export type Node = Omit<components['schemas']['Node'], 'inventory'> & {
  inventory?: FreeJson;
};
export type Entry = Omit<components['schemas']['NodeEntry'], 'node'> & {
  node: Node;
};
export type Task = Omit<components['schemas']['TaskEnvelope'], 'result'> & {
  result?: FreeJson;
};

let csrf = '';
export function setCsrf(value: string) { csrf = value; }
export function getCsrf() { return csrf; }

export function basePath() {
  const segment = location.pathname.split('/')[1] || '';
  return segment ? `/${segment}` : '';
}

export function apiPath(path: string) {
  return `${basePath()}/api/v1${path}`;
}

export async function api(path: string, method = 'GET', body?: unknown) {
  const response = await fetch(apiPath(path), {
    method,
    headers: {
      'Content-Type': 'application/json',
      'X-CSRF-Token': csrf,
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const data = await response.json().catch(
    () => ({ error: `服务返回异常状态 ${response.status}` }),
  );
  if (!response.ok) throw new Error(data.error || `请求失败：${response.status}`);
  return data;
}

export async function submit(node: string, action: unknown) {
  return api(`/nodes/${node}/actions`, 'POST', {
    idempotency_key: crypto.randomUUID(),
    action,
  });
}

export const statusNames: Record<string, string> = {
  pending: '等待下发',
  accepted: '节点已接收',
  running: '正在执行',
  validating: '验证中',
  succeeded: '已完成',
  failed: '失败',
  uncertain: '结果待核实',
  rollback_pending: '等待回滚',
  rolled_back: '已回滚',
  blocked: '已阻止',
  cancelled: '已取消',
};

export function formatTime(timestamp: number) {
  return timestamp
    ? new Date(timestamp * 1000).toLocaleString('zh-CN', { hour12: false })
    : '尚未采集';
}
