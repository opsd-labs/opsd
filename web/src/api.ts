import type { components } from './generated/api';
/**
 * 对外类型取自生成的 OpenAPI 契约（web/src/generated/api.ts），不再手写第二份。
 * 契约源文件是 docs/api/openapi.yaml；改接口先改契约，再跑 npm run api:generate。
 */

/**
 * 主机盘点（inventory）与任务结果（result）在 Rust 侧是 Option<Value>，契约如实表达为
 * 「不约束字段的对象」，所以生成类型只能给到 unknown。界面要按已知形状逐层读取
 * （inventory.firewall.data.policy.rules 这类），因此在门面这一处放宽为 any。
 * 这是当前唯一的放宽点；等这两个字段在 Rust 侧收敛成命名 DTO，就可以删掉它。
 */
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
export type EnrollmentSummary = components['schemas']['EnrollmentSummary'];
export type EnrollmentToken = components['schemas']['EnrollmentToken'];
export type InstallMode = components['schemas']['AgentInstallMode'];
let csrf='';
export function setCsrf(value:string){csrf=value;}
export function getCsrf(){return csrf;}
/**
 * 控制台整体挂在 /{安全入口}/ 之下，静态资源与 API 共用这一层前缀。
 * 入口是路径的第一段，因此从当前地址推导即可，不需要额外配置。
 * 开发服务器没有入口，此时返回空串，由 vite 代理补上前缀。
 */
export function basePath(){
  const segment=location.pathname.split('/')[1]||'';
  return segment?`/${segment}`:'';
}
/** 把 API 路径补上入口前缀。所有请求（含 SSE 与 WebSocket）都必须经过这里。 */
export function apiPath(path:string){return `${basePath()}/api/v1${path}`;}
export async function api(path:string,method='GET',body?:unknown){
  const response=await fetch(apiPath(path),{method,headers:{'Content-Type':'application/json','X-CSRF-Token':csrf},body:body===undefined?undefined:JSON.stringify(body)});
  const data=await response.json().catch(()=>({error:`服务返回异常状态 ${response.status}`}));
  if(!response.ok)throw new Error(data.error||`请求失败：${response.status}`);return data;
}
export async function submit(node:string,action:unknown){return api(`/nodes/${node}/actions`,'POST',{idempotency_key:crypto.randomUUID(),action});}
export const statusNames:Record<string,string>={pending:'等待下发',accepted:'节点已接收',running:'正在执行',validating:'验证中',succeeded:'已完成',failed:'失败',uncertain:'结果待核实',rollback_pending:'等待回滚',rolled_back:'已回滚',blocked:'已阻止',cancelled:'已取消'};
export function formatTime(timestamp:number){return timestamp?new Date(timestamp*1000).toLocaleString('zh-CN',{hour12:false}):'尚未采集';}
