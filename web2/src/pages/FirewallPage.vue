<script setup lang="ts">
import { computed, ref, inject } from "vue";
import { workspaceKey } from "../workspace";
import Button from "../components/ui/Button.vue";
import Input from "../components/ui/Input.vue";
import Status from "../components/ui/Status.vue";
import SectionNumber from "../components/ui/SectionNumber.vue";
import Toolbar from "../components/resources/Toolbar.vue";
import DataTable from "../components/resources/DataTable.vue";
import { resourceStatus } from "../resourcePresentation";

const w = inject(workspaceKey)!;
const search = ref("");

const rows = computed(() => {
  if (!w.node?.node.inventory?.firewall) return [];
  const data = w.node.node.inventory.firewall.data;
  if (!data?.policy?.rules) return [];
  return data.policy.rules.map((rule: any, i: number) => ({
    id: i,
    ...rule,
    status: resourceStatus({ kind: "firewall", enabled: rule.enabled !== false }),
  }));
});

const columns = [
  { key: "port", label: "端口", width: 100, mono: true, sortable: true },
  { key: "protocol", label: "协议", width: 80 },
  { key: "source", label: "来源", width: 140, mono: true },
  { key: "action", label: "动作", width: 80 },
  { key: "status", label: "状态", width: 100 },
  { key: "actions", label: "操作", width: 120 },
];
</script>

<template>
  <div class="firewall">
    <div class="firewall-header">
      <SectionNumber num="05" />
      <div>
        <h2 class="firewall-title">防火墙规则</h2>
        <p class="firewall-sub">管理节点防火墙规则</p>
      </div>
    </div>

    <Toolbar label="规则" :count="rows.length">
      <Button variant="primary" :disabled="!w.node?.connected" @click="w.open('firewall-rule')">添加规则</Button>
      <Input v-model="search" placeholder="搜索端口或来源" />
      <Button :disabled="w.busy" @click="w.refresh">刷新</Button>
    </Toolbar>

    <DataTable
      :rows="rows"
      :columns="columns"
      row-key="id"
      label="防火墙规则"
      :search="search"
      :loading="w.refreshing && !rows.length"
      empty-text="暂无规则"
    >
      <template #cell-port="{ row }">
        <span class="firewall-mono">{{ row.port }}</span>
      </template>
      <template #cell-source="{ row }">
        <span class="firewall-mono">{{ row.source || "任意" }}</span>
      </template>
      <template #cell-status="{ row }">
        <Status :tone="row.status.tone" :mark="row.status.mark">{{ row.status.text }}</Status>
      </template>
      <template #cell-actions="{ row }">
        <div class="firewall-actions">
          <Button variant="text" @click="w.open('firewall-rule', row)">编辑</Button>
          <Button variant="text" @click="w.open('firewall-rule-delete', row)">删除</Button>
        </div>
      </template>
    </DataTable>
  </div>
</template>

<style scoped>
.firewall {
  display: flex;
  flex-direction: column;
  gap: 24px;
}
.firewall-header {
  display: flex;
  align-items: flex-start;
  gap: 24px;
}
.firewall-title {
  font-size: 24px;
  font-weight: 700;
  line-height: 1.2;
  margin: 0 0 4px;
}
.firewall-sub {
  font-size: 13px;
  color: var(--muted);
  margin: 0;
}
.firewall-mono {
  font-family: "JetBrains Mono", "Fira Code", monospace;
  font-size: 11px;
}
.firewall-actions {
  display: flex;
  gap: 4px;
}
</style>
