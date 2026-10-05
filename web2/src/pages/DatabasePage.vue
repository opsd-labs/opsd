<script setup lang="ts">
import { computed, ref, inject } from "vue";
import { workspaceKey } from "../workspace";
import Button from "../components/ui/Button.vue";
import Input from "../components/ui/Input.vue";
import Status from "../components/ui/Status.vue";
import Notice from "../components/ui/Notice.vue";
import SectionNumber from "../components/ui/SectionNumber.vue";
import Toolbar from "../components/resources/Toolbar.vue";
import DataTable from "../components/resources/DataTable.vue";

const w = inject(workspaceKey)!;
const search = ref("");

const rows = computed(() => {
  if (!w.node?.node.inventory?.database) return [];
  const data = w.node.node.inventory.database.data;
  if (!data?.databases) return [];
  return data.databases.map((db: any, i: number) => ({
    id: i,
    ...db,
  }));
});

const columns = [
  { key: "name", label: "数据库", width: 200, sortable: true },
  { key: "size", label: "大小", width: 100, mono: true },
  { key: "tables", label: "表数量", width: 80 },
  { key: "status", label: "状态", width: 100 },
];
</script>

<template>
  <div class="database">
    <div class="database-header">
      <SectionNumber num="03" />
      <div>
        <h2 class="database-title">数据库巡检</h2>
        <p class="database-sub">只读巡检数据库状态</p>
      </div>
    </div>

    <Notice tone="info">
      数据库巡检为只读操作，不会修改任何数据。
    </Notice>

    <Toolbar label="数据库" :count="rows.length">
      <Input v-model="search" placeholder="搜索数据库名称" />
      <Button :disabled="w.busy" @click="w.refresh">刷新</Button>
    </Toolbar>

    <DataTable
      :rows="rows"
      :columns="columns"
      row-key="id"
      label="数据库列表"
      :search="search"
      :loading="w.refreshing && !rows.length"
      empty-text="暂无数据库"
    >
      <template #cell-size="{ row }">
        <span class="database-mono">{{ row.size }}</span>
      </template>
      <template #cell-status="{ row }">
        <Status tone="success" mark="active">正常</Status>
      </template>
    </DataTable>
  </div>
</template>

<style scoped>
.database {
  display: flex;
  flex-direction: column;
  gap: 24px;
}
.database-header {
  display: flex;
  align-items: flex-start;
  gap: 24px;
}
.database-title {
  font-size: 24px;
  font-weight: 700;
  line-height: 1.2;
  margin: 0 0 4px;
}
.database-sub {
  font-size: 13px;
  color: var(--muted);
  margin: 0;
}
.database-mono {
  font-family: "JetBrains Mono", "Fira Code", monospace;
  font-size: 11px;
}
</style>
