<script setup lang="ts">
/**
 * NodesPage — 单节点深度视图
 * 指标概览（后续扩展为曲线图 + 文件管理 + 宿主机 Shell）
 * MetricsOverviewNode.sample = MetricsSample | null
 */
import { computed } from 'vue'
import { workspace } from '../state/workspace'
import { formatPercent, formatBytes, formatUptime } from '../utils/format'
import Status from '../components/ui/Status.vue'

const entry = computed(() => workspace.selectedEntry.value)
const node = computed(() => entry.value?.node ?? null)
const metricsNode = computed(() =>
  node.value ? workspace.state.metrics[node.value.id] : null
)
// 便捷访问 sample 内层（避免模板里反复 .sample?.）
const m = computed(() => metricsNode.value?.sample ?? null)

function nodeAddr(): string {
  if (!node.value) return '—'
  return node.value.overlay_address ?? node.value.public_addresses[0] ?? '—'
}

const memPercent = computed(() => {
  if (!m.value || m.value.memory_total === 0) return null
  return (m.value.memory_used / m.value.memory_total) * 100
})

const diskPercent = computed(() => {
  const disk = m.value?.disks?.[0]
  if (!disk || disk.total === 0) return null
  return (disk.used / disk.total) * 100
})
</script>

<template>
  <div class="nodes-page">
    <div v-if="!node" class="nodes-page__empty">
      <span>请选择一个节点</span>
    </div>

    <template v-else>
      <!-- 节点头部 -->
      <div class="node-header">
        <div class="node-header__info">
          <h2 class="node-header__name">{{ node.name }}</h2>
          <code class="node-header__addr">{{ nodeAddr() }}</code>
        </div>
        <Status :tone="entry!.connected ? 'success' : 'danger'"
                :label="entry!.connected ? '在线' : '离线'" />
      </div>

      <!-- 指标卡片 -->
      <div v-if="m" class="metrics-grid">
        <div class="metric-card">
          <span class="metric-card__label">CPU</span>
          <span class="metric-card__value">{{ formatPercent(m.cpu_usage) }}</span>
        </div>
        <div class="metric-card">
          <span class="metric-card__label">内存</span>
          <span class="metric-card__value">{{ formatPercent(memPercent) }}</span>
          <span class="metric-card__sub">{{ formatBytes(m.memory_used) }} / {{ formatBytes(m.memory_total) }}</span>
        </div>
        <div class="metric-card">
          <span class="metric-card__label">磁盘</span>
          <span class="metric-card__value">{{ formatPercent(diskPercent) }}</span>
          <template v-if="m.disks[0]">
            <span class="metric-card__sub">{{ formatBytes(m.disks[0].used) }} / {{ formatBytes(m.disks[0].total) }}</span>
          </template>
        </div>
        <div class="metric-card">
          <span class="metric-card__label">运行时间</span>
          <span class="metric-card__value">{{ formatUptime(m.uptime) }}</span>
        </div>
      </div>

      <div v-else-if="!entry!.connected" class="nodes-page__offline">
        节点离线，指标不可用
      </div>
      <div v-else class="nodes-page__offline">{{ metricsNode?.error || '尚未采集指标' }}</div>
    </template>
  </div>
</template>

<style scoped>
.nodes-page { display: flex; flex-direction: column; gap: var(--sp-5); }
.nodes-page__empty, .nodes-page__offline {
  color: var(--color-muted); font-size: var(--text-sm);
  padding: var(--sp-8); text-align: center;
}

.node-header {
  display: flex; align-items: center;
  justify-content: space-between; gap: var(--sp-4);
}
.node-header__info { display: flex; flex-direction: column; gap: var(--sp-1); }
.node-header__name { font-size: var(--text-xl); font-weight: var(--weight-semibold); color: var(--color-ink); }
.node-header__addr { font-size: var(--text-sm); color: var(--color-muted); font-family: var(--font-mono); }

.metrics-grid {
  display: grid; grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
  gap: var(--sp-3);
}
.metric-card {
  display: flex; flex-direction: column; gap: var(--sp-1);
  padding: var(--sp-4); background: var(--color-surface);
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-lg); box-shadow: var(--shadow-sm);
}
.metric-card__label { font-size: var(--text-xs); color: var(--color-muted); font-weight: var(--weight-medium); }
.metric-card__value { font-size: var(--text-xl); font-weight: var(--weight-bold); color: var(--color-ink); font-family: var(--font-mono); }
.metric-card__sub { font-size: var(--text-xs); color: var(--color-muted); }
</style>
