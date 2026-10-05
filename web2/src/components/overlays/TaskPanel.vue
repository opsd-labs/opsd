<script setup lang="ts">
import { computed, inject, ref } from "vue";
import { workspaceKey } from "../../workspace";
import Drawer from "../ui/Drawer.vue";
import Tabs from "../ui/Tabs.vue";
import TaskTable from "./TaskTable.vue";
import Notice from "../ui/Notice.vue";
const w = inject(workspaceKey)!;
const tab = ref("active");
const names = computed(() =>
  Object.fromEntries(w.nodes.map((e) => [e.node.id, e.node.name])),
);
const active = computed(() => w.activeTasks);
const failed = computed(() => w.failures);
const history = computed(() => w.tasks);
const rows = computed(() =>
  tab.value === "active"
    ? active.value
    : tab.value === "failed"
      ? failed.value
      : history.value,
);
</script>
<template>
  <Drawer
    v-model:open="w.taskPanel"
    title="任务与异常"
    wide
    description="变更类任务的执行状态与结果；页面内不再单独占用一级入口。"
    ><Tabs
      v-model="tab"
      :items="[
        { value: 'active', label: `执行中 ${active.length}` },
        { value: 'failed', label: `异常 ${failed.length}` },
        { value: 'all', label: `全部 ${history.length}` },
      ]"
    /><Notice v-if="w.demo">演示 · 不执行操作</Notice>
    <TaskTable
      :tasks="rows"
      :node-names="names"
      @detail="w.open('detail', $event)"
      @plan="w.open('plan', $event)"
  /></Drawer>
</template>
