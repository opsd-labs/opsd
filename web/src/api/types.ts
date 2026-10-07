/**
 * opsd web3 — 业务类型别名
 * 从 openapi-typescript 生成的 generated.ts 重导出
 * 运行 `npm run api:generate` 更新 generated.ts
 */

export type { components } from './generated'

// 常用类型别名，供组件直接引用
import type { components } from './generated'

export type Session              = components['schemas']['Session']
export type Node                 = components['schemas']['Node']
export type NodeEntry            = components['schemas']['NodeEntry']
export type NodeCapabilities     = components['schemas']['NodeCapabilities']
export type MetricsOverviewNode  = components['schemas']['MetricsOverviewNode']
export type MetricsSample        = components['schemas']['MetricsSample']
export type EnrollmentSummary    = components['schemas']['EnrollmentSummary']
export type EnrollmentToken      = components['schemas']['EnrollmentToken']
export type ThemeDescribe        = components['schemas']['ThemeDescribe']
export type ThemeActive          = components['schemas']['ThemeActive']
