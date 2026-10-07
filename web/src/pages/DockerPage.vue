<script setup lang="ts">
/**
 * DockerPage — 容器管理（5 Tab）
 * 容器 / Stack / 镜像 / 网络 / 卷
 */
import { ref, computed } from 'vue'
import { workspace } from '../state/workspace'

type DockerTab = 'containers' | 'stacks' | 'images' | 'networks' | 'volumes'

const tabs: { key: DockerTab; label: string }[] = [
  { key: 'containers', label: '容器' },
  { key: 'stacks',     label: 'Stack' },
  { key: 'images',     label: '镜像' },
  { key: 'networks',   label: '网络' },
  { key: 'volumes',    label: '卷' },
]

const active = ref<DockerTab>('containers')
const node = computed(() => workspace.selectedNode.value)
</script>

<template>
  <div class="docker-page">
    <div v-if="!node?.capabilities.docker" class="docker-page__unavail">
      <span>当前节点不支持 Docker 或节点离线</span>
    </div>

    <template v-else>
      <!-- Tab 导航 -->
      <div class="docker-tabs" role="tablist">
        <button
          v-for="t in tabs"
          :key="t.key"
          class="docker-tab"
          :class="{ 'docker-tab--active': active === t.key }"
          role="tab"
          :aria-selected="active === t.key"
          @click="active = t.key"
        >
          {{ t.label }}
        </button>
      </div>

      <!-- Tab 内容（占位） -->
      <div class="docker-content" role="tabpanel">
        <div class="docker-placeholder">
          {{ tabs.find(t => t.key === active)?.label }} 列表（实现中）
        </div>
      </div>
    </template>
  </div>
</template>

<style scoped>
.docker-page { display: flex; flex-direction: column; gap: var(--sp-4); }
.docker-page__unavail { color: var(--color-muted); font-size: var(--text-sm); padding: var(--sp-8); text-align: center; }

.docker-tabs {
  display: flex; gap: 0;
  border-bottom: 1px solid var(--color-border-subtle);
}
.docker-tab {
  padding: var(--sp-2) var(--sp-4);
  font-size: var(--text-sm);
  font-weight: var(--weight-medium);
  color: var(--color-muted);
  background: transparent;
  border: none;
  border-bottom: 2px solid transparent;
  cursor: pointer;
  transition: color var(--duration-fast), border-color var(--duration-fast);
  margin-bottom: -1px;
}
.docker-tab:hover { color: var(--color-ink); }
.docker-tab--active { color: var(--color-accent); border-bottom-color: var(--color-accent); }
.docker-tab:focus-visible { outline: none; box-shadow: var(--shadow-focus); border-radius: var(--radius-sm); }

.docker-content { flex: 1; }
.docker-placeholder { color: var(--color-muted); font-size: var(--text-sm); padding: var(--sp-8); text-align: center; }
</style>
