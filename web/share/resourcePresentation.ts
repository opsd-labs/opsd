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
