import type { StatusMark, StatusTone } from "./components/ui/marks";

export const resourceLabels: Record<string, string> = {
  id: "资源编号",
  name: "名称",
  names: "名称",
  node_name: "所属节点",
  node_id: "节点编号",
  image: "镜像",
  state: "状态",
  status: "状态",
  project: "项目",
  directory: "工作目录",
  revision: "配置修订",
  protected: "保护资源",
  ports: "发布端口",
  mounts: "挂载",
  networks: "网络",
  restart_policy: "重启策略",
  stats: "资源使用",
  error: "错误",
  task_id: "任务编号",
  sequence: "事件序号",
  created_at: "创建时间",
  updated_at: "更新时间",
  fingerprint: "文件指纹",
  files: "配置文件",
  env_files: "环境文件",
  result: "执行结果",
  before: "变更前",
  after: "变更后",
  services: "服务",
  plan_id: "计划编号",
  expires: "有效期",
  content: "配置内容",
};

/* ------------------------------------------------------------------ *
 * 状态
 *
 * 列表与详情共用同一判定，避免同一资源在两处显示不同状态。
 * 判定优先级固定为：离线快照 > 未知 > 具体状态。
 * 「未知」必须是独立态，不得归入「已停止」或成功。
 * ------------------------------------------------------------------ */

export type ResourceKind =
  | "container"
  | "rule"
  | "stack"
  | "node"
  | "sync"
  | "result";

export type StatusDescriptor = {
  text: string;
  tone: StatusTone;
  mark: StatusMark;
};

export const UNKNOWN_STATUS: StatusDescriptor = {
  text: "未知",
  tone: "neutral",
  mark: "unknown",
};

/** 离线时统一显示快照语义，不把最后采集值当作当前值。 */
export const OFFLINE_STATUS: StatusDescriptor = {
  text: "离线快照",
  tone: "warning",
  mark: "pending",
};

/** 地址同步在离线时既要表达快照，也要保留「仍待同步」的含义。 */
export const OFFLINE_SYNC_STATUS: StatusDescriptor = {
  text: "离线 · 待同步",
  tone: "warning",
  mark: "pending",
};

const results: Record<string, StatusDescriptor> = {
  已验证: { text: "已验证", tone: "success", mark: "active" },
  无冲突: { text: "无冲突", tone: "success", mark: "active" },
  已同步: { text: "已同步", tone: "success", mark: "active" },
  待同步: { text: "待同步", tone: "warning", mark: "pending" },
  失败: { text: "失败", tone: "danger", mark: "failed" },
  读取失败: { text: "读取失败", tone: "danger", mark: "failed" },
  回滚失败: { text: "回滚失败", tone: "danger", mark: "failed" },
  冲突: { text: "冲突", tone: "danger", mark: "failed" },
  只读发现: { text: "只读发现", tone: "neutral", mark: "neutral" },
  仅本地修改: { text: "仅本地修改", tone: "neutral", mark: "neutral" },
};

/** 任务与发布结果的状态表达。 */
export function resultStatus(result?: string | null): StatusDescriptor {
  if (!result) return UNKNOWN_STATUS;
  return results[result] || { text: result, tone: "neutral", mark: "unknown" };
}

const containerStates: Record<string, StatusDescriptor> = {
  running: { text: "运行中", tone: "success", mark: "active" },
  exited: { text: "已停止", tone: "neutral", mark: "paused" },
  paused: { text: "已暂停", tone: "neutral", mark: "paused" },
  created: { text: "已创建", tone: "neutral", mark: "paused" },
  restarting: { text: "重启中", tone: "warning", mark: "pending" },
  dead: { text: "异常", tone: "danger", mark: "failed" },
};

export type StatusInput = {
  kind: ResourceKind;
  /** 节点离线或场景级离线；离线优先于一切具体状态。 */
  offline?: boolean;
  /** 尚未采集、读取失败或能力缺失；不得当作空资源或成功。 */
  unknown?: boolean;
  /** 容器 state 原值。 */
  state?: string | null;
  /** 规则是否启用、容器是否运行等布尔事实；null 表示未上报。 */
  enabled?: boolean | null;
  /** 结果类文本，仅 kind 为 result 时使用。 */
  result?: string | null;
};

export function resourceStatus(input: StatusInput): StatusDescriptor {
  const { kind, offline, unknown, state, enabled, result } = input;
  if (kind === "result") return resultStatus(result);
  if (offline && kind === "sync") return OFFLINE_SYNC_STATUS;
  if (offline) return OFFLINE_STATUS;
  if (unknown) return UNKNOWN_STATUS;
  switch (kind) {
    case "node":
      return enabled === true
        ? { text: "在线", tone: "success", mark: "active" }
        : enabled === false
          ? { text: "离线", tone: "warning", mark: "pending" }
          : UNKNOWN_STATUS;
    case "container":
      return (state && containerStates[state]) || UNKNOWN_STATUS;
    case "rule":
      return enabled === true
        ? { text: "启用", tone: "success", mark: "active" }
        : enabled === false
          ? { text: "停用", tone: "neutral", mark: "paused" }
          : UNKNOWN_STATUS;
    case "stack":
      return { text: "已发现", tone: "neutral", mark: "neutral" };
    case "sync":
      return enabled === true
        ? { text: "已同步", tone: "success", mark: "active" }
        : enabled === false
          ? { text: "待同步", tone: "warning", mark: "pending" }
          : UNKNOWN_STATUS;
    default:
      return UNKNOWN_STATUS;
  }
}

/* ------------------------------------------------------------------ *
 * 归属
 *
 * Docker 自有对象与外部维护来源必须与 opsd 受管对象区分开；
 * 非受管来源一律显式标注只读，避免误以为可以改写。
 * ------------------------------------------------------------------ */

export function ownership(owner?: string | null): string {
  if (!owner) return "维护来源未知";
  if (owner === "Docker") return "Docker · 系统";
  if (owner === "opsd") return "opsd";
  return `${owner} · 只读`;
}

export function isReadOnly(owner?: string | null): boolean {
  return !!owner && owner !== "opsd";
}

/* ------------------------------------------------------------------ *
 * 端口
 *
 * 解析失败必须保留原文，不得伪造结构化映射；原文同时用于搜索与诊断。
 * ------------------------------------------------------------------ */

export type PortMapping = {
  address?: string;
  host: string;
  container: string;
  protocol: string;
};

const IPV4 = String.raw`\d{1,3}(?:\.\d{1,3}){3}`;
const IPV6 = String.raw`\[[0-9a-fA-F:]+\]`;
const PORT = String.raw`\d{1,5}(?:[-–]\d{1,5})?`;
const MAPPING = new RegExp(
  `^(?:(${IPV4}|${IPV6}):)?(${PORT})\\s*(?:->|→|:)\\s*(${PORT})/(tcp|udp|sctp)$`,
  "i",
);

export function isPortRange(value: string): boolean {
  const parts = value.split(/[-–]/).map(Number);
  if (parts.some((n) => !Number.isInteger(n) || n < 1 || n > 65535)) return false;
  return parts.length === 1 || parts[0] <= parts[1];
}

function validAddress(address: string): boolean {
  if (address.startsWith("[")) return true;
  return address.split(".").every((n) => Number(n) <= 255);
}

/** 解析 Docker 发布端口。任一项无法解析则整体返回 null，由调用方保留原文。 */
export function parseDockerPorts(raw?: string | null): PortMapping[] | null {
  const value = String(raw ?? "").trim();
  if (!value) return [];
  const items = value.split(/\s*[;,，；]\s*/).filter(Boolean);
  const parsed: PortMapping[] = [];
  for (const item of items) {
    const match = item.match(MAPPING);
    if (!match) return null;
    const [, address, host, container, protocol] = match;
    if (!isPortRange(host) || !isPortRange(container)) return null;
    if (address && !validAddress(address)) return null;
    parsed.push({ address, host, container, protocol: protocol.toLowerCase() });
  }
  return parsed;
}

/** 标注为未发布的容器端口，例如 `80/tcp（未发布）`。 */
export function parseUnpublishedPorts(
  raw?: string | null,
): { port: string; protocol: string }[] | null {
  const value = String(raw ?? "").trim();
  if (!value) return [];
  const items = value.split(/\s*[;,，；]\s*/).filter(Boolean);
  const parsed: { port: string; protocol: string }[] = [];
  for (const item of items) {
    const match = item.match(
      new RegExp(`^(${PORT})/(tcp|udp|sctp)（未发布）$`, "i"),
    );
    if (!match || !isPortRange(match[1])) return null;
    parsed.push({ port: match[1], protocol: match[2].toLowerCase() });
  }
  return parsed;
}

/** 防火墙协议：`any` 统一显示为「全部」，其余大写。 */
export function protocolLabel(value?: string | null): string {
  const text = String(value ?? "").trim();
  if (!text) return "全部";
  return /^any$/i.test(text) ? "全部" : text.toUpperCase();
}

/** 防火墙端口：端口为主，协议为辅助标识。 */
export function firewallPorts(ports?: (number | string)[] | null): string {
  if (!ports || !ports.length) return "全部";
  return ports.join(",");
}

/* ------------------------------------------------------------------ *
 * 指标
 *
 * 「未上报」「尚未上报」「采集失败」「数据陈旧」是四种不同的情况，
 * 必须分开表达；任何一种都不能退化成 0，也不能互相冒充。
 * ------------------------------------------------------------------ */

export type MetricsState =
  | "unsupported"
  | "pending"
  | "error"
  | "stale"
  | "ok";

export type MetricsDescriptor = {
  state: MetricsState;
  text: string;
  tone: StatusTone;
  mark: StatusMark;
};

/** 超过该秒数没有新采样即视为陈旧，约等于三个采样周期。 */
export const METRICS_STALE_SECONDS = 60;

export function metricsStatus(
  node: { capabilities?: { metrics?: boolean } | null } | null | undefined,
  record: { received_at?: number; sample?: unknown; error?: string | null } | null | undefined,
  nowSeconds: number,
): MetricsDescriptor {
  if (!node?.capabilities?.metrics)
    return {
      state: "unsupported",
      text: "未上报指标",
      tone: "neutral",
      mark: "unknown",
    };
  if (!record)
    return {
      state: "pending",
      text: "尚未上报",
      tone: "neutral",
      mark: "unknown",
    };
  if (record.error || !record.sample)
    return {
      state: "error",
      text: "采集失败",
      tone: "danger",
      mark: "failed",
    };
  const age = nowSeconds - (record.received_at || 0);
  if (age > METRICS_STALE_SECONDS)
    return {
      state: "stale",
      text: "数据陈旧",
      tone: "warning",
      mark: "pending",
    };
  return { state: "ok", text: "正常", tone: "success", mark: "active" };
}

/** 时钟偏移是否已大到会让曲线时间轴错位。 */
export function clockSkewExceeded(
  record: { clock_offset?: number } | null | undefined,
  threshold: number,
): boolean {
  return !!record && Math.abs(record.clock_offset || 0) > threshold;
}

const UNITS = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];

/** 人类可读的字节数。负值与非法输入返回「—」，不显示 0。 */
export function formatBytes(value?: number | null): string {
  if (value == null || !Number.isFinite(value) || value < 0) return "—";
  if (value === 0) return "0 B";
  let size = value;
  let unit = 0;
  while (size >= 1024 && unit < UNITS.length - 1) {
    size /= 1024;
    unit += 1;
  }
  const digits = size >= 100 || unit === 0 ? 0 : 1;
  return `${size.toFixed(digits)} ${UNITS[unit]}`;
}

/** 百分比。缺值返回「—」而不是 0。 */
export function formatPercent(value?: number | null): string {
  if (value == null || !Number.isFinite(value)) return "—";
  return `${value.toFixed(1)}%`;
}

/** 使用率：已用 / 总量。总量为 0 时返回 null，避免除零后显示 0%。 */
export function usagePercent(
  used?: number | null,
  total?: number | null,
): number | null {
  if (used == null || total == null || total <= 0) return null;
  return Math.min(100, Math.max(0, (used / total) * 100));
}

/** 运行时长，例如「12 天 3 小时」。 */
export function formatUptime(seconds?: number | null): string {
  if (seconds == null || !Number.isFinite(seconds) || seconds <= 0) return "—";
  const days = Math.floor(seconds / 86400);
  const hours = Math.floor((seconds % 86400) / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  if (days) return `${days} 天 ${hours} 小时`;
  if (hours) return `${hours} 小时 ${minutes} 分`;
  return `${minutes} 分`;
}

/** 按区间与目标点数挑选合适的时间步长（秒）。 */
export function chooseStep(spanSeconds: number, targetPoints = 240): number {
  // 候选值同时要落在后端层级边界上：<300 用原始数据，<3600 用 5 分钟桶，其余用小时桶。
  const candidates = [
    15, 30, 60, 300, 600, 900, 1800, 3600, 7200, 21600, 43200, 86400,
  ];
  const ideal = spanSeconds / Math.max(1, targetPoints);
  return candidates.find((c) => c >= ideal) || candidates[candidates.length - 1];
}

/**
 * 取出可用的数值。**必须显式排除 null、undefined 与空串**：
 * `Number(null)` 是 0，只靠 `Number.isFinite` 会把缺失值当成真实的零。
 * 这是「缺失不补零」这条约束的落点，图表与统计都要走这里。
 */
export function finiteNumber(raw: unknown): number | null {
  if (raw === null || raw === undefined || raw === "") return null;
  const value = Number(raw);
  return Number.isFinite(value) ? value : null;
}

/**
 * 把曲线点切成连续段。相邻点间隔超过两倍步长即视为断档，段与段之间不连线。
 * 这样离线时段会表现为真实的空隙，而不是被补成一条平线或零点。
 * 无法解析的取值直接跳过，不会变成 0。
 */
export function segmentSeries<T extends { at: number }>(
  points: T[],
  field: string,
  step: number,
): { at: number; value: number }[][] {
  const threshold = Math.max(step, 1) * 2;
  const segments: { at: number; value: number }[][] = [];
  let current: { at: number; value: number }[] = [];
  for (const point of points) {
    const value = finiteNumber((point as Record<string, unknown>)[field]);
    if (value === null) continue;
    const previous = current[current.length - 1];
    if (previous && point.at - previous.at > threshold) {
      segments.push(current);
      current = [];
    }
    current.push({ at: point.at, value });
  }
  if (current.length) segments.push(current);
  return segments;
}

/** 序列里的有效数值，供图表的极值统计使用。 */
export function finiteSeries<T extends { at: number }>(
  points: T[],
  field: string,
): number[] {
  return points
    .map((point) => finiteNumber((point as Record<string, unknown>)[field]))
    .filter((value): value is number => value !== null);
}
