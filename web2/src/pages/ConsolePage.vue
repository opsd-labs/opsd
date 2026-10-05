<script setup lang="ts">
import { computed, ref, inject } from "vue";
import { workspaceKey, taskNames } from "../workspace";
import { formatTime } from "../api";
import Button from "../components/ui/Button.vue";
import Input from "../components/ui/Input.vue";
import Status from "../components/ui/Status.vue";
import SectionNumber from "../components/ui/SectionNumber.vue";
import Toolbar from "../components/resources/Toolbar.vue";
import DataTable from "../components/resources/DataTable.vue";
import {
  formatBytes,
  formatPercent,
  formatUptime,
  metricsStatus,
  resourceStatus,
  usagePercent,
} from "../resourcePresentation";

const w = inject(workspaceKey)!;
const search = ref("");
const nowSeconds = ref(Math.floor(Date.now() / 1000));
setInterval(() => (nowSeconds.value = Math.floor(Date.now() / 1000)), 15000);

const rows = computed(() =>
  w.nodes.map((e) => {
    const record: any = w.metrics[e.environment] || null;
    const sample = record?.sample || null;
    const metrics = metricsStatus(e.node, record, nowSeconds.value);
    const docker = e.node.inventory?.docker;
    const firewall = e.node.inventory?.firewall;
    const diskUsed = (sample?.disks || []).reduce(
      (sum: number, d: any) => sum + (d.used || 0),
      0,
    );
    const diskTotal = (sample?.disks || []).reduce(
      (sum: number, d: any) => sum + (d.total || 0),
      0,
    );
    return {
      id: e.environment,
      name: e.name,
      online: e.connected,
      status: resourceStatus({
        kind: "node",
        offline: !e.connected,
        enabled: e.connected,
      }),
      metrics,
      address: e.node.overlay_address || "未登记",
      cpu: sample ? formatPercent(sample.cpu_usage) : "—",
      memory: sample
        ? formatPercent(usagePercent(sample.memory_used, sample.memory_total))
        : "—",
      disk: sample ? formatPercent(usagePercent(diskUsed, diskTotal)) : "—",
      hasSample: !!sample,
      uptime: sample ? formatUptime(sample.uptime) : "—",
      memoryDetail: sample
        ? `${formatBytes(sample.memory_used)} / ${formatBytes(sample.memory_total)}`
        : "",
      containers:
        docker?.state === "ok"
          ? docker.data.containers.length
          : docker?.state === "error"
            ? "读取失败"
            : "未知",
      firewall:
        firewall?.state === "error"
          ? "读取失败"
          : firewall?.data?.backend || "未知",
      sync: `v${e.node.address_version} / v${w.peers?.version ?? "—"}`,
      updated: formatTime(e.node.inventory?.collected_at),
      entry: e,
    };
  }),
);

const columns = [
  { key: "name", label: "节点", width: 170, sortable: true },
  { key: "status", label: "连接", width: 96 },
  { key: "metrics", label: "指标", width: 96 },
  { key: "address", label: "地址", width: 140, mono: true },
  { key: "cpu", label: "CPU", width: 74, sortable: true },
  { key: "memory", label: "内存", width: 74, sortable: true },
  { key: "disk", label: "磁盘", width: 74, sortable: true },
  { key: "uptime", label: "运行时长", width: 106, sortable: true },
  { key: "containers", label: "容器", width: 70, sortable: true },
  { key: "firewall", label: "防火墙", width: 92 },
  { key: "actions", label: "操作", width: 116 },
];

const attention = computed(() => {
  const items: { id: string; node: string; text: string; tone: any }[] = [];
  for (const e of w.nodes) {
    if (!e.connected)
      items.push({
        id: `offline:${e.environment}`,
        node: e.name,
        text: "节点离线，显示最后采集数据",
        tone: "warning",
      });
    if (e.node.inventory?.docker?.state === "error")
      items.push({
        id: `docker:${e.environment}`,
        node: e.name,
        text: "容器资源读取失败",
        tone: "danger",
      });
    if (e.node.inventory?.firewall?.state === "error")
      items.push({
        id: `firewall:${e.environment}`,
        node: e.name,
        text: "防火墙读取失败",
        tone: "danger",
      });
    if (e.connected && !e.node.inventory)
      items.push({
        id: `pending:${e.environment}`,
        node: e.name,
        text: "尚未采集，资源数量未知",
        tone: "neutral",
      });
  }
  for (const t of w.tasks.filter((t) => t.status === "failed"))
    items.push({
      id: `task:${t.id}`,
      node: w.nodes.find((n) => n.environment === t.node_id)?.name || t.node_id,
      text: `${taskNames[t.action?.type] || t.action?.type} · ${t.error || t.status}`,
      tone: "danger",
    });
  return items;
});
</script>

<template>
  <div class="console">
    <div class="console-header">
      <SectionNumber num="01" />
      <div>
        <h2 class="console-title">控制台总览</h2>
        <p class="console-sub">全部节点状态与异常聚合</p>
      </div>
    </div>

    <div class="console-stats">
      <div class="console-stat">
        <span class="console-stat-label">节点</span>
        <strong class="console-stat-value">{{ w.nodes.length }}</strong>
      </div>
      <div class="console-stat">
        <Status tone="success">在线 {{ w.nodes.filter((n) => n.connected).length }}</Status>
      </div>
      <div class="console-stat">
        <Status :tone="w.nodes.filter((n) => !n.connected).length ? 'warning' : 'neutral'">
          离线 {{ w.nodes.filter((n) => !n.connected).length }}
        </Status>
      </div>
      <div class="console-stat">
        <span class="console-stat-label">地址集合</span>
        <strong class="console-stat-value">v{{ w.peers?.version ?? "—" }}</strong>
      </div>
      <div class="console-stat">
        <Button :disabled="w.busy" @click="w.refresh">刷新节点</Button>
      </div>
    </div>

    <section v-if="attention.length" class="console-attention">
      <h3 class="console-section-title">需要关注 · {{ attention.length }} 项</h3>
      <ul class="console-attention-list">
        <li v-for="item in attention" :key="item.id">
          <Status :tone="item.tone">{{ item.node }}</Status>
          <span>{{ item.text }}</span>
        </li>
      </ul>
    </section>
    <section v-else class="console-clear">
      <Status tone="success" mark="active">全部节点状态正常</Status>
    </section>

    <Toolbar label="全部节点" :count="rows.length">
      <Input v-model="search" placeholder="搜索节点、地址" />
    </Toolbar>

    <DataTable
      :rows="rows"
      :columns="columns"
      row-key="id"
      label="全部节点状态"
      :search="search"
      :loading="w.refreshing && !rows.length"
      empty-text="尚未接入节点"
    >
      <template #cell-name="{ row }">
        <Button variant="text" @click="w.environment = row.id; w.page = 'nodes'">
          {{ row.name }}
        </Button>
      </template>
      <template #cell-status="{ row }">
        <Status :tone="row.status.tone" :mark="row.status.mark">{{ row.status.text }}</Status>
      </template>
      <template #cell-metrics="{ row }">
        <Status :tone="row.metrics.tone" :mark="row.metrics.mark">{{ row.metrics.text }}</Status>
      </template>
      <template #cell-address="{ row }">
        <span v-if="row.address !== '未登记'" class="console-mono">{{ row.address }}</span>
        <span v-else class="console-muted">未登记</span>
      </template>
      <template #cell-cpu="{ row }">
        <span :class="{ 'console-muted': !row.hasSample }">{{ row.cpu }}</span>
      </template>
      <template #cell-memory="{ row }">
        <span :class="{ 'console-muted': !row.hasSample }" :title="row.memoryDetail">{{ row.memory }}</span>
      </template>
      <template #cell-disk="{ row }">
        <span :class="{ 'console-muted': !row.hasSample }">{{ row.disk }}</span>
      </template>
      <template #cell-uptime="{ row }">
        <span :class="{ 'console-muted': !row.hasSample }">{{ row.uptime }}</span>
      </template>
      <template #cell-containers="{ row }">
        <span v-if="row.containers === '读取失败'" class="console-danger">读取失败</span>
        <span v-else-if="row.containers === '未知'" class="console-muted">未知</span>
        <span v-else>{{ row.containers }}</span>
      </template>
      <template #cell-firewall="{ row }">
        <span v-if="row.firewall === '读取失败'" class="console-danger">读取失败</span>
        <span v-else-if="row.firewall === '未知'" class="console-muted">未知</span>
        <span v-else class="console-mono">{{ row.firewall }}</span>
      </template>
      <template #cell-actions="{ row }">
        <div class="console-actions">
          <Button variant="text" @click="w.environment = row.id; w.page = 'docker'">容器</Button>
          <Button variant="text" @click="w.environment = row.id; w.page = 'firewall'">防火墙</Button>
        </div>
      </template>
    </DataTable>
  </div>
</template>

<style scoped>
.console {
  display: flex;
  flex-direction: column;
  gap: 24px;
}
.console-header {
  display: flex;
  align-items: flex-start;
  gap: 24px;
}
.console-title {
  font-size: 24px;
  font-weight: 700;
  line-height: 1.2;
  margin: 0 0 4px;
}
.console-sub {
  font-size: 13px;
  color: var(--muted);
  margin: 0;
}
.console-stats {
  display: flex;
  align-items: center;
  gap: 24px;
  padding: 16px 0;
  border-bottom: 1px solid var(--line);
}
.console-stat {
  display: flex;
  align-items: baseline;
  gap: 6px;
}
.console-stat-label {
  font-size: 11px;
  color: var(--muted);
}
.console-stat-value {
  font-size: 20px;
  font-weight: 700;
  font-variant-numeric: tabular-nums;
}
.console-attention {
  padding: 16px;
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: var(--r);
}
.console-section-title {
  font-size: 11px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--muted);
  margin: 0 0 12px;
}
.console-attention-list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.console-attention-list li {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
}
.console-clear {
  padding: 16px;
  border-bottom: 1px solid var(--line);
}
.console-muted {
  color: var(--muted);
}
.console-mono {
  font-family: "JetBrains Mono", "Fira Code", monospace;
  font-size: 11px;
}
.console-danger {
  color: var(--accent);
}
.console-actions {
  display: flex;
  gap: 4px;
}
</style>
