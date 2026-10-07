<script setup lang="ts">
/**
 * ConsolePage — 多节点总览
 * 显示所有节点卡片，异常聚合，指标概览
 */
import { computed } from 'vue'
import { workspace } from '../state/workspace'
import { formatPercent, formatUptime } from '../utils/format'
import Status from '../components/ui/Status.vue'
import Skeleton from '../components/ui/Skeleton.vue'
import type { StatusTone } from '../components/ui/Status.vue'

const nodes = computed(() => workspace.state.nodes)
const metrics = computed(() => workspace.state.metrics)
const loading = computed(() => workspace.state.loading)

function nodeStatus(nodeId: string): { tone: StatusTone; label: string } {
  const node = nodes.value.find(n => n.id === nodeId)
  if (!node) return { tone: 'unknown', label: '未知' }
  if (!node.connected) return { tone: 'danger', label: '离线' }
  const m = metrics.value[nodeId]
  if (!m || m.error) return { tone: 'warning', label: '指标异常' }
  if (m.cpu_percent > 90 || m.mem_percent > 90) return { tone: 'warning', label: '高负载' }
  return { tone: 'success', label: '正常' }
}

function goToNode(nodeId: string) {
  workspace.navigate('nodes', nodeId)
}
</script>

<template>
  <div class="console-page">
    <div class="console-page__header">
      <span class="console-page__count">{{ nodes.length }} 个节点</span>
    </div>

    <!-- 骨架加载 -->
    <div v-if="loading && nodes.length === 0" class="nodes-grid">
      <div v-for="i in 4" :key="i" class="node-card node-card--skeleton">
        <Skeleton height="18px" width="60%" />
        <Skeleton height="13px" width="40%" />
        <div style="display:flex;gap:8px;margin-top:8px">
          <Skeleton height="36px" style="flex:1" />
          <Skeleton height="36px" style="flex:1" />
          <Skeleton height="36px" style="flex:1" />
        </div>
      </div>
    </div>

    <!-- 节点卡片网格 -->
    <div v-else class="nodes-grid">
      <button
        v-for="node in nodes"
        :key="node.id"
        class="node-card"
        :class="{ 'node-card--offline': !node.connected }"
        @click="goToNode(node.id)"
      >
        <div class="node-card__header">
          <span class="node-card__name">{{ node.name }}</span>
          <Status v-bind="nodeStatus(node.id)" />
        </div>

        <div class="node-card__address">{{ node.address }}</div>

        <!-- 指标行 -->
        <template v-if="node.connected && metrics[node.id] && !metrics[node.id].error">
          <div class="node-card__metrics">
            <div class="metric-item">
              <span class="metric-item__label">CPU</span>
              <span class="metric-item__value">
                {{ formatPercent(metrics[node.id].cpu_percent) }}
              </span>
              <div class="metric-bar">
                <div
                  class="metric-bar__fill"
                  :class="metrics[node.id].cpu_percent > 80 ? 'metric-bar__fill--warn' : ''"
                  :style="{ width: `${Math.min(metrics[node.id].cpu_percent, 100)}%` }"
                />
              </div>
            </div>
            <div class="metric-item">
              <span class="metric-item__label">内存</span>
              <span class="metric-item__value">
                {{ formatPercent(metrics[node.id].mem_percent) }}
              </span>
              <div class="metric-bar">
                <div
                  class="metric-bar__fill"
                  :class="metrics[node.id].mem_percent > 85 ? 'metric-bar__fill--warn' : ''"
                  :style="{ width: `${Math.min(metrics[node.id].mem_percent, 100)}%` }"
                />
              </div>
            </div>
            <div class="metric-item">
              <span class="metric-item__label">磁盘</span>
              <span class="metric-item__value">
                {{ formatPercent(metrics[node.id].disk_percent) }}
              </span>
              <div class="metric-bar">
                <div
                  class="metric-bar__fill"
                  :class="metrics[node.id].disk_percent > 90 ? 'metric-bar__fill--warn' : ''"
                  :style="{ width: `${Math.min(metrics[node.id].disk_percent, 100)}%` }"
                />
              </div>
            </div>
          </div>
          <div class="node-card__uptime">
            运行时间 {{ formatUptime(metrics[node.id].uptime_secs) }}
          </div>
        </template>

        <div v-else-if="!node.connected" class="node-card__offline-msg">
          节点离线，无法获取指标
        </div>
      </button>

      <!-- 空状态 -->
      <div v-if="nodes.length === 0" class="console-empty">
        <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor"
             stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <rect x="2" y="3" width="20" height="14" rx="2"/>
          <path d="M8 21h8M12 17v4"/>
        </svg>
        <span>尚无节点，请前往设置添加注册令牌</span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.console-page {
  display: flex;
  flex-direction: column;
  gap: var(--sp-4);
}

.console-page__header {
  display: flex;
  align-items: center;
  gap: var(--sp-3);
}

.console-page__count {
  font-size: var(--text-sm);
  color: var(--color-muted);
}

/* ---- 网格 ---- */
.nodes-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
  gap: var(--sp-4);
}

/* ---- 节点卡片 ---- */
.node-card {
  display: flex;
  flex-direction: column;
  gap: var(--sp-3);
  padding: var(--sp-4);
  background: var(--color-surface);
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-sm);
  cursor: pointer;
  text-align: left;
  transition:
    box-shadow var(--duration-fast) var(--ease-out),
    border-color var(--duration-fast) var(--ease-out);
}

.node-card:hover {
  box-shadow: var(--shadow-md);
  border-color: var(--color-border);
}

.node-card:focus-visible {
  outline: none;
  box-shadow: var(--shadow-focus);
}

.node-card--offline {
  opacity: 0.7;
}

.node-card--skeleton {
  cursor: default;
  gap: var(--sp-2);
}

.node-card__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--sp-2);
}

.node-card__name {
  font-size: var(--text-md);
  font-weight: var(--weight-semibold);
  color: var(--color-ink);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.node-card__address {
  font-size: var(--text-sm);
  color: var(--color-muted);
  font-family: var(--font-mono);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* ---- 指标 ---- */
.node-card__metrics {
  display: flex;
  flex-direction: column;
  gap: var(--sp-2);
}

.metric-item {
  display: grid;
  grid-template-columns: 40px 1fr 80px;
  align-items: center;
  gap: var(--sp-2);
}

.metric-item__label {
  font-size: var(--text-xs);
  color: var(--color-muted);
  font-weight: var(--weight-medium);
}

.metric-item__value {
  font-size: var(--text-xs);
  color: var(--color-secondary);
  font-family: var(--font-mono);
  text-align: right;
  white-space: nowrap;
}

.metric-bar {
  height: 4px;
  background: var(--color-border-subtle);
  border-radius: var(--radius-full);
  overflow: hidden;
}

.metric-bar__fill {
  height: 100%;
  background: var(--color-accent);
  border-radius: var(--radius-full);
  transition: width var(--duration-base) var(--ease-out);
}

.metric-bar__fill--warn {
  background: var(--color-warning);
}

.node-card__uptime {
  font-size: var(--text-xs);
  color: var(--color-muted);
}

.node-card__offline-msg {
  font-size: var(--text-sm);
  color: var(--color-muted);
  font-style: italic;
}

/* ---- 空状态 ---- */
.console-empty {
  grid-column: 1 / -1;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--sp-3);
  padding: var(--sp-12);
  color: var(--color-muted);
  font-size: var(--text-sm);
}
</style>
