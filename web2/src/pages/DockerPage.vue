<script setup lang="ts">
import { computed, ref, inject } from "vue";
import { workspaceKey } from "../workspace";
import Button from "../components/ui/Button.vue";
import Input from "../components/ui/Input.vue";
import Tabs from "../components/ui/Tabs.vue";
import Status from "../components/ui/Status.vue";
import Notice from "../components/ui/Notice.vue";
import SectionNumber from "../components/ui/SectionNumber.vue";
import Toolbar from "../components/resources/Toolbar.vue";
import DataTable from "../components/resources/DataTable.vue";
import { resourceStatus } from "../resourcePresentation";

const w = inject(workspaceKey)!;
const tab = ref("containers");
const search = ref("");

const labels = {
  containers: "容器",
  stacks: "Stack",
  images: "镜像",
  networks: "网络",
  volumes: "卷",
};

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

const nodeCount = computed(() => w.nodes.length);
const errors = computed(() =>
  w.nodes.filter(
    (e) =>
      e.node.inventory?.docker?.state === "error" ||
      (!e.node.inventory?.docker && e.connected) ||
      !e.connected,
  ),
);
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
    : [base[0], base[1], { key: "node_name", label: "所属节点", width: 128 }, base[2], base[3], base[4]];
});

const rows = computed(() => {
  if (tab.value === "containers")
    return w.containers.map((c: any) => {
      const entry = w.nodes.find((e) => e.environment === c.node_id);
      const offline = !entry?.connected;
      return {
        ...c,
        uid: c.id,
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
  return w.nodes.flatMap((e) =>
    (e.node.inventory?.docker?.data?.[tab.value] || []).map((r: any) => ({
      ...r,
      uid: `${e.environment}:${r.Id || r.Name}`,
      name: r.Name || r.RepoTags?.join(", ") || r.Id,
      node_id: e.environment,
      node_name: e.name,
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
          { key: "detail", label: tab.value === "images" ? "大小" : "驱动", width: 120 },
          { key: "actions", label: "操作", width: 88 },
        ],
);

const tabItems = computed(() => [
  { value: "containers", label: `容器 ${w.containers.length}` },
  { value: "stacks", label: `Stack ${w.stacks.length}` },
  { value: "images", label: "镜像" },
  { value: "networks", label: "网络" },
  { value: "volumes", label: "卷" },
]);

function containerAction(action: string, c: any) {
  if (action === "stop" || action === "restart") w.open(action, c);
  if (action === "start")
    w.dispatch(c.node_id, { type: "docker", container: c.id, operation: "start" });
  if (action === "logs") w.startStream(c, false);
  if (action === "terminal") w.startStream(c, true);
}
</script>

<template>
  <div class="docker">
    <div class="docker-header">
      <SectionNumber num="04" />
      <div>
        <h2 class="docker-title">容器管理</h2>
        <p class="docker-sub">Docker 资源与 Stack 管理</p>
      </div>
    </div>

    <Notice v-if="!singleNode && nodeCount > 1">
      当前为全部环境视图，共 {{ nodeCount }} 个节点；选择顶栏节点后可执行操作。
    </Notice>

    <Tabs v-model="tab" :items="tabItems" />

    <Toolbar>
      <template v-if="tab === 'stacks'">
        <Button variant="primary" :disabled="!w.node?.connected" @click="w.open('create-stack')">创建 Stack</Button>
        <Button :disabled="!w.node?.connected" @click="w.open('stack')">导入 Stack</Button>
      </template>
      <Button v-if="tab === 'images'" variant="primary" :disabled="!w.node?.connected" @click="w.open('image')">拉取镜像</Button>
      <Input v-model="search" placeholder="搜索名称、镜像或节点" />
      <Button :disabled="w.busy" @click="w.refresh">刷新</Button>
    </Toolbar>

    <Notice
      v-for="e in errors"
      :key="e.environment"
      :tone="e.node.inventory?.docker?.state === 'error' ? 'danger' : 'warning'"
    >
      {{ e.name }}：{{ e.node.inventory?.docker?.error || (!e.connected ? "节点离线" : "尚未采集") }}
    </Notice>

    <DataTable
      :key="tab + w.environment"
      :rows="rows"
      :columns="columns"
      row-key="uid"
      :label="`Docker ${labels[tab as keyof typeof labels]}`"
      :search="search"
      :loading="w.refreshing && !rows.length"
      :error="w.node?.node.inventory?.docker?.state === 'error' ? w.node.node.inventory.docker.error : undefined"
      :unknown="!!w.node && !w.node.node.inventory?.docker"
      :empty-text="tab === 'stacks' ? '尚未接管 Stack' : '暂无已采集资源'"
    >
      <template #cell-name="{ row }">
        <Button variant="text" @click="tab === 'containers' ? w.inspectContainer(row) : w.open('detail', row)">
          {{ row.name }}
        </Button>
      </template>
      <template #cell-stateText="{ row }">
        <Status :tone="row.status.tone" :mark="row.status.mark">{{ row.status.text }}</Status>
      </template>
      <template #cell-image="{ row }">
        <span class="docker-image" :title="row.image">
          <span>{{ row.imageParts.name }}</span>
          <span v-if="row.imageParts.tag" class="docker-muted">:{{ row.imageParts.tag }}</span>
        </span>
      </template>
      <template #cell-portText="{ row }">
        <span class="docker-mono">{{ row.portText || "—" }}</span>
      </template>
      <template #cell-protected="{ row }">
        <Status :tone="row.protected ? 'warning' : 'neutral'" :mark="row.protected ? 'pending' : 'neutral'">
          {{ row.protected ? "是" : "否" }}
        </Status>
      </template>
      <template #cell-actions="{ row }">
        <div v-if="tab === 'containers'" class="docker-actions">
          <Button variant="text" :disabled="!row.connected" @click="containerAction('logs', row)">日志</Button>
          <Button variant="text" :disabled="!row.connected || row.state === 'running'" @click="containerAction('start', row)">启动</Button>
          <Button variant="text" :disabled="!row.connected" @click="containerAction('stop', row)">停止</Button>
          <Button variant="text" :disabled="!row.connected" @click="containerAction('terminal', row)">终端</Button>
        </div>
        <div v-else-if="tab === 'stacks'" class="docker-actions">
          <Button variant="text" :disabled="!row.connected" @click="w.dispatch(row.node_id, { type: 'stack_control', project: row.project, operation: 'start' })">启动</Button>
          <Button variant="text" :disabled="!row.connected" @click="w.open('compose', row)">更新</Button>
          <Button variant="text" :disabled="!row.connected" @click="w.open('stack-control', row)">管理</Button>
        </div>
        <Button v-else variant="text" @click="w.open('detail', row)">详情</Button>
      </template>
    </DataTable>
  </div>
</template>

<style scoped>
.docker {
  display: flex;
  flex-direction: column;
  gap: 24px;
}
.docker-header {
  display: flex;
  align-items: flex-start;
  gap: 24px;
}
.docker-title {
  font-size: 24px;
  font-weight: 700;
  line-height: 1.2;
  margin: 0 0 4px;
}
.docker-sub {
  font-size: 13px;
  color: var(--muted);
  margin: 0;
}
.docker-image {
  display: inline-flex;
  gap: 2px;
}
.docker-muted {
  color: var(--muted);
}
.docker-mono {
  font-family: "JetBrains Mono", "Fira Code", monospace;
  font-size: 11px;
}
.docker-actions {
  display: flex;
  gap: 4px;
}
</style>
