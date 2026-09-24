<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useWorkspace } from "../workspace";
import Button from "../components/ui/Button.vue";
import Input from "../components/ui/Input.vue";
import Select from "../components/ui/Select.vue";
import Tabs from "../components/ui/Tabs.vue";
import Status from "../components/ui/Status.vue";
import Tag from "../components/ui/Tag.vue";
import Endpoint from "../components/ui/Endpoint.vue";
import Notice from "../components/ui/Notice.vue";
import Menu from "../components/ui/Menu.vue";
import Hint from "../components/ui/Hint.vue";
import Toolbar from "../components/resources/Toolbar.vue";
import StatusBar from "../components/resources/StatusBar.vue";
import DataTable from "../components/resources/DataTable.vue";
import TaskTable from "../components/TaskTable.vue";
import NodesPage from "./NodesPage.vue";
import { Plus, RefreshCw, Info } from "lucide-vue-next";
import { ownership, resourceStatus } from "../resourcePresentation";
const w = useWorkspace(),
  tab = ref("rules"),
  search = ref(""),
  direction = ref(""),
  action = ref("");
watch(
  () => w.environment,
  () => {
    search.value = "";
    direction.value = "";
    action.value = "";
  },
);
/** 规则动作是预期策略，不是执行结果：允许用绿字但不给成功勾号，
    丢弃与拒绝使用中性阻断标记，红色留给失败与冲突。 */
const actions: Record<string, { text: string; tone: any; mark: any }> = {
  accept: { text: "允许", tone: "success", mark: "active" },
  drop: { text: "丢弃", tone: "neutral", mark: "neutral" },
  reject: { text: "拒绝", tone: "warning", mark: "blocked" },
};
const adopted = computed(
  () => w.firewall?.data?.policy?.adopted === true,
);
const rules = computed(() =>
  (w.firewall?.data?.policy?.rules || [])
    .map((r: any, index: number) => ({
      ...r,
      order: index + 1,
      familyText: `IPv${r.family}`,
      directionText: (
        { input: "入站", output: "出站", forward: "转发" } as Record<
          string,
          string
        >
      )[r.direction],
      actionInfo: actions[r.action] || {
        text: r.action,
        tone: "neutral",
        mark: "unknown",
      },
      status: resourceStatus({
        kind: "rule",
        offline: !w.node?.connected,
        unknown: r.enabled === undefined || r.enabled === null,
        enabled: r.enabled,
      }),
      ownerText: adopted.value ? ownership("opsd") : ownership("1Panel"),
      ownerReadOnly: !adopted.value,
    }))
    .filter(
      (r: any) =>
        (!direction.value || r.direction === direction.value) &&
        (!action.value || r.action === action.value),
    ),
);
const writable = computed(
  () =>
    w.node?.connected &&
    !w.busy &&
    w.firewall?.state === "ok" &&
    !w.firewall?.data?.conflict,
);
const columns = [
  { key: "order", label: "顺序", width: 52, sortable: true },
  { key: "familyText", label: "地址族", width: 68 },
  { key: "directionText", label: "方向", width: 58 },
  { key: "ports", label: "端口 / 协议", width: 122 },
  { key: "source", label: "来源", width: 216, mono: true },
  { key: "destination", label: "目标", width: 112, mono: true },
  { key: "actionInfo", label: "动作", width: 88 },
  { key: "status", label: "状态", width: 92 },
  { key: "ownerText", label: "所有权", width: 112 },
  { key: "actions", label: "操作", width: 92 },
];
const names = computed(() =>
  Object.fromEntries(w.nodes.map((e) => [e.node.id, e.node.name])),
);
const peerRows = computed(() =>
  w.nodes
    .filter((e) => !e.node.revoked)
    .map((e) => ({
      id: e.node.id,
      name: e.node.name,
      addresses: e.node.public_addresses.join(" · ") || "未登记",
      desired: `v${w.peers?.version ?? "—"}`,
      actual: `v${e.node.address_version}`,
      state: resourceStatus({
        kind: "sync",
        offline: !e.connected,
        unknown: e.node.address_version === undefined,
        enabled: !e.connected
          ? false
          : e.node.address_version === w.peers?.version,
      }),
      connected: e.connected,
    })),
);
const peerColumns = [
  { key: "name", label: "节点", width: 140 },
  {
    key: "addresses",
    label: w.demo ? "公网地址（演示）" : "公网地址",
    width: 560,
    mono: true,
  },
  { key: "desired", label: "期望版本", width: 100 },
  { key: "actual", label: "实际版本", width: 100 },
  { key: "state", label: "同步状态", width: 150 },
];
const history = computed(() =>
  w.tasks.filter(
    (t) =>
      t.node_id === w.node?.node.id &&
      ["firewall_plan", "firewall_apply", "peer_sync"].includes(t.action.type),
  ),
);
function ruleAction(id: string, r: any) {
  if (id === "edit") w.open("rule", r);
  if (id === "toggle")
    w.dispatch(w.node!.node.id, {
      type: "firewall_plan",
      operation: {
        type: "rule_put",
        rule: {
          id: r.id,
          family: r.family,
          direction: r.direction,
          protocol: r.protocol,
          source: r.source,
          destination: r.destination,
          ports: r.ports,
          action: r.action,
          enabled: !r.enabled,
        },
      },
    });
  if (id === "delete")
    w.dispatch(w.node!.node.id, {
      type: "firewall_plan",
      operation: { type: "rule_delete", id: r.id },
    });
}
</script>
<template>
  <template v-if="!w.node"
    ><Notice>全部节点防火墙状态；选择节点后查看规则和发布记录。</Notice
    ><NodesPage firewall-summary /></template
  ><template v-else
    ><StatusBar
      ><strong>{{ w.firewall?.data?.backend || "管理器未知" }}</strong
      ><Status
        :tone="w.firewall?.data?.policy?.adopted ? 'success' : 'neutral'"
        :mark="w.firewall?.data?.policy?.adopted ? 'active' : 'neutral'"
        >{{
          w.firewall?.data?.policy?.adopted ? "已接管" : "外部管理 / 只读"
        }}</Status
      ><span>节点地址 {{ w.peers?.addresses?.length ?? "未知" }}</span
      ><span
        >同步 v{{ w.node.node.address_version }} / v{{
          w.peers?.version ?? "—"
        }}</span
      ><Hint
        text="节点公网地址全端口互访；管理权限独立认证，不自动发布容器端口。"
        ><Button icon variant="text" aria-label="节点互访说明"
          ><Info /></Button></Hint
      ><template #end
        ><Button
          v-if="!w.firewall?.data?.policy?.adopted"
          :disabled="!writable"
          @click="
            w.dispatch(w.node!.node.id, {
              type: 'firewall_plan',
              operation: { type: 'adopt' },
            })
          "
          >接管计划</Button
        ></template
      ></StatusBar
    ><Notice v-if="!w.node.connected" tone="warning"
      >节点离线，显示最后采集数据，操作不可用。</Notice
    ><Notice v-if="w.firewall?.data?.conflict" tone="danger">{{
      w.firewall.data.conflict
    }}</Notice
    ><Tabs
      v-model="tab"
      :items="[
        { value: 'rules', label: '规则' },
        { value: 'peers', label: '节点地址' },
        { value: 'history', label: '发布记录' },
      ]" /><template v-if="tab === 'rules'"
      ><Toolbar
        ><Button variant="primary" :disabled="!writable" @click="w.open('rule')"
          ><Plus />新建规则</Button
        ><Select
          v-model="direction"
          aria-label="筛选方向"
          :options="[
            { value: '', label: '全部方向' },
            { value: 'input', label: '入站' },
            { value: 'output', label: '出站' },
            { value: 'forward', label: '转发' },
          ]"
        /><Select
          v-model="action"
          aria-label="筛选动作"
          :options="[
            { value: '', label: '全部动作' },
            { value: 'accept', label: '允许' },
            { value: 'drop', label: '丢弃' },
            { value: 'reject', label: '拒绝' },
          ]"
        /><Input
          v-model="search"
          class="search-input"
          aria-label="搜索规则"
          placeholder="搜索地址或端口"
        /><template #end
          ><Button
            :disabled="!w.firewall?.data?.external_rules"
            @click="w.open('external', w.firewall?.data?.external_rules)"
            >外部规则 · 未结构化</Button
          ><Button :disabled="w.busy" @click="w.refresh"
            ><RefreshCw />刷新</Button
          ></template
        ></Toolbar
      ><DataTable
        :rows="rules"
        :columns="columns"
        row-key="id"
        label="防火墙规则"
        :search="search"
        :filtered="!!direction || !!action"
        :error="w.firewall?.state === 'error' ? w.firewall.error : undefined"
        :unknown="!w.firewall"
        empty-text="没有受管规则；外部规则未计入此表"
        ><template #cell-ports="{ row }"
          ><Endpoint :protocol="row.protocol" :ports="row.ports" /></template
        ><template #cell-source="{ row }"
          ><span v-if="row.source" class="mono">{{ row.source }}</span
          ><span v-else class="muted">任意来源</span></template
        ><template #cell-destination="{ row }"
          ><span v-if="row.destination" class="mono">{{ row.destination }}</span
          ><span v-else class="muted">任意目标</span></template
        ><template #cell-actionInfo="{ row }"
          ><Status :tone="row.actionInfo.tone" :mark="row.actionInfo.mark">{{
            row.actionInfo.text
          }}</Status></template
        ><template #cell-status="{ row }"
          ><Status :tone="row.status.tone" :mark="row.status.mark">{{
            row.status.text
          }}</Status></template
        ><template #cell-ownerText="{ row }"
          ><Tag :title="row.ownerReadOnly ? '非 opsd 受管，只读' : '由 opsd 维护'">{{
            row.ownerText
          }}</Tag></template
        ><template #cell-actions="{ row }"
          ><div class="inline">
            <Button
              variant="text"
              :disabled="!writable"
              @click="ruleAction('edit', row)"
              >编辑</Button
            ><Menu
              :items="[
                {
                  id: 'toggle',
                  label: row.enabled ? '生成停用计划' : '生成启用计划',
                  disabled: !writable,
                },
                {
                  id: 'delete',
                  label: '生成删除计划',
                  danger: true,
                  disabled: !writable,
                },
              ]"
              @select="ruleAction($event, row)"
            /></div></template></DataTable></template
    ><template v-else-if="tab === 'peers'"
      ><Toolbar
        ><span>节点公网地址全端口互访</span
        ><template #end
          ><Button @click="w.refresh"><RefreshCw />刷新</Button></template
        ></Toolbar
      ><DataTable
        :rows="peerRows"
        :columns="peerColumns"
        row-key="id"
        label="节点地址同步"
        ><template #cell-state="{ row }"
          ><Status :tone="row.state.tone" :mark="row.state.mark">{{
            row.state.text
          }}</Status></template
        ></DataTable
      ></template
    ><TaskTable
      v-else
      :tasks="history"
      :node-names="names"
      @detail="w.open('detail', $event)"
      @plan="w.open('plan', $event)"
  /></template>
</template>
