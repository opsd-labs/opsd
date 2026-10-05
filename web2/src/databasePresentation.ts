import { formatBytes, formatUptime } from "./resourcePresentation";

/**
 * 数据库只读运维视图的展示规则。
 *
 * 这一页最重要的一条约束是：**未知不等于 0，也不等于正常**。
 * 因此所有取值都必须经过这里的函数，它们对缺失值统一返回「未知」，
 * 而不是让 `0`、空串或 `undefined` 直接落到界面上。
 */

/** 未采集到时的统一文本。界面与测试都以它为唯一口径。 */
export const UNKNOWN = "未知";

/** 数值。缺失显示「未知」，绝不出 0。 */
export function dbValue(raw: number | null | undefined, suffix = ""): string {
  return raw === null || raw === undefined ? UNKNOWN : `${raw}${suffix}`;
}

/** 布尔事实。缺失显示「未知」，不默认成「否」。 */
export function dbOnOff(raw: boolean | null | undefined): string {
  return raw === null || raw === undefined ? UNKNOWN : raw ? "是" : "否";
}

/** 时长。缺失显示「未知」，不显示 0 分。 */
export function dbDuration(seconds: number | null | undefined): string {
  return seconds === null || seconds === undefined
    ? UNKNOWN
    : formatUptime(seconds);
}

/** 「N 前」：备份新鲜度、采集时刻都用它。0 或缺失一律「未知」。 */
export function dbSince(
  at: number | null | undefined,
  nowSeconds = Math.floor(Date.now() / 1000),
): string {
  if (!at) return UNKNOWN;
  // 夹到 0：节点与主控时钟有偏差时不该出现「-5 分前」
  const age = Math.max(0, nowSeconds - at);
  // 一分钟以内说「刚刚」，比「0 分前」更贴近事实
  return age < 60 ? "刚刚" : `${dbDuration(age)}前`;
}

/** 「N 后」：定时器下次触发。 */
export function dbUntil(
  at: number | null | undefined,
  nowSeconds = Math.floor(Date.now() / 1000),
): string {
  if (!at) return UNKNOWN;
  const remaining = Math.max(0, at - nowSeconds);
  return remaining < 60 ? "即将触发" : `${dbDuration(remaining)}后`;
}

/** 体积。缺失显示「未知」，不显示 0 B。 */
export function dbBytes(raw: number | null | undefined): string {
  return raw === null || raw === undefined ? UNKNOWN : formatBytes(raw);
}

/** 延迟：ProxySQL 给的是微秒，界面用毫秒。缺失显示「未知」，不补 0。 */
export function dbLatency(raw: number | null | undefined): string {
  return raw === null || raw === undefined
    ? UNKNOWN
    : `${(raw / 1000).toFixed(1)} ms`;
}

/** 流控暂停比例。缺失显示「未知」，不显示 0.0000。 */
export function dbRatio(raw: number | null | undefined): string {
  return raw === null || raw === undefined ? UNKNOWN : raw.toFixed(4);
}

/** 结论语气到界面用词的映射。 */
export function dbToneText(tone: string): string {
  if (tone === "critical") return "需要立即处理";
  if (tone === "warning") return "需要注意";
  return "未发现异常";
}

/** 结论语气到状态色。未知不在其中——它由调用方单独表达。 */
export function dbToneColor(tone: string): "success" | "warning" | "danger" {
  if (tone === "critical") return "danger";
  if (tone === "warning") return "warning";
  return "success";
}

/** 归档头检查结果的用词。`null` 表示没检查过，与「不是 age」严格区分。 */
export function dbAgeHeader(raw: boolean | null | undefined): string {
  if (raw === null || raw === undefined) return UNKNOWN;
  return raw ? "正常" : "不是 age 格式";
}

/** binlog 位点：未开启 binlog 是「没有」，不是错误。 */
export function dbBinlog(
  file: string | null | undefined,
  position: number | null | undefined,
): string {
  if (!file) return "未开启 binlog";
  return `${file}:${dbValue(position)}`;
}
