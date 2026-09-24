<script setup lang="ts">
import { computed, ref } from "vue";
import { useWorkspace } from "../workspace";
import { formatTime } from "../api";
import Button from "../components/ui/Button.vue";
import Input from "../components/ui/Input.vue";
import Status from "../components/ui/Status.vue";
import Tag from "../components/ui/Tag.vue";
import Toolbar from "../components/resources/Toolbar.vue";
import StatusBar from "../components/resources/StatusBar.vue";
import DataTable from "../components/resources/DataTable.vue";
import NodeMetrics from "../components/resources/NodeMetrics.vue";
import { Plus, RefreshCw } from "lucide-vue-next";
import { ownership, resourceStatus } from "../resourcePresentation";
const props = defineProps<{ firewallSummary?: boolean }>();
const w = useWorkspace(),
  search = ref("");
const rows = computed(() =>
  w.entries.map((e) => ({
    id: e.node.id,
    name: e.node.name,
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
  { key: "address", label: "EasyTier 地址", width: 156, mono: true },
  { key: "containers", label: "容器", width: 84, sortable: true },
  { key: "firewall", label: "防火墙", width: 100 },
  { key: "ownership", label: "维护来源", width: 120 },
  { key: "sync", label: "地址同步", width: 104 },
  { key: "updated", label: "采集时间", width: 158 },
  { key: "actions", label: "操作", width: 150 },
];
/** 选中节点后在同一页展开指标详情；未选择时只显示列表。 */
const selected = ref("");
function select(id: string) {
  selected.value = selected.value === id ? "" : id;
}
</script>
<template>
  <StatusBar
    ><span
      >节点 <strong>{{ w.entries.length }}</strong></span
    ><Status tone="success">在线 {{ w.online }}</Status
    ><Status :tone="w.entries.length - w.online ? 'warning' : 'neutral'"
      >离线 {{ w.entries.length - w.online }}</Status
    ><span
      >已发现容器 <strong>{{ w.containers.length }}</strong></span
    ><span
      >需关注任务 <strong>{{ w.failures.length }}</strong></span
    ></StatusBar
  ><Toolbar
    ><Button v-if="!firewallSummary" variant="primary" @click="w.open('node')"
      ><Plus />添加节点</Button
    ><Input
      v-model="search"
      aria-label="搜索节点"
      placeholder="搜索节点或地址"
      class="search-input"
    /><template #end
      ><Button :disabled="w.busy" @click="w.refresh"
        ><RefreshCw />刷新</Button
      ></template
    ></Toolbar
  ><DataTable
    :rows="rows"
    :columns="
      firewallSummary
        ? columns.filter((c) => !['address', 'containers'].includes(c.key))
        : columns
    "
    row-key="id"
    :label="firewallSummary ? '节点防火墙汇总' : '节点环境'"
    :search="search"
    :loading="w.refreshing && !rows.length"
    empty-text="尚未接入节点"
    ><template #cell-name="{ row }"
      ><Button variant="text" @click="select(row.id)">{{ row.name }}</Button
      ><span v-if="selected === row.id" class="muted"> · 已展开</span></template
    ><template #cell-status="{ row }"
      ><Status :tone="row.status.tone" :mark="row.status.mark">{{
        row.status.text
      }}</Status></template
    ><template #cell-ownership="{ row }"
      ><span v-if="row.ownership === '未知'" class="muted">未知</span
      ><Tag v-else>{{ row.ownership }}</Tag></template
    ><template #cell-actions="{ row }"
      ><div class="inline">
        <Button variant="text" @click="select(row.id)">指标</Button
        ><Button
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
  <NodeMetrics v-if="selected" :node-id="selected" />
</template>
