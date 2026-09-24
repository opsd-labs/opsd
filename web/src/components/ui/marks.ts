/** 状态符号词汇表。列表与详情共用，避免各处自造标记。 */
export type StatusMark =
  | "active" // 运行中 / 在线 / 已同步 / 启用
  | "paused" // 已停止 / 停用
  | "pending" // 离线快照 / 待同步
  | "failed" // 失败 / 冲突 / 读取失败
  | "unknown" // 未知，既不算停止也不算成功
  | "blocked" // 中性阻断：丢弃 / 拒绝这类预期策略
  | "neutral"; // 中性事实，如"已发现"

export type StatusTone = "neutral" | "success" | "warning" | "danger";

/** 语义色到符号的默认映射；调用方可用 mark 覆盖。 */
export const defaultMark: Record<StatusTone, StatusMark> = {
  success: "active",
  warning: "pending",
  danger: "failed",
  neutral: "neutral",
};
