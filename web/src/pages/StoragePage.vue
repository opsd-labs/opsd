<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useWorkspace } from "../workspace";
import { api } from "../api";
import Button from "../components/ui/Button.vue";
import Notice from "../components/ui/Notice.vue";
import Status from "../components/ui/Status.vue";
import Tag from "../components/ui/Tag.vue";
import Toolbar from "../components/resources/Toolbar.vue";
import StatusBar from "../components/resources/StatusBar.vue";
import { RefreshCw, Search } from "lucide-vue-next";
import Field from "../components/resources/Field.vue";
import Input from "../components/ui/Input.vue";
import Checkbox from "../components/ui/Checkbox.vue";

/**
 * 存储预检。
 *
 * 页面展示的是**结论式报告**：先给总体结论与一致性检查，再逐节点列出适合与否
 * 以及具体原因。未采集的节点显示「待采集」，不会被当作不适合——
 * 这两种情况的处理方式完全不同。
 */
type Candidate = {
  device: string;
  path: string;
  size: number;
  rotational: boolean;
  model: string | null;
};
type Verdict = {
  node_id: string;
  name: string;
  online: boolean;
  suitability: "suitable" | "needs_work" | "unsuitable" | "pending";
  reasons: string[];
  candidates: Candidate[];
  rejected: { device: string; reason: string }[];
  collected_at: number | null;
  gaps: string[];
  link_mbps: number | null;
};
type Check = { name: string; passed: boolean; detail: string };
type Report = {
  generated_at: number;
  nodes: Verdict[];
  checks: Check[];
  topology: {
    nodes: number;
    drives_per_node: number;
    drive_size: number;
    raw_capacity: number;
    parity: number;
    usable_capacity: number;
    parity_note: string;
  } | null;
  ready: boolean;
  summary: string;
};

const w = useWorkspace();
const report = ref<Report | null>(null);
const error = ref("");
const notice = ref("");
const busy = ref(false);
const expanded = ref("");

/* ---- 集群定义与计划 ---- */
type Plan = {
  id: string;
  cluster: string;
  expires: number;
  fingerprint: string;
  image: string;
  destructive: boolean;
  irreversible_notice: string;
  nodes: {
    node_id: string;
    name: string;
    devices: { path: string; expected_size: number; mount: string }[];
  }[];
};
const clusters = ref<any[]>([]);
const limits = ref({ image_prefix: "pgsty/silo", filesystem: "xfs", min_nodes: 4 });
const spec = ref({
  name: "silo-prod",
  nodes: [] as string[],
  image_tag: "RELEASE.2026-09-03T13-18-01Z",
  endpoint: "",
  access_key: "opsdadmin",
  secret_key: "",
});
const plan = ref<Plan | null>(null);
/** 破坏性计划必须显式确认，界面用勾选框而不是回车键兜底。 */
const acknowledged = ref(false);
const applied = ref<any[]>([]);

const deviceCount = computed(
  () => plan.value?.nodes.reduce((sum, n) => sum + n.devices.length, 0) || 0,
);
const planExpired = computed(
  () => !!plan.value && plan.value.expires * 1000 < Date.now(),
);

/** 可纳入集群的节点：预检判定为适合的节点。 */
const selectableNodes = computed(() =>
  (report.value?.nodes || []).filter((n) => n.suitability === "suitable"),
);

async function loadClusters() {
  if (w.demo) {
    clusters.value = [];
    return;
  }
  try {
    const data = await api("/storage/clusters");
    clusters.value = data.clusters || [];
    limits.value = {
      image_prefix: data.image_prefix,
      filesystem: data.filesystem,
      min_nodes: data.min_nodes,
    };
  } catch {
    // 集群列表读不到不影响预检结论展示
  }
}

async function saveCluster() {
  await guard(async () => {
    if (w.demo) {
      notice.value = "演示模式：未保存集群定义";
      return;
    }
    await api("/storage/clusters", "PUT", spec.value);
    notice.value = "集群定义已保存";
    await loadClusters();
  });
}

async function makePlan() {
  await guard(async () => {
    if (w.demo) {
      notice.value = "演示模式：未生成计划";
      return;
    }
    const result = await api("/storage/plans", "POST", { name: spec.value.name });
    plan.value = result.plan;
    acknowledged.value = false;
    applied.value = [];
    notice.value = "计划已生成；请逐项核对将要清空的设备，确认后才能执行";
  });
}

async function applyPlan() {
  await guard(async () => {
    if (w.demo) {
      notice.value = "演示模式：未执行计划";
      return;
    }
    const result = await api("/storage/plans/apply", "POST", {
      plan_id: plan.value!.id,
      acknowledge_destructive: acknowledged.value,
    });
    applied.value = result.dispatched || [];
    notice.value = `已下发 ${applied.value.length} 个步骤；执行进度可在任务面板查看`;
  });
}

async function guard(fn: () => Promise<void>) {
  busy.value = true;
  error.value = "";
  try {
    await fn();
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e);
  } finally {
    busy.value = false;
  }
}

const suitabilityLabel: Record<string, string> = {
  suitable: "适合部署",
  needs_work: "需要先处理",
  unsuitable: "不适合",
  pending: "待采集",
};
function tone(value: string) {
  if (value === "suitable") return "success" as const;
  if (value === "needs_work") return "warning" as const;
  if (value === "unsuitable") return "danger" as const;
  return "neutral" as const;
}
function mark(value: string) {
  if (value === "suitable") return "active" as const;
  if (value === "needs_work") return "pending" as const;
  if (value === "unsuitable") return "failed" as const;
  return "unknown" as const;
}
const bytes = (value: number) => {
  const units = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
  if (!value) return "0 B";
  let size = value;
  let unit = 0;
  while (size >= 1024 && unit < units.length - 1) {
    size /= 1024;
    unit += 1;
  }
  return `${size >= 100 || unit === 0 ? size.toFixed(0) : size.toFixed(1)} ${units[unit]}`;
};

const counts = computed(() => {
  const nodes = report.value?.nodes || [];
  return {
    total: nodes.length,
    suitable: nodes.filter((n) => n.suitability === "suitable").length,
    pending: nodes.filter((n) => n.suitability === "pending").length,
    drives: nodes.reduce((sum, n) => sum + n.candidates.length, 0),
  };
});

async function load() {
  busy.value = true;
  error.value = "";
  try {
    if (w.demo) {
      report.value = demoReport();
    } else {
      report.value = await api("/storage/readiness");
    }
    // 默认勾选全部「适合部署」的节点；管理员可以取消其中任意一台
    if (!spec.value.nodes.length)
      spec.value.nodes = selectableNodes.value.map((n) => n.node_id);
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e);
  } finally {
    busy.value = false;
  }
}

async function inspect(nodeId: string) {
  notice.value = "";
  error.value = "";
  try {
    if (w.demo) {
      notice.value = "演示模式：未提交盘点任务";
      return;
    }
    const result = await api(`/nodes/${nodeId}/host-inspect`, "POST", {
      idempotency_key: crypto.randomUUID(),
    });
    notice.value = `已提交盘点任务 ${result.task_id.slice(0, 8)}；执行完成后刷新即可看到结果`;
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e);
  }
}

async function inspectAll() {
  for (const node of w.nodes.filter((e) => e.connected && !e.node.revoked))
    await inspect(node.node.id);
}

/** 演示数据：明确标注为静态，四种结论都出现，且有足够节点让计划流程可走通。 */
function demoReport(): Report {
  const gb = (n: number) => n * 1024 ** 3;
  const clean = (id: string, name: string, extra: Partial<Verdict> = {}): Verdict => ({
    node_id: id,
    name,
    online: true,
    suitability: "suitable",
    reasons: ["发现 2 块可用磁盘，合计 3.6 TiB", "SMART 健康"],
    candidates: [
      { device: "sdb", path: "/dev/sdb", size: gb(1800), rotational: false, model: "Samsung SSD" },
      { device: "sdc", path: "/dev/sdc", size: gb(1800), rotational: false, model: "Samsung SSD" },
    ],
    rejected: [{ device: "sda", reason: "承载系统挂载点（/），不可格式化" }],
    collected_at: Math.floor(Date.now() / 1000) - 120,
    gaps: [],
    link_mbps: 1000,
    ...extra,
  });
  const nodes: Verdict[] = [
    clean("C001", "C001 · 美国"),
    clean("C002", "C011 · 台湾"),
    clean("C003", "C041 · 美国"),
    clean("C004", "C091 · 日本"),
    clean("C006", "C071 · 美国", {
      // 有采集缺口 → 需要先处理，因此不会被算进候选节点
      suitability: "needs_work",
      gaps: ["SMART 未采集：smartctl 不存在"],
      reasons: ["发现 2 块可用磁盘，合计 3.6 TiB", "SMART 未采集，磁盘健康状态未知"],
    }),
    {
      node_id: "C005",
      name: "C052 · 日本",
      online: true,
      suitability: "unsuitable",
      reasons: ["没有找到可用于存储集群的干净磁盘"],
      candidates: [],
      rejected: [
        { device: "sda", reason: "承载系统挂载点（/），不可格式化" },
        {
          device: "sdb",
          reason:
            "已有 ext4 文件系统但未挂载，可能仍有数据；确认无用后请先 wipefs 再来",
        },
      ],
      collected_at: Math.floor(Date.now() / 1000) - 90,
      gaps: [],
      link_mbps: 100,
    },
    {
      node_id: "C007",
      name: "C061 · 广州",
      online: false,
      suitability: "pending",
      reasons: ["节点离线，无法采集盘位信息"],
      candidates: [],
      rejected: [],
      collected_at: null,
      gaps: [],
      link_mbps: null,
    },
  ];
  const checks: Check[] = [
    { name: "候选节点不少于 4 台", passed: true, detail: "4 台已确认可用" },
    { name: "各节点磁盘数量一致", passed: true, detail: "每节点 2 块" },
    { name: "磁盘容量基本一致", passed: true, detail: "最小 1.8 TiB，最大 1.8 TiB" },
    { name: "介质类型一致", passed: true, detail: "全部为固态盘" },
    { name: "节点链路不低于 100 Mbps", passed: true, detail: "链路速率满足要求，或未采集到速率信息" },
    {
      name: "主机信息采集完整",
      passed: false,
      detail: "以下节点存在采集缺口：C071 · 美国（1 项）",
    },
  ];
  return {
    generated_at: Math.floor(Date.now() / 1000),
    nodes,
    checks,
    topology: null,
    ready: false,
    // 与 hub 的优先级一致：只要还有节点没采集，就先说结论不完整
    summary:
      "还有 1 个节点尚未采集到盘位信息，暂不能给出结论；先触发一次主机盘点。",
  };
}

onMounted(() => {
  load();
  loadClusters();
});
</script>
<template>
  <StatusBar
    ><span
      >节点 <strong>{{ counts.total }}</strong></span
    ><Status tone="success">适合 {{ counts.suitable }}</Status
    ><Status :tone="counts.pending ? 'neutral' : 'success'" mark="unknown"
      >待采集 {{ counts.pending }}</Status
    ><span
      >候选磁盘 <strong>{{ counts.drives }}</strong></span
    ><span v-if="report?.topology"
      >可用容量 <strong>{{ bytes(report.topology.usable_capacity) }}</strong></span
    ><template #end
      ><Button :disabled="busy" @click="inspectAll"
        ><Search />全部节点盘点</Button
      ><Button :disabled="busy" @click="load"><RefreshCw />刷新结论</Button></template
    ></StatusBar
  >
  <Notice v-if="error" tone="danger">{{ error }}</Notice>
  <Notice v-if="notice">{{ notice }}</Notice>
  <Notice v-if="w.demo">演示 · 静态预检结论，未连接服务器</Notice>

  <section v-if="report" class="storage-verdict">
    <header>
      <Status
        :tone="report.ready ? 'success' : 'warning'"
        :mark="report.ready ? 'active' : 'pending'"
        >{{ report.ready ? "可以进入部署计划" : "暂不满足部署条件" }}</Status
      >
      <span>{{ report.summary }}</span>
    </header>
    <p class="muted">
      预检只读：不格式化、不挂载、不写任何磁盘。判断集中在控制台完成，
      节点侧仅带回事实，因此结论可以被复核。
    </p>
  </section>

  <section v-if="report" class="storage-checks">
    <h2>一致性检查</h2>
    <ul>
      <li v-for="check in report.checks" :key="check.name">
        <Status
          :tone="check.passed ? 'success' : 'warning'"
          :mark="check.passed ? 'active' : 'pending'"
          >{{ check.passed ? "通过" : "未通过" }}</Status
        >
        <span class="check-name">{{ check.name }}</span>
        <span class="muted">{{ check.detail }}</span>
      </li>
    </ul>
  </section>

  <section v-if="report?.topology" class="settings-section">
    <h2>推荐拓扑</h2>
    <p>
      {{ report.topology.nodes }} 个节点 × {{ report.topology.drives_per_node }} 块盘 ·
      每块 {{ bytes(report.topology.drive_size) }}
    </p>
    <p class="muted">
      裸容量 {{ bytes(report.topology.raw_capacity) }}，可用容量
      {{ bytes(report.topology.usable_capacity) }}。{{ report.topology.parity_note }}。
    </p>
  </section>

  <section class="storage-nodes">
    <h2>逐节点结论</h2>
    <article
      v-for="node in report?.nodes || []"
      :key="node.node_id"
      class="storage-node"
    >
      <header @click="expanded = expanded === node.node_id ? '' : node.node_id">
        <Status :tone="tone(node.suitability)" :mark="mark(node.suitability)">{{
          suitabilityLabel[node.suitability]
        }}</Status>
        <strong>{{ node.name }}</strong>
        <Tag v-if="!node.online">离线</Tag>
        <Tag v-if="node.link_mbps">{{ node.link_mbps }} Mbps</Tag>
        <span class="muted"
          >候选磁盘 {{ node.candidates.length }} 块 · 已排除
          {{ node.rejected.length }} 块</span
        >
      </header>
      <ul class="storage-reasons">
        <li v-for="reason in node.reasons" :key="reason">{{ reason }}</li>
      </ul>
      <div v-if="expanded === node.node_id" class="storage-detail">
        <div v-if="node.candidates.length">
          <h3>可用于存储集群</h3>
          <table class="file-table">
            <thead>
              <tr>
                <th>设备</th>
                <th>容量</th>
                <th>介质</th>
                <th>型号</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="disk in node.candidates" :key="disk.device">
                <td class="mono">{{ disk.path }}</td>
                <td class="mono">{{ bytes(disk.size) }}</td>
                <td>{{ disk.rotational ? "机械盘" : "固态盘" }}</td>
                <td class="muted">{{ disk.model || "未提供" }}</td>
              </tr>
            </tbody>
          </table>
        </div>
        <div v-if="node.rejected.length">
          <h3>已排除的磁盘</h3>
          <table class="file-table">
            <thead>
              <tr>
                <th>设备</th>
                <th>原因</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="disk in node.rejected" :key="disk.device">
                <td class="mono">{{ disk.device }}</td>
                <td>{{ disk.reason }}</td>
              </tr>
            </tbody>
          </table>
        </div>
        <p v-if="node.gaps.length" class="tone-warning">
          采集缺口：{{ node.gaps.join("；") }}
        </p>
        <p v-if="node.collected_at" class="muted">
          采集于
          {{ new Date(node.collected_at * 1000).toLocaleString("zh-CN", { hour12: false }) }}
        </p>
      </div>
      <footer>
        <Button :disabled="busy || w.demo" @click="inspect(node.node_id)"
          ><Search />在该节点盘点</Button
        >
        <span class="muted">点击标题展开磁盘明细</span>
      </footer>
    </article>
  </section>

  <section class="settings-section">
    <h2>集群定义</h2>
    <p class="muted">
      定义本身不碰任何节点。生成计划时会再核一次现实：节点必须在线、磁盘必须仍处于
      「无使用痕迹」的状态，任何一处不符都会拒绝生成。
    </p>
    <Field label="集群名称"
      ><Input v-model="spec.name" aria-label="集群名称" :maxlength="40"
    /></Field>
    <Field label="镜像标签">
      <Input
        v-model="spec.image_tag"
        aria-label="镜像标签"
        :placeholder="`RELEASE.<时间戳>`"
      />
      <span class="muted">仓库固定为 {{ limits.image_prefix }}，不接受 latest</span>
    </Field>
    <Field label="对外服务地址"
      ><Input
        v-model="spec.endpoint"
        aria-label="对外服务地址"
        placeholder="https://s3.example.com"
    /></Field>
    <Field label="访问凭据">
      <Input v-model="spec.access_key" aria-label="访问密钥 ID" placeholder="密钥 ID" />
      <Input
        v-model="spec.secret_key"
        aria-label="访问密钥"
        type="password"
        placeholder="密钥（至少 16 位）"
      />
    </Field>
    <Field label="参与节点">
      <div class="node-picker">
        <Checkbox
          v-for="node in selectableNodes"
          :key="node.node_id"
          :model-value="spec.nodes.includes(node.node_id)"
          @update:model-value="
            (checked: boolean) =>
              (spec.nodes = checked
                ? [...spec.nodes, node.node_id]
                : spec.nodes.filter((id) => id !== node.node_id))
          "
          >{{ node.name }}（{{ node.candidates.length }} 块盘）</Checkbox
        >
      </div>
      <span class="muted"
        >只列出预检判定为「适合部署」的节点，至少 {{ limits.min_nodes }} 台</span
      >
    </Field>
    <Field label="操作">
      <Button :disabled="busy" @click="saveCluster">保存定义</Button>
      <Button variant="primary" :disabled="busy || spec.nodes.length < limits.min_nodes" @click="makePlan"
        >生成部署计划</Button
      >
    </Field>
  </section>

  <section v-if="plan" class="storage-plan" :class="{ 'is-destructive': plan.destructive }">
    <header>
      <h2>部署计划</h2>
      <Status tone="danger" mark="failed">含不可逆步骤</Status>
      <span class="muted"
        >{{ plan.nodes.length }} 个节点 · {{ deviceCount }} 块盘 ·
        {{ planExpired ? "已过期" : `有效至 ${new Date(plan.expires * 1000).toLocaleTimeString("zh-CN", { hour12: false })}` }}</span
      >
    </header>
    <p class="storage-notice">{{ plan.irreversible_notice }}</p>
    <table class="file-table">
      <thead>
        <tr>
          <th>节点</th>
          <th>将被清空的设备</th>
          <th>容量</th>
          <th>挂载点</th>
        </tr>
      </thead>
      <tbody>
        <template v-for="node in plan.nodes" :key="node.node_id">
          <tr v-for="device in node.devices" :key="device.path">
            <td>{{ node.name }}</td>
            <td class="mono tone-danger">{{ device.path }}</td>
            <td class="mono">{{ bytes(device.expected_size) }}</td>
            <td class="mono">{{ device.mount }}</td>
          </tr>
        </template>
      </tbody>
    </table>
    <p class="muted">
      计划指纹 <code class="mono">{{ plan.fingerprint.slice(0, 16) }}…</code> ·
      镜像 <code class="mono">{{ plan.image }}</code>。
      执行前节点会重新核对每块盘的容量与状态；任一不符即整节点拒绝，一块盘都不会动。
      逐节点串行执行，任一节点失败即阻断后续节点。
    </p>
    <Field label="执行">
      <Checkbox v-model="acknowledged"
        >我已核对上述设备清单，确认这些磁盘上的数据可以被清除</Checkbox
      >
      <Button
        variant="danger"
        :disabled="busy || !acknowledged || planExpired"
        @click="applyPlan"
        >执行计划</Button
      >
    </Field>
    <ul v-if="applied.length" class="storage-reasons">
      <li v-for="step in applied" :key="step.task_id">
        已下发：{{ step.node }} · {{ step.step }} · 任务 {{ step.task_id.slice(0, 8) }}
      </li>
    </ul>
  </section>

  <Notice v-if="w.demo" tone="warning">
    演示模式：上方集群定义与计划均为空白，不会生成计划，也不会下发任何任务。
  </Notice>
  <Notice v-if="clusters.length" tone="warning">
    已定义的集群：{{ clusters.map((c) => c.name).join("、") }}。
    同一集群重复部署会覆盖 Compose 文件，请先确认不需要保留现有配置。
  </Notice>

  <Notice tone="warning">
    格式化是全系统最危险的操作，因此预检**只批准没有任何先前使用痕迹的磁盘**：
    挂着文件系统、有分区表、是系统盘，或属于 LVM／RAID／加密卷的盘都会被排除。
    用过但已清理的盘请先 <code class="mono">wipefs</code> 再重新盘点。
    未采集到盘位的节点显示「待采集」，不代表不适合。
  </Notice>
</template>
