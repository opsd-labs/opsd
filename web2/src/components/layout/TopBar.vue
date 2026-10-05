<script setup lang="ts">
import { inject, computed } from "vue";
import { workspaceKey, navigation } from "../../workspace";
import Button from "../ui/Button.vue";

const w = inject(workspaceKey)!;

const pageTitle = computed(() => {
  const item = navigation.find((n) => n.id === w.page);
  return item?.label || "";
});

const currentNode = computed(() => {
  if (!w.environment) return "全部节点";
  const node = w.nodes.find((n) => n.environment === w.environment);
  return node?.name || w.environment;
});
</script>

<template>
  <header class="topbar">
    <h1 class="topbar-title">{{ pageTitle }}</h1>
    <div class="topbar-actions">
      <select
        class="topbar-node"
        :value="w.environment"
        @change="w.environment = ($event.target as HTMLSelectElement).value"
      >
        <option value="">全部节点</option>
        <option v-for="n in w.nodes" :key="n.environment" :value="n.environment">
          {{ n.name }}
        </option>
      </select>
      <Button variant="text" @click="w.taskPanel = true">
        任务
        <span v-if="w.tasks.length" class="topbar-count">{{ w.tasks.length }}</span>
      </Button>
    </div>
  </header>
</template>

<style scoped>
.topbar {
  height: 40px;
  border-bottom: 1px solid var(--line);
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 32px;
  background: var(--surface);
}
.topbar-title {
  font-size: 16px;
  font-weight: 700;
  line-height: 1.3;
  margin: 0;
  color: var(--ink);
}
.topbar-actions {
  display: flex;
  align-items: center;
  gap: 12px;
}
.topbar-node {
  padding: 4px 8px;
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: var(--r);
  font: inherit;
  font-size: 12px;
  color: var(--ink);
  cursor: pointer;
}
.topbar-node:hover {
  border-color: var(--ink);
}
.topbar-count {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 16px;
  height: 16px;
  padding: 0 4px;
  background: var(--accent);
  color: white;
  font-size: 10px;
  font-weight: 700;
  border-radius: 8px;
  margin-left: 4px;
}
</style>
