<script setup lang="ts">
import { computed, ref } from "vue";
import { useWorkspace } from "../workspace";
import Button from "../components/ui/Button.vue";
import Input from "../components/ui/Input.vue";
import Tabs from "../components/ui/Tabs.vue";
import Status from "../components/ui/Status.vue";
import Ports from "../components/ui/Ports.vue";
import Notice from "../components/ui/Notice.vue";
import Menu from "../components/ui/Menu.vue";
import Toolbar from "../components/resources/Toolbar.vue";
import DataTable from "../components/resources/DataTable.vue";
import { Plus, RefreshCw } from "lucide-vue-next";
import { resourceStatus } from "../resourcePresentation";
const w = useWorkspace(),
  tab = ref("containers"),
  search = ref("");
const labels = {
  containers: "容器",
  stacks: "Stack",
  images: "镜像",
  networks: "网络",
  volumes: "卷",
};
/** 镜像只显示仓库名与版本；完整地址保留在 title 与搜索里。 */
function splitImage(image: string) {
  const raw = String(image || "");
  let repository = raw;
  let tag = "";
  const at = raw.indexOf("@");
  if (at >= 0) {
    repository = raw.slice(0, at);
    tag = raw.slice(at + 1).replace(/^sha256:/, "").slice(0, 12);
  } else {
    const slash = raw.lastIndexOf("/");
    const colon = raw.lastIndexOf(":");
    if (colon > slash) {
      repository = raw.slice(0, colon);
      tag = raw.slice(colon + 1);
    }
  }
  return { name: repository.split("/").pop() || repository, tag, full: raw };
}
const nodeCount = computed(() => w.entries.length);
const errors = computed(() =>
  w.entries.filter(
    (e) =>
      e.node.inventory?.docker?.state === "error" ||
      (!e.node.inventory?.docker && e.connected) ||
      !e.connected,
  ),
);
/** 单节点视图下节点名已在顶栏选定，表格不再重复该列。 */
const singleNode = computed(() => !!w.environment);
const containerColumns = computed(() => {
  const base = [
    { key: "name", label: "容器", width: 180, sortable: true },
    { key: "stateText", label: "状态", width: 100 },
    { key: "image", label: "镜像", width: 236 },
    { key: "portText", label: "发布端口", width: 176, mono: true },
    { key: "actions", label: "操作", width: 64 },
  ];
  return singleNode.value
    ? base
    : [
        base[0],
        base[1],
        { key: "node_name", label: "所属节点", width: 128 },
        base[2],
        base[3],
        base[4],
      ];
});
const rows = computed(() => {
  if (tab.value === "containers")
    return w.containers.map((c: any) => {
      const entry = w.entries.find((e) => e.node.id === c.node_id);
      const offline = !entry?.connected;
      return {
        ...c,
        name: c.names?.[0]?.replace(/^\//, ""),
        imageParts: splitImage(c.image),
        state: c.state,
        status: resourceStatus({
          kind: "container",
          offline,
          unknown: !c.state,
          state: c.state,
        }),
        portText:
          c.ports
            ?.map((p: any) =>
              p.PublicPort
                ? `${p.IP || "0.0.0.0"}:${p.PublicPort} → ${p.PrivatePort}/${p.Type}`
                : `${p.PrivatePort}/${p.Type}（未发布）`,
            )
            .join(";") || "",
      };
    });
  if (tab.value === "stacks") return w.stacks;
  return w.entries.flatMap((e) =>
    (e.node.inventory?.docker?.data?.[tab.value] || []).map((r: any) => ({
      ...r,
      uid: `${e.node.id}:${r.Id || r.Name}`,
      name: r.Name || r.RepoTags?.join(", ") || r.Id,
      node_id: e.node.id,
      node_name: e.node.name,
      detail:
        tab.value === "images"
          ? `${Math.round((r.Size || 0) / 1024 / 1024)} MiB`
          : r.Driver,
    })),
  );
});
const columns = computed(() =>
  tab.value === "containers"
    ? containerColumns.value
    : tab.value === "stacks"
      ? [
          { key: "project", label: "Stack", width: 176, sortable: true },
          { key: "node_name", label: "所属节点", width: 128 },
          { key: "directory", label: "工作目录", width: 330, mono: true },
          { key: "revision", label: "修订", width: 80 },
          { key: "protected", label: "保护资源", width: 100 },
          { key: "actions", label: "操作", width: 184 },
        ]
      : [
          { key: "name", label: "名称", width: 460, sortable: true },
          { key: "node_name", label: "所属节点", width: 140 },
          {
            key: "detail",
            label: tab.value === "images" ? "大小" : "驱动",
            width: 120,
          },
          { key: "actions", label: "操作", width: 88 },
        ],
);
/** 资源数量只在页签出现一次。 */
const tabItems = computed(() => [
  { value: "containers", label: `容器 ${w.containers.length}` },
  { value: "stacks", label: `Stack ${w.stacks.length}` },
  { value: "images", label: "镜像" },
  { value: "networks", label: "网络" },
  { value: "volumes", label: "卷" },
]);
function containerMenu(id: string, c: any) {
  if (id === "stop" || id === "restart") w.open(id, c);
  if (id === "start")
    w.dispatch(c.node_id, {
      type: "docker",
      container: c.id,
      operation: "start",
    });
  if (id === "logs") w.startStream(c, false);
  if (id === "terminal") w.startStream(c, true);
}
</script>
<template>
  <Notice v-if="!singleNode && nodeCount > 1"
    >当前为全部环境视图，共 {{ nodeCount }} 个节点；选择顶栏节点后可执行操作。</Notice
  >
  <Tabs v-model="tab" :items="tabItems" /><Toolbar
    ><template v-if="tab === 'stacks'"
      ><Button
        variant="primary"
        :disabled="!w.node?.connected"
        @click="w.open('create-stack')"
        ><Plus />创建 Stack</Button
      ><Button :disabled="!w.node?.connected" @click="w.open('stack')"
        >导入 Stack</Button
      ></template
    ><Button
      v-if="tab === 'images'"
      variant="primary"
      :disabled="!w.node?.connected"
      @click="w.open('image')"
      ><Plus />拉取镜像</Button
    ><Input
      v-model="search"
      class="search-input"
      aria-label="搜索 Docker 资源"
      placeholder="搜索名称、镜像或节点"
      :style="{ minWidth: '240px' }"
    /><template #end
      ><Button :disabled="w.busy" @click="w.refresh"
        ><RefreshCw />刷新</Button
      ></template
    ></Toolbar
  ><Notice
    v-for="e in errors"
    :key="e.node.id"
    :tone="e.node.inventory?.docker?.state === 'error' ? 'danger' : 'warning'"
    >{{ e.node.name }}：{{
      e.node.inventory?.docker?.error ||
      (!e.connected ? "节点离线，显示最后采集结果" : "尚未采集，资源数量未知")
    }}</Notice
  ><DataTable
    :key="tab + w.environment"
    :rows="rows"
    :columns="columns"
    row-key="uid"
    :label="`Docker ${labels[tab as keyof typeof labels]}`"
    :search="search"
    :loading="w.refreshing && !rows.length"
    :error="
      w.node?.node.inventory?.docker?.state === 'error'
        ? w.node.node.inventory.docker.error
        : undefined
    "
    :unknown="!!w.node && !w.node.node.inventory?.docker"
    :empty-text="
      tab === 'stacks'
        ? '尚未接管 Stack；可导入原配置管理现有项目'
        : '暂无已采集资源'
    "
    ><template #cell-name="{ row }"
      ><Button
        variant="text"
        @click="
          tab === 'containers' ? w.inspectContainer(row) : w.open('detail', row)
        "
        >{{ row.name }}</Button
      ></template
    ><template #cell-stateText="{ row }"
      ><Status :tone="row.status.tone" :mark="row.status.mark">{{
        row.status.text
      }}</Status></template
    ><template #cell-image="{ row }"
      ><span class="image-cell" :title="row.image"
        ><span>{{ row.imageParts.name }}</span
        ><span v-if="row.imageParts.tag" class="muted"
          >:{{ row.imageParts.tag }}</span
        ></span
      ></template
    ><template #cell-portText="{ row }"
      ><Ports :ports="row.portText" /></template
    ><template #cell-protected="{ row }"
      ><Status
        :tone="row.protected ? 'warning' : 'neutral'"
        :mark="row.protected ? 'pending' : 'neutral'"
        >{{ row.protected ? "是" : "否" }}</Status
      ></template
    ><template #cell-actions="{ row }"
      ><div v-if="tab === 'containers'" class="inline">
        <Menu
          :items="[
            { id: 'logs', label: '查看日志', disabled: !row.connected },
            {
              id: 'start',
              label: '启动',
              disabled: !row.connected || row.state === 'running',
            },
            { id: 'stop', label: '停止', disabled: !row.connected, danger: true },
            { id: 'restart', label: '重启', disabled: !row.connected },
            { id: 'terminal', label: '容器终端', disabled: !row.connected },
          ]"
          @select="containerMenu($event, row)"
        />
      </div>
      <div v-else-if="tab === 'stacks'" class="inline">
        <Button
          variant="text"
          :disabled="!row.connected || w.busy"
          @click="
            w.dispatch(row.node_id, {
              type: 'stack_control',
              project: row.project,
              operation: 'start',
            })
          "
          >启动</Button
        ><Button
          variant="text"
          :disabled="!row.connected"
          @click="w.open('compose', row)"
          >更新</Button
        ><Button
          variant="text"
          :disabled="!row.connected"
          @click="w.open('stack-control', row)"
          >管理</Button
        >
      </div>
      <Button v-else variant="text" @click="w.open('detail', row)"
        >详情</Button
      ></template
    ></DataTable
  >
</template>
