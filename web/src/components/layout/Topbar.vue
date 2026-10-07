<script setup lang="ts">
/**
 * Topbar — 顶部操作栏
 * 左：页面标题  右：节点选择器 → 活跃任务数 → 主题切换
 */
import { computed } from 'vue'
import { workspace, PAGES } from '../../state/workspace'
import { useTheme } from '../../composables/useTheme'

const { resolved, toggleTheme } = useTheme()

const page = computed(() => workspace.state.page)
const nodes = computed(() => workspace.state.nodes)
const selectedNodeId = computed(() => workspace.state.selectedNodeId)
const taskCount = computed(() => workspace.state.activeTaskCount)

const pageLabel = computed(
  () => PAGES.find(p => p.name === page.value)?.label ?? '控制台'
)

// 需要节点选择器的页面
const nodePages = new Set(['nodes', 'docker', 'firewall', 'database', 'storage'])
const showNodeSelect = computed(() => nodePages.has(page.value))

function onNodeChange(e: Event) {
  const v = (e.target as HTMLSelectElement).value
  workspace.selectNode(v || null)
}
</script>

<template>
  <header class="topbar">
    <div class="topbar__left">
      <h1 class="topbar__title">{{ pageLabel }}</h1>
    </div>

    <div class="topbar__right">
      <!-- 节点选择器（仅部分页面显示） -->
      <template v-if="showNodeSelect && nodes.length > 0">
        <select
          class="node-select"
          :value="selectedNodeId ?? nodes[0]?.id"
          aria-label="选择节点"
          @change="onNodeChange"
        >
          <option v-for="n in nodes" :key="n.id" :value="n.id">
            <span>{{ n.name }}</span>
            <span v-if="!n.connected"> (离线)</span>
          </option>
        </select>
      </template>

      <!-- 活跃任务指示器 -->
      <button
        v-if="taskCount > 0"
        class="task-indicator"
        aria-label="`${taskCount} 个任务进行中`"
        title="查看任务"
        @click="workspace.navigate('console')"
      >
        <span class="task-indicator__spinner" aria-hidden="true"></span>
        <span class="task-indicator__count">{{ taskCount }}</span>
      </button>

      <!-- 主题切换 -->
      <button
        class="icon-btn"
        :aria-label="resolved === 'dark' ? '切换为浅色主题' : '切换为深色主题'"
        :title="resolved === 'dark' ? '切换为浅色主题' : '切换为深色主题'"
        @click="toggleTheme()"
      >
        <!-- 浅色图标：sun -->
        <svg v-if="resolved === 'dark'" width="16" height="16" viewBox="0 0 24 24"
             fill="none" stroke="currentColor" stroke-width="1.75"
             stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <circle cx="12" cy="12" r="5"/>
          <line x1="12" y1="1" x2="12" y2="3"/>
          <line x1="12" y1="21" x2="12" y2="23"/>
          <line x1="4.22" y1="4.22" x2="5.64" y2="5.64"/>
          <line x1="18.36" y1="18.36" x2="19.78" y2="19.78"/>
          <line x1="1" y1="12" x2="3" y2="12"/>
          <line x1="21" y1="12" x2="23" y2="12"/>
          <line x1="4.22" y1="19.78" x2="5.64" y2="18.36"/>
          <line x1="18.36" y1="5.64" x2="19.78" y2="4.22"/>
        </svg>
        <!-- 深色图标：moon -->
        <svg v-else width="16" height="16" viewBox="0 0 24 24"
             fill="none" stroke="currentColor" stroke-width="1.75"
             stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z"/>
        </svg>
      </button>
    </div>
  </header>
</template>

<style scoped>
.topbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  height: var(--topbar-h);
  padding-inline: var(--page-pad);
  background: var(--color-surface);
  border-bottom: 1px solid var(--color-border-subtle);
  flex-shrink: 0;
  gap: var(--sp-4);
}

.topbar__left {
  display: flex;
  align-items: center;
  gap: var(--sp-3);
  min-width: 0;
}

.topbar__title {
  font-size: var(--text-lg);
  font-weight: var(--weight-semibold);
  color: var(--color-ink);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.topbar__right {
  display: flex;
  align-items: center;
  gap: var(--sp-2);
  flex-shrink: 0;
}

/* ---- 节点选择器 ---- */
.node-select {
  height: var(--control-md);
  padding-inline: var(--sp-3) var(--sp-8);
  background: var(--color-surface);
  color: var(--color-ink);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  font-size: var(--text-sm);
  font-family: inherit;
  cursor: pointer;
  appearance: none;
  background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='10' height='6' viewBox='0 0 10 6'%3E%3Cpath d='M1 1l4 4 4-4' stroke='%23868e9e' stroke-width='1.5' stroke-linecap='round' stroke-linejoin='round' fill='none'/%3E%3C/svg%3E");
  background-repeat: no-repeat;
  background-position: right 10px center;
  outline: none;
  transition:
    border-color var(--duration-fast) var(--ease-out),
    box-shadow var(--duration-fast) var(--ease-out);
  max-width: 200px;
}

.node-select:focus-visible {
  border-color: var(--color-accent);
  box-shadow: var(--shadow-focus);
}

/* ---- 任务指示器 ---- */
.task-indicator {
  display: flex;
  align-items: center;
  gap: var(--sp-1);
  height: var(--control-md);
  padding-inline: var(--sp-3);
  background: var(--color-accent-subtle);
  color: var(--color-accent);
  border: 1px solid var(--color-accent-muted);
  border-radius: var(--radius-md);
  font-size: var(--text-xs);
  font-weight: var(--weight-medium);
  cursor: pointer;
  transition: background-color var(--duration-fast) var(--ease-out);
}

.task-indicator:hover {
  background: var(--color-accent-muted);
}

.task-indicator:focus-visible {
  outline: none;
  box-shadow: var(--shadow-focus);
}

.task-indicator__spinner {
  width: 12px;
  height: 12px;
  border: 1.5px solid var(--color-accent-muted);
  border-top-color: var(--color-accent);
  border-radius: var(--radius-full);
  animation: topbar-spin 800ms linear infinite;
}

@keyframes topbar-spin {
  to { transform: rotate(360deg); }
}

/* ---- 图标按钮 ---- */
.icon-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: var(--control-md);
  height: var(--control-md);
  background: transparent;
  color: var(--color-muted);
  border: none;
  border-radius: var(--radius-md);
  cursor: pointer;
  transition:
    background-color var(--duration-fast) var(--ease-out),
    color var(--duration-fast) var(--ease-out);
}

.icon-btn:hover {
  background: var(--color-hover-bg);
  color: var(--color-ink);
}

.icon-btn:focus-visible {
  outline: none;
  box-shadow: var(--shadow-focus);
}
</style>
