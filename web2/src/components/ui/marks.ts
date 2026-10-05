export type StatusMark =
  | "active"
  | "paused"
  | "pending"
  | "failed"
  | "unknown"
  | "blocked"
  | "neutral";

export type StatusTone = "neutral" | "success" | "warning" | "danger";

export const defaultMark: Record<StatusTone, StatusMark> = {
  success: "active",
  warning: "pending",
  danger: "failed",
  neutral: "neutral",
};
