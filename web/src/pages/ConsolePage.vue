<script setup lang="ts">
import { computed, ref } from "vue";
import { useWorkspace, taskNames } from "../workspace";
import { formatTime } from "../api";
import Button from "../components/ui/Button.vue";
import Input from "../components/ui/Input.vue";
import Status from "../components/ui/Status.vue";
import Toolbar from "../components/resources/Toolbar.vue";
import StatusBar from "../components/resources/StatusBar.vue";
import DataTable from "../components/resources/DataTable.vue";
import { RefreshCw } from "lucide-vue-next";
import {
  formatBytes,
  formatPercent,
  formatUptime,
  metricsStatus,
  resourceStatus,
  usagePercent,
} from "../resourcePresentation";
const w = useWorkspace(),
  search = ref("");
const nowSeconds = ref(Math.floor(Date.now() / 1000));
setInterval(() => (nowSeconds.value = Math.floor(Date.now() / 1000)), 15000);
function percent(value?: number | null) {
  return value == null ? "未上报" : formatPercent(value);
}
const rows = computed(() =>
  w.entries.map((e) => {
    const record: any = w.metrics[e.node.id] || null;
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
      id: e.node.id,
      name: e.node.name,
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
      disk: sample
        ? formatPercent(usagePercent(diskUsed, diskTotal))
        : "—",
      // 采样缺失时列里显示占位而不是 0；具体原因由指标状态列表达
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
      adopted:
        firewall?.state !== "ok"
          ? "未知"
          : firewall.data?.policy?.adopted
            ? "已接管"
            : "外部管理",
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
  { key: "address", label: "EasyTier", width: 140, mono: true },
  { key: "cpu", label: "CPU", width: 74, sortable: true },
  { key: "memory", label: "内存", width: 74, sortable: true },
  { key: "disk", label: "磁盘", width: 74, sortable: true },
  { key: "uptime", label: "运行时长", width: 106, sortable: true },
  { key: "containers", label: "容器", width: 70, sortable: true },
  { key: "firewall", label: "防火墙", width: 92 },
  { key: "sync", label: "地址同步", width: 100 },
  { key: "actions", label: "操作", width: 116 },
];
/** 异常聚合：离线、读取失败与异常任务必须直接可见，不靠颜色暗示。 */
const attention = computed(() => {
  const items: { id: string; node: string; text: string; tone: any }[] = [];
  for (const e of w.entries) {
    if (!e.connected)
      items.push({
        id: `offline:${e.node.id}`,
        node: e.node.name,
        text: "节点离线，显示最后采集数据，操作不可用",
        tone: "warning",
      });
    if (e.node.inventory?.docker?.state === "error")
      items.push({
        id: `docker:${e.node.id}`,
        node: e.node.name,
        text: "容器资源读取失败，不能视为没有容器",
        tone: "danger",
      });
    if (e.node.inventory?.firewall?.state === "error")
      items.push({
        id: `firewall:${e.node.id}`,
        node: e.node.name,
        text: "防火墙读取失败，不能视为没有规则",
        tone: "danger",
      });
    if (e.connected && !e.node.inventory)
      items.push({
        id: `pending:${e.node.id}`,
        node: e.node.name,
        text: "尚未采集，资源数量未知",
        tone: "neutral",
      });
  }
  for (const t of w.failures)
    items.push({
      id: `task:${t.id}`,
      node: w.nodes.find((n) => n.node.id === t.node_id)?.node.name || t.node_id,
      text: `${taskNames[t.action?.type] || t.action?.type} · ${t.error || t.status}`,
      tone: "danger",
    });
  return items;
});
</script>
<template>
  <StatusBar
    ><span
      >节点 <strong>{{ w.entries.length }}</strong></span
    ><Status tone="success">在线 {{ w.online }}</Status
    ><Status
      :tone="w.entries.length - w.online ? 'warning' : 'neutral'"
      :mark="w.entries.length - w.online ? 'pending' : 'active'"
      >离线 {{ w.entries.length - w.online }}</Status
    ><span
      >容器 <strong>{{ w.containers.length }}</strong></span
    ><span
      >地址集合 <strong>v{{ w.peers?.version ?? "—" }}</strong></span
    ><template #end
      ><Button :disabled="w.busy" @click="w.refresh"
        ><RefreshCw />刷新节点</Button
      ></template
    ></StatusBar
  >
  <section v-if="attention.length" class="attention" aria-label="需要关注">
    <header>
      <h2>需要关注</h2>
      <span class="muted">{{ attention.length }} 项</span>
    </header>
    <ul>
      <li v-for="item in attention" :key="item.id">
        <Status :tone="item.tone">{{ item.node }}</Status>
        <span>{{ item.text }}</span>
      </li>
    </ul>
  </section>
  <section v-else class="attention is-clear" aria-label="需要关注">
    <Status tone="success" mark="active">全部节点状态正常</Status>
    <span class="muted">离线、读取失败与异常任务均无记录</span>
  </section>
  <Toolbar
    ><Input
      v-model="search"
      aria-label="搜索节点"
      placeholder="搜索节点、地址或管理状态"
      class="search-input"
  /></Toolbar>
  <DataTable
    :rows="rows"
    :columns="columns"
    row-key="id"
    label="全部节点状态"
    :search="search"
    :loading="w.refreshing && !rows.length"
    empty-text="尚未接入节点"
    ><template #cell-name="{ row }"
      ><Button
        variant="text"
        @click="
          w.environment = row.id;
          w.page = 'nodes';
        "
        >{{ row.name }}</Button
      ></template
    ><template #cell-status="{ row }"
      ><Status :tone="row.status.tone" :mark="row.status.mark">{{
        row.status.text
      }}</Status></template
    ><template #cell-metrics="{ row }"
      ><Status :tone="row.metrics.tone" :mark="row.metrics.mark">{{
        row.metrics.text
      }}</Status></template
    ><template #cell-address="{ row }"
      ><span v-if="row.address !== '未登记'" class="mono">{{
        row.address
      }}</span
      ><span v-else class="muted">未登记</span></template
    ><template #cell-cpu="{ row }"
      ><span :class="{ muted: !row.hasSample }">{{ row.cpu }}</span></template
    ><template #cell-memory="{ row }"
      ><span :class="{ muted: !row.hasSample }" :title="row.memoryDetail">{{
        row.memory
      }}</span></template
    ><template #cell-disk="{ row }"
      ><span :class="{ muted: !row.hasSample }">{{ row.disk }}</span></template
    ><template #cell-uptime="{ row }"
      ><span :class="{ muted: !row.hasSample }">{{ row.uptime }}</span></template
    ><template #cell-containers="{ row }"
      ><span v-if="row.containers === '读取失败'" class="tone-danger"
        >读取失败</span
      ><span v-else-if="row.containers === '未知'" class="muted">未知</span
      ><span v-else class="port-number">{{ row.containers }}</span></template
    ><template #cell-firewall="{ row }"
      ><span v-if="row.firewall === '读取失败'" class="tone-danger"
        >读取失败</span
      ><span v-else-if="row.firewall === '未知'" class="muted">未知</span
      ><span v-else class="mono">{{ row.firewall }}</span></template
    ><template #cell-actions="{ row }"
      ><div class="inline">
        <Button
          variant="text"
          @click="
            w.environment = row.id;
            w.page = 'docker';
          "
          >容器</Button
        ><Button
          variant="text"
          @click="
            w.environment = row.id;
            w.page = 'firewall';
          "
          >防火墙</Button
        >
      </div></template
    ></DataTable
  >
</template>
