<script setup lang="ts">
import { computed, ref, inject } from "vue";
import { workspaceKey } from "../workspace";
import { formatTime } from "../api";
import Button from "../components/ui/Button.vue";
import Input from "../components/ui/Input.vue";
import Status from "../components/ui/Status.vue";
import Tag from "../components/ui/Tag.vue";
import SectionNumber from "../components/ui/SectionNumber.vue";
import Toolbar from "../components/resources/Toolbar.vue";
import DataTable from "../components/resources/DataTable.vue";
import NodeMetrics from "../components/resources/NodeMetrics.vue";
import { ownership, resourceStatus } from "../resourcePresentation";

const w = inject(workspaceKey)!;
const search = ref("");
const selected = ref("");

const rows = computed(() =>
  w.nodes.map((e) => ({
    id: e.environment,
    name: e.name,
    address: e.node.overlay_address || "未登记",
    status: resourceStatus({
      kind: "node",
      offline: !e.connected,
      enabled: e.connected,
    }),
    containers:
      e.node.inventory?.docker?.state === "ok"
        ? e.node.inventory.docker.data.containers.length
        : e.node.inventory?.docker?.state === "error"
          ? "读取失败"
          : "未知",
    firewall:
      e.node.inventory?.firewall?.state === "error"
        ? "读取失败"
        : e.node.inventory?.firewall?.data?.backend || "未知",
    ownership:
      e.node.inventory?.firewall?.state !== "ok"
        ? "未知"
        : e.node.inventory.firewall.data?.policy?.adopted
          ? "opsd 已接管"
          : ownership("1Panel"),
    sync: `v${e.node.address_version} / v${w.peers?.version ?? "—"}`,
    updated: formatTime(e.node.inventory?.collected_at),
    entry: e,
  })),
);

const columns = [
  { key: "name", label: "节点", width: 170, sortable: true },
  { key: "status", label: "连接", width: 96 },
  { key: "address", label: "地址", width: 156, mono: true },
  { key: "containers", label: "容器", width: 84, sortable: true },
  { key: "firewall", label: "防火墙", width: 100 },
  { key: "ownership", label: "维护来源", width: 120 },
  { key: "sync", label: "地址同步", width: 104 },
  { key: "actions", label: "操作", width: 150 },
];

function select(id: string) {
  selected.value = selected.value === id ? "" : id;
}
</script>

<template>
  <div class="nodes">
    <div class="nodes-header">
      <SectionNumber num="02" />
      <div>
        <h2 class="nodes-title">节点管理</h2>
        <p class="nodes-sub">全部节点状态与指标详情</p>
      </div>
    </div>

    <Toolbar label="节点" :count="rows.length">
      <Button variant="primary" @click="w.open('node')">添加节点</Button>
      <Input v-model="search" placeholder="搜索节点或地址" />
      <Button :disabled="w.busy" @click="w.refresh">刷新</Button>
    </Toolbar>

    <DataTable
      :rows="rows"
      :columns="columns"
      row-key="id"
      label="节点环境"
      :search="search"
      :loading="w.refreshing && !rows.length"
      empty-text="尚未接入节点"
    >
      <template #cell-name="{ row }">
        <Button variant="text" @click="select(row.id)">{{ row.name }}</Button>
        <span v-if="selected === row.id" class="nodes-expanded"> · 已展开</span>
      </template>
      <template #cell-status="{ row }">
        <Status :tone="row.status.tone" :mark="row.status.mark">{{ row.status.text }}</Status>
      </template>
      <template #cell-address="{ row }">
        <span v-if="row.address !== '未登记'" class="nodes-mono">{{ row.address }}</span>
        <span v-else class="nodes-muted">未登记</span>
      </template>
      <template #cell-containers="{ row }">
        <span v-if="row.containers === '读取失败'" class="nodes-danger">读取失败</span>
        <span v-else-if="row.containers === '未知'" class="nodes-muted">未知</span>
        <span v-else>{{ row.containers }}</span>
      </template>
      <template #cell-firewall="{ row }">
        <span v-if="row.firewall === '读取失败'" class="nodes-danger">读取失败</span>
        <span v-else-if="row.firewall === '未知'" class="nodes-muted">未知</span>
        <span v-else class="nodes-mono">{{ row.firewall }}</span>
      </template>
      <template #cell-ownership="{ row }">
        <span v-if="row.ownership === '未知'" class="nodes-muted">未知</span>
        <Tag v-else>{{ row.ownership }}</Tag>
      </template>
      <template #cell-actions="{ row }">
        <div class="nodes-actions">
          <Button variant="text" @click="select(row.id)">指标</Button>
          <Button variant="text" @click="w.environment = row.id; w.page = 'docker'">容器</Button>
          <Button variant="text" @click="w.environment = row.id; w.page = 'firewall'">防火墙</Button>
        </div>
      </template>
    </DataTable>

    <div v-if="selected" class="nodes-detail">
      <NodeMetrics :node-id="selected" />
    </div>
  </div>
</template>

<style scoped>
.nodes {
  display: flex;
  flex-direction: column;
  gap: 24px;
}
.nodes-header {
  display: flex;
  align-items: flex-start;
  gap: 24px;
}
.nodes-title {
  font-size: 24px;
  font-weight: 700;
  line-height: 1.2;
  margin: 0 0 4px;
}
.nodes-sub {
  font-size: 13px;
  color: var(--muted);
  margin: 0;
}
.nodes-expanded {
  font-size: 11px;
  color: var(--muted);
}
.nodes-muted {
  color: var(--muted);
}
.nodes-mono {
  font-family: "JetBrains Mono", "Fira Code", monospace;
  font-size: 11px;
}
.nodes-danger {
  color: var(--accent);
}
.nodes-actions {
  display: flex;
  gap: 4px;
}
.nodes-detail {
  padding-top: 24px;
  border-top: 1px solid var(--line);
}
</style>
