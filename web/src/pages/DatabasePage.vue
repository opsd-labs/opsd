<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useWorkspace } from "../workspace";
import { api } from "../api";
import Button from "../components/ui/Button.vue";
import Notice from "../components/ui/Notice.vue";
import Status from "../components/ui/Status.vue";
import Tag from "../components/ui/Tag.vue";
import StatusBar from "../components/resources/StatusBar.vue";
import { Activity, RefreshCw } from "lucide-vue-next";
import {
  dbAgeHeader,
  dbBinlog,
  dbBytes,
  dbDuration,
  dbLatency,
  dbOnOff,
  dbRatio,
  dbSince,
  dbToneColor,
  dbToneText,
  dbUntil,
  dbValue,
} from "../databasePresentation";

/**
 * 数据库只读运维视图。
 *
 * 这一页回答的问题是「五节点 Galera 集群现在到底怎么样」，因此：
 *
 * - **只读**：页面上没有任何写入口。不建库、不建用户、不改配置、不触发 SST、
 *   不 kill 查询。语句固定在 Agent 里，主控无法影响它们。
 * - **未知是一等状态**：连不上、没配置、没权限都显示「未知」并给出原因，
 *   绝不退化成 0 或「正常」。把采集失败显示成平静的绿色，是运维视图最危险的假象。
 * - **结论在控制台**：阈值与告警规则一致，所以这一页与告警不会互相矛盾。
 */
type Finding = {
  tone: "ok" | "warning" | "critical";
  title: string;
  detail: string;
};
type Section<T> = { data: T | null; reason: string | null };
type Galera = {
  cluster_status: string;
  cluster_size: number | null;
  expected_cluster_size: number;
  local_state: number | null;
  local_state_comment: string;
  ready: boolean | null;
  connected: boolean | null;
  desync: boolean | null;
  flow_control_paused: number | null;
  last_committed: number | null;
  recv_queue: number | null;
  send_queue: number | null;
  cert_failures: number | null;
  bf_aborts: number | null;
  cluster_state_uuid: string | null;
  node_uuid: string | null;
  sst_donor: string | null;
  provider_version: string | null;
  incoming: string[];
};
type DbHost = {
  version: string;
  version_comment: string;
  hostname: string | null;
  server_id: number | null;
  uptime: number | null;
  threads_connected: number | null;
  threads_running: number | null;
  read_only: boolean | null;
  binlog_file: string | null;
  binlog_position: number | null;
};
type Server = {
  hostgroup_id: number;
  hostname: string;
  port: number;
  status: string;
  weight: number;
  max_connections: number;
  latency_us: number | null;
  queries: number | null;
  conn_used: number | null;
  conn_free: number | null;
};
type Pool = {
  hostgroup: number;
  srv_host: string;
  srv_port: number;
  conn_used: number | null;
  conn_free: number | null;
  conn_err: number | null;
  queries: number | null;
  latency_us: number | null;
};
type Digest = {
  digest: string;
  hostgroup: number;
  schemaname: string;
  count_star: number;
  sum_time: number;
  max_time: number;
};
type ProxySql = {
  version: string | null;
  uptime: number | null;
  writer_online: number | null;
  backup_writer_online: number | null;
  servers: Server[];
  pools: Pool[];
  digests: Digest[];
};
type Tier = {
  tier: string;
  last_success: number | null;
  age_seconds: number | null;
  size_bytes: number | null;
  directory: string;
  files: number;
  has_checksums: boolean;
  age_header_ok: boolean | null;
};
type Backup = {
  root: string;
  tiers: Tier[];
  next_run: number | null;
  last_trigger: number | null;
};
type Report = {
  collected_at: number;
  configured: boolean;
  galera: Section<Galera>;
  host: Section<DbHost>;
  proxysql: Section<ProxySql>;
  backup: Section<Backup>;
  gaps: string[];
};
type NodeVerdict = {
  node_id: string;
  name: string;
  online: boolean;
  collected_at: number | null;
  configured: boolean;
  tone: "ok" | "warning" | "critical";
  findings: Finding[];
  unknown: string[];
  report: Report | null;
};
type Overview = {
  generated_at: number;
  nodes: NodeVerdict[];
  checks: Finding[];
  summary: string;
  expected_cluster_size: number;
  tone: "ok" | "warning" | "critical";
  unknown_count: number;
};

const w = useWorkspace();
const overview = ref<Overview | null>(null);
const error = ref("");
const notice = ref("");
const busy = ref(false);
const expanded = ref("");

/* 展示规则集中在 databasePresentation.ts：那里的单元测试守着
   「未知不等于 0，也不等于正常」这条约束，页面只负责调用。 */
const toneOf = dbToneColor;
const toneText = dbToneText;
const value = dbValue;
const onOff = dbOnOff;
const duration = dbDuration;
const since = dbSince;
const until = dbUntil;
const bytes = dbBytes;
const latency = dbLatency;
const ratio = dbRatio;
const binlog = dbBinlog;
const ageHeader = dbAgeHeader;

/** 顶栏汇总只统计**已知**的节点，未知单独计数。 */
const counts = computed(() => {
  const nodes = overview.value?.nodes || [];
  const reports = nodes
    .map((node) => node.report?.galera.data)
    .filter((galera): galera is Galera => !!galera);
  return {
    total: nodes.length,
    inspected: nodes.filter((node) => node.collected_at).length,
    synced: reports.filter(
      (galera) => galera.ready === true && galera.local_state === 4,
    ).length,
    notReady: reports.filter(
      (galera) => galera.ready === false || galera.local_state !== 4,
    ).length,
    writers: nodes
      .map((node) => node.report?.proxysql.data?.writer_online)
      .filter((value): value is number => value !== null && value !== undefined),
    backup: (() => {
      const ages = nodes
        .flatMap((node) => node.report?.backup.data?.tiers || [])
        .filter((tier) => tier.tier === "hourly")
        .map((tier) => tier.age_seconds)
        .filter((age): age is number => age !== null && age !== undefined);
      return ages.length ? Math.min(...ages) : null;
    })(),
  };
});

async function load() {
  busy.value = true;
  error.value = "";
  try {
    overview.value = w.demo ? demoOverview() : await api("/database/overview");
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
      notice.value = "演示模式：未提交巡检任务";
      return;
    }
    const result = await api(`/nodes/${nodeId}/db-inspect`, "POST", {
      idempotency_key: crypto.randomUUID(),
    });
    notice.value = `已提交只读巡检任务 ${result.task_id.slice(0, 8)}；执行完成后刷新即可看到结果`;
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e);
  }
}

async function inspectAll() {
  for (const node of (overview.value?.nodes || []).filter((n) => n.online))
    await inspect(node.node_id);
}

/** 巡检任务状态一变就重新取结论，不必手动刷新。 */
const inspectSignature = computed(() =>
  w.tasks
    .filter((task: any) => task.action?.type === "db_inspect")
    .map((task: any) => `${task.id}:${task.status}`)
    .join(","),
);
watch(inspectSignature, (now, before) => {
  if (before !== undefined && now !== before) load();
});
onMounted(load);

/** 演示数据：五种状态都出现，且刻意留出「未知」，避免演示变成一片绿。 */
function demoOverview(): Overview {
  const galera = (extra: Partial<Galera> = {}): Section<Galera> => ({
    reason: null,
    data: {
      cluster_status: "Primary",
      cluster_size: 5,
      expected_cluster_size: 5,
      local_state: 4,
      local_state_comment: "Synced",
      ready: true,
      connected: true,
      desync: false,
      flow_control_paused: 0,
      last_committed: 18446744073709551000,
      recv_queue: 0,
      send_queue: 0,
      cert_failures: 0,
      bf_aborts: 0,
      cluster_state_uuid: "6df0d1a4-1111-2222-3333-444455556666",
      node_uuid: "aaaaaaaa-0000-0000-0000-000000000001",
      sst_donor: null,
      provider_version: "26.4.20(r3b6c1f4)",
      incoming: [
        "100.100.201.1:3306",
        "100.100.201.41:3306",
        "100.100.201.52:3306",
        "100.100.201.71:3306",
        "100.100.201.91:3306",
      ],
      ...extra,
    },
  });
  /** 主机名与 server_id 必须与所在节点一致，否则演示数据本身就在骗人。 */
  const host = (id: string, serverId: number): Section<DbHost> => ({
    reason: null,
    data: {
      version: "12.2.2-MariaDB-log",
      version_comment: "Ubuntu 24.04",
      hostname: id,
      server_id: serverId,
      uptime: 1_382_400,
      threads_connected: 42,
      threads_running: 2,
      read_only: serverId !== 1,
      binlog_file: `mariadb-bin.0000${40 + serverId}`,
      binlog_position: 812_345_678,
    },
  });
  const servers: Server[] = [
    {
      hostgroup_id: 10,
      hostname: "100.100.201.1",
      port: 3306,
      status: "ONLINE",
      weight: 1000000,
      max_connections: 100,
      latency_us: 321,
      queries: 1_284_512,
      conn_used: 2,
      conn_free: 8,
    },
    {
      hostgroup_id: 20,
      hostname: "100.100.201.41",
      port: 3306,
      status: "ONLINE",
      weight: 900000,
      max_connections: 100,
      latency_us: 654,
      queries: 1_284_500,
      conn_used: 0,
      conn_free: 4,
    },
    {
      hostgroup_id: 20,
      hostname: "100.100.201.52",
      port: 3306,
      status: "ONLINE",
      weight: 900000,
      max_connections: 100,
      latency_us: 702,
      queries: 1_284_490,
      conn_used: 0,
      conn_free: 4,
    },
    {
      hostgroup_id: 20,
      hostname: "100.100.201.71",
      port: 3306,
      status: "SHUNNED",
      weight: 900000,
      max_connections: 100,
      latency_us: null,
      queries: null,
      conn_used: null,
      conn_free: null,
    },
    {
      hostgroup_id: 40,
      hostname: "100.100.201.91",
      port: 3306,
      status: "OFFLINE_HARD",
      weight: 1,
      max_connections: 100,
      latency_us: null,
      queries: null,
      conn_used: null,
      conn_free: null,
    },
  ];
  const proxysql = (extra: Partial<ProxySql> = {}): Section<ProxySql> => ({
    reason: null,
    data: {
      version: "2.6.5",
      uptime: 1_382_000,
      writer_online: 1,
      backup_writer_online: 4,
      servers,
      pools: [
        {
          hostgroup: 10,
          srv_host: "100.100.201.1",
          srv_port: 3306,
          conn_used: 2,
          conn_free: 8,
          conn_err: 0,
          queries: 1_284_512,
          latency_us: 321,
        },
        {
          hostgroup: 20,
          srv_host: "100.100.201.41",
          srv_port: 3306,
          conn_used: 0,
          conn_free: 4,
          conn_err: 1,
          queries: 1_284_500,
          latency_us: 654,
        },
      ],
      digests: [
        {
          digest: "0A1B2C3D4E5F6071",
          hostgroup: 10,
          schemaname: "app",
          count_star: 184_233,
          sum_time: 5_420_331,
          max_time: 12_004,
        },
        {
          digest: "9F8E7D6C5B4A3021",
          hostgroup: 10,
          schemaname: "vaultwarden",
          count_star: 41_002,
          sum_time: 902_113,
          max_time: 3_450,
        },
      ],
      ...extra,
    },
  });
  const tiers = (ageHourly: number, ageDaily: number): Tier[] => [
    {
      tier: "hourly",
      last_success: Math.floor(Date.now() / 1000) - ageHourly,
      age_seconds: ageHourly,
      size_bytes: 1_284_512_000,
      directory: "/data1/server/db/backups/hourly/20260910T191504Z",
      files: 3,
      has_checksums: true,
      age_header_ok: true,
    },
    {
      tier: "daily",
      last_success: Math.floor(Date.now() / 1000) - ageDaily,
      age_seconds: ageDaily,
      size_bytes: 18_442_000_000,
      directory: "/data1/server/db/backups/daily/20260910T033001Z",
      files: 4,
      has_checksums: true,
      age_header_ok: true,
    },
    {
      tier: "weekly",
      last_success: Math.floor(Date.now() / 1000) - 86_400 * 2,
      age_seconds: 86_400 * 2,
      size_bytes: 18_442_000_000,
      directory: "/data1/server/db/backups/weekly/20260907T033002Z",
      files: 4,
      has_checksums: true,
      age_header_ok: true,
    },
  ];
  const backup = (
    ageHourly: number,
    ageDaily: number,
    headerOk: boolean | null = true,
  ): Section<Backup> => ({
    reason: null,
    data: {
      root: "/data1/server/db/backups",
      tiers: tiers(ageHourly, ageDaily).map((tier) =>
        tier.tier === "daily" ? { ...tier, age_header_ok: headerOk } : tier,
      ),
      next_run: Math.floor(Date.now() / 1000) + 900,
      last_trigger: Math.floor(Date.now() / 1000) - ageHourly,
    },
  });
  const nodes: NodeVerdict[] = [
    {
      node_id: "C001",
      name: "C001 · 香港",
      online: true,
      collected_at: Math.floor(Date.now() / 1000) - 40,
      configured: true,
      tone: "ok",
      findings: [],
      unknown: [],
      report: {
        collected_at: Math.floor(Date.now() / 1000) - 40,
        configured: true,
        galera: galera(),
        host: host("C001", 1),
        proxysql: proxysql(),
        backup: backup(1_500, 40_000),
        gaps: [],
      },
    },
    {
      node_id: "C041",
      name: "C041 · 美国",
      online: true,
      collected_at: Math.floor(Date.now() / 1000) - 55,
      configured: true,
      tone: "warning",
      findings: [
        {
          tone: "warning",
          title: "接收队列积压",
          detail: "wsrep_local_recv_queue=120，超过阈值 32",
        },
      ],
      unknown: [],
      report: {
        collected_at: Math.floor(Date.now() / 1000) - 55,
        configured: true,
        galera: galera({ recv_queue: 120, node_uuid: "aaaaaaaa-0000-0000-0000-000000000041" }),
        host: host("C041", 41),
        proxysql: proxysql({ backup_writer_online: 3 }),
        backup: backup(2_100, 41_000),
        gaps: [],
      },
    },
    {
      node_id: "C052",
      name: "C052 · 新加坡",
      online: true,
      collected_at: Math.floor(Date.now() / 1000) - 30,
      configured: true,
      tone: "critical",
      findings: [
        {
          tone: "critical",
          title: "集群成员数与期望不符",
          detail: "本节点看到的成员数是 4，期望 5；可能有节点掉线或多余节点",
        },
      ],
      unknown: ["ProxySQL 未知：本机未配置这一部分的只读巡检账号"],
      report: {
        collected_at: Math.floor(Date.now() / 1000) - 30,
        configured: true,
        galera: galera({ cluster_size: 4, node_uuid: "aaaaaaaa-0000-0000-0000-000000000052" }),
        host: host("C052", 52),
        proxysql: {
          data: null,
          reason: "本机未配置这一部分的只读巡检账号",
        },
        backup: backup(2_400, 42_000),
        gaps: ["ProxySQL：本机未配置这一部分的只读巡检账号"],
      },
    },
    {
      node_id: "C071",
      name: "C071 · 法兰克福",
      online: true,
      collected_at: null,
      configured: false,
      tone: "ok",
      findings: [],
      unknown: ["尚未巡检（请先触发一次只读巡检）"],
      report: null,
    },
    {
      node_id: "C091",
      name: "C091 · 东京",
      online: false,
      collected_at: Math.floor(Date.now() / 1000) - 7_200,
      configured: true,
      tone: "critical",
      findings: [
        {
          tone: "critical",
          title: "daily 备份过期",
          detail: "最近一次成功在 112320 秒前，超过允许的 93600 秒",
        },
      ],
      unknown: ["节点当前离线，以下为 7200 秒前的巡检结果"],
      report: {
        collected_at: Math.floor(Date.now() / 1000) - 7_200,
        configured: true,
        galera: galera({ node_uuid: "aaaaaaaa-0000-0000-0000-000000000091" }),
        host: host("C091", 91),
        proxysql: proxysql(),
        backup: backup(2_000, 112_320, false),
        gaps: [],
      },
    },
  ];
  return {
    generated_at: Math.floor(Date.now() / 1000),
    nodes,
    checks: [
      {
        tone: "critical",
        title: "各节点看到的成员数不一致",
        detail: "[4, 5, 5, 5]；通常意味着有节点掉线或尚未完成状态传输",
      },
      {
        tone: "warning",
        title: "后端存在非 ONLINE 节点",
        detail: "100.100.201.71:3306=SHUNNED",
      },
    ],
    summary: "已巡检 4/5 台：3 项异常需要立即处理，2 项需要注意。",
    expected_cluster_size: 5,
    tone: "critical",
    unknown_count: 3,
  };
}
</script>
<template>
  <StatusBar
    ><span
      >已巡检 <strong>{{ counts.inspected }}/{{ counts.total }}</strong></span
    ><Status :tone="counts.notReady ? 'danger' : 'success'"
      >Ready/Synced {{ counts.synced }}</Status
    ><span
      >ProxySQL writer
      <strong>{{ counts.writers.length ? counts.writers.join("/") : "未知" }}</strong></span
    ><span
      >最近小时备份
      <strong>{{ since(counts.backup) }}</strong></span
    ><template #end
      ><Button :disabled="busy" @click="inspectAll"
        ><Activity />全部节点巡检</Button
      ><Button :disabled="busy" @click="load"><RefreshCw />刷新结论</Button></template
    ></StatusBar
  >
  <Notice v-if="error" tone="danger">{{ error }}</Notice>
  <Notice v-if="notice">{{ notice }}</Notice>
  <Notice v-if="w.demo">演示 · 静态只读结论，未连接服务器</Notice>

  <section v-if="overview" class="storage-verdict">
    <header>
      <Status
        :tone="overview.tone === 'critical' ? 'danger' : overview.tone === 'warning' ? 'warning' : 'success'"
        :mark="overview.tone === 'critical' ? 'failed' : overview.tone === 'warning' ? 'pending' : 'active'"
        >{{ toneText(overview.tone) }}</Status
      >
      <span>{{ overview.summary }}</span>
      <Tag v-if="overview.unknown_count">未知 {{ overview.unknown_count }} 项</Tag>
    </header>
    <p class="muted">
      只读巡检：不建库、不建用户、不改配置、不触发 SST、不 kill
      查询。语句固定在节点侧的 Agent 里，主控无法影响它们；
      只读账号的密码留在节点本机，既不上传也不进审计。判断集中在控制台完成，
      因此每条结论都能被复核。
    </p>
  </section>

  <section v-if="overview?.checks.length" class="storage-checks">
    <h2>跨节点一致性检查</h2>
    <ul>
      <li v-for="check in overview.checks" :key="check.title">
        <Status
          :tone="toneOf(check.tone)"
          :mark="check.tone === 'ok' ? 'active' : check.tone === 'warning' ? 'pending' : 'failed'"
          >{{ check.tone === "critical" ? "异常" : "注意" }}</Status
        >
        <span class="check-name">{{ check.title }}</span>
        <span class="muted">{{ check.detail }}</span>
      </li>
    </ul>
  </section>

  <section class="storage-nodes">
    <h2>逐节点结论</h2>
    <article
      v-for="node in overview?.nodes || []"
      :key="node.node_id"
      class="storage-node db-node"
      :class="{ 'is-unknown': !node.report }"
    >
      <header @click="expanded = expanded === node.node_id ? '' : node.node_id">
        <Status
          :tone="node.report ? toneOf(node.tone) : 'neutral'"
          :mark="node.report ? (node.tone === 'ok' ? 'active' : node.tone === 'warning' ? 'pending' : 'failed') : 'unknown'"
          >{{ node.report ? toneText(node.tone) : "尚未巡检" }}</Status
        >
        <strong>{{ node.name }}</strong>
        <Tag v-if="!node.online">离线</Tag>
        <Tag v-if="node.report && !node.configured">未配置只读账号</Tag>
        <Tag v-if="node.unknown.length">未知 {{ node.unknown.length }} 项</Tag>
        <span class="muted">
          <template v-if="node.report?.galera.data">
            {{ node.report.galera.data.local_state_comment }} ·
            {{ value(node.report.galera.data.cluster_size) }}/{{ node.report.galera.data.expected_cluster_size }} 成员
          </template>
          <template v-else>未采集到集群状态</template>
          <template v-if="node.collected_at"> · {{ since(node.collected_at) }}采集</template>
        </span>
        <span class="spacer"></span>
        <Button
          :disabled="busy || !node.online"
          @click.stop="inspect(node.node_id)"
          ><Activity />巡检</Button
        >
      </header>

      <!-- 未知项必须直接列出来：只显示语气会让人以为"没消息就是好消息" -->
      <ul v-if="node.unknown.length" class="storage-reasons db-unknown">
        <li v-for="reason in node.unknown" :key="reason">{{ reason }}</li>
      </ul>
      <ul v-if="node.findings.length" class="storage-reasons">
        <li v-for="finding in node.findings" :key="finding.title">
          <Status
            :tone="toneOf(finding.tone)"
            :mark="finding.tone === 'warning' ? 'pending' : 'failed'"
            >{{ finding.tone === "critical" ? "异常" : "注意" }}</Status
          >
          <span><strong>{{ finding.title }}</strong>：{{ finding.detail }}</span>
        </li>
      </ul>

      <div v-if="expanded === node.node_id && node.report" class="storage-detail">
        <div>
          <h3>Galera</h3>
          <table v-if="node.report.galera.data" class="file-table">
            <tbody>
              <tr>
                <th>组件状态</th>
                <td>{{ node.report.galera.data.cluster_status }}</td>
                <th>本地状态</th>
                <td>
                  {{ node.report.galera.data.local_state_comment }}（{{
                    value(node.report.galera.data.local_state)
                  }}）
                </td>
              </tr>
              <tr>
                <th>成员数</th>
                <td>
                  {{ value(node.report.galera.data.cluster_size) }} / 期望
                  {{ node.report.galera.data.expected_cluster_size }}
                </td>
                <th>就绪 / 已连接</th>
                <td>
                  {{ onOff(node.report.galera.data.ready) }} /
                  {{ onOff(node.report.galera.data.connected) }}
                </td>
              </tr>
              <tr>
                <th>接收 / 发送队列</th>
                <td>
                  {{ value(node.report.galera.data.recv_queue) }} /
                  {{ value(node.report.galera.data.send_queue) }}
                </td>
                <th>流控暂停</th>
                <td>{{ ratio(node.report.galera.data.flow_control_paused) }}</td>
              </tr>
              <tr>
                <th>认证失败 / BF 中止</th>
                <td>
                  {{ value(node.report.galera.data.cert_failures) }} /
                  {{ value(node.report.galera.data.bf_aborts) }}
                </td>
                <th>SST 捐赠者</th>
                <td>{{ node.report.galera.data.sst_donor || "未指定" }}</td>
              </tr>
              <tr>
                <th>组件 UUID</th>
                <td class="mono">
                  {{ node.report.galera.data.cluster_state_uuid || "未知" }}
                </td>
                <th>节点 UUID</th>
                <td class="mono">{{ node.report.galera.data.node_uuid || "未知" }}</td>
              </tr>
              <tr>
                <th>last_committed</th>
                <td class="mono">{{ value(node.report.galera.data.last_committed) }}</td>
                <th>wsrep 版本</th>
                <td class="muted">{{ node.report.galera.data.provider_version || "未知" }}</td>
              </tr>
            </tbody>
          </table>
          <Notice v-else tone="warning"
            >集群状态未知：{{ node.report.galera.reason || "未说明原因" }}</Notice
          >
          <p v-if="node.report.galera.data?.incoming.length" class="muted mono">
            成员地址：{{ node.report.galera.data.incoming.join("、") }}
          </p>
        </div>

        <div>
          <h3>主机侧</h3>
          <table v-if="node.report.host.data" class="file-table">
            <tbody>
              <tr>
                <th>版本</th>
                <td>
                  {{ node.report.host.data.version }}
                  <span class="muted">{{ node.report.host.data.version_comment }}</span>
                </td>
                <th>主机名</th>
                <td>{{ node.report.host.data.hostname || "未知" }}</td>
              </tr>
              <tr>
                <th>运行时长</th>
                <td>{{ duration(node.report.host.data.uptime) }}</td>
                <th>只读</th>
                <td>{{ onOff(node.report.host.data.read_only) }}</td>
              </tr>
              <tr>
                <th>连接 / 活跃线程</th>
                <td>
                  {{ value(node.report.host.data.threads_connected) }} /
                  {{ value(node.report.host.data.threads_running) }}
                </td>
                <th>binlog 位点</th>
                <td class="mono">
                  {{
                    binlog(
                      node.report.host.data.binlog_file,
                      node.report.host.data.binlog_position,
                    )
                  }}
                </td>
              </tr>
            </tbody>
          </table>
          <Notice v-else tone="warning"
            >主机侧状态未知：{{ node.report.host.reason || "未说明原因" }}</Notice
          >
        </div>

        <div>
          <h3>ProxySQL</h3>
          <template v-if="node.report.proxysql.data">
            <p class="muted">
              在线 writer
              {{ value(node.report.proxysql.data.writer_online) }} · 备用 writer
              {{ value(node.report.proxysql.data.backup_writer_online) }} · 运行
              {{ duration(node.report.proxysql.data.uptime) }}
            </p>
            <table class="file-table">
              <thead>
                <tr>
                  <th>组</th>
                  <th>后端</th>
                  <th>状态</th>
                  <th>权重</th>
                  <th>延迟</th>
                  <th>在用的连接</th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="server in node.report.proxysql.data.servers" :key="`${server.hostgroup_id}-${server.hostname}-${server.port}`">
                  <td>{{ server.hostgroup_id }}</td>
                  <td class="mono">{{ server.hostname }}:{{ server.port }}</td>
                  <td>
                    <Status
                      :tone="server.status === 'ONLINE' ? 'success' : 'warning'"
                      :mark="server.status === 'ONLINE' ? 'active' : 'pending'"
                      >{{ server.status }}</Status
                    >
                  </td>
                  <td>{{ server.weight }}</td>
                  <td>{{ latency(server.latency_us) }}</td>
                  <td>{{ value(server.conn_used) }}</td>
                </tr>
              </tbody>
            </table>
            <template v-if="node.report.proxysql.data.digests.length">
              <h4>最重的查询摘要</h4>
              <p class="muted">
                只显示摘要哈希与计数：真实 SQL 正文可能带着用户名、邮箱或令牌，因此不采集也不展示。
              </p>
              <table class="file-table">
                <thead>
                  <tr>
                    <th>摘要</th>
                    <th>库</th>
                    <th>次数</th>
                    <th>总耗时（ms）</th>
                    <th>最大耗时（ms）</th>
                  </tr>
                </thead>
                <tbody>
                  <tr v-for="digest in node.report.proxysql.data.digests" :key="digest.digest">
                    <td class="mono">{{ digest.digest.slice(0, 16) }}</td>
                    <td>{{ digest.schemaname || "—" }}</td>
                    <td>{{ digest.count_star }}</td>
                    <td>{{ (digest.sum_time / 1000).toFixed(0) }}</td>
                    <td>{{ (digest.max_time / 1000).toFixed(0) }}</td>
                  </tr>
                </tbody>
              </table>
            </template>
          </template>
          <Notice v-else tone="warning"
            >ProxySQL 未知：{{ node.report.proxysql.reason || "未说明原因" }}</Notice
          >
        </div>

        <div>
          <h3>备份链路</h3>
          <template v-if="node.report.backup.data">
            <p class="muted">
              根目录 <code class="mono">{{ node.report.backup.data.root }}</code> · 下次触发
              {{ until(node.report.backup.data.next_run) }}
            </p>
            <table class="file-table">
              <thead>
                <tr>
                  <th>层级</th>
                  <th>最近成功</th>
                  <th>体积</th>
                  <th>归档</th>
                  <th>校验清单</th>
                  <th>age 头部</th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="tier in node.report.backup.data.tiers" :key="tier.tier">
                  <td>{{ tier.tier }}</td>
                  <td>{{ since(tier.last_success) }}</td>
                  <td>{{ bytes(tier.size_bytes) }}</td>
                  <td>{{ tier.files ? `${tier.files} 个` : "无" }}</td>
                  <td>{{ tier.files ? (tier.has_checksums ? "有" : "缺失") : "—" }}</td>
                  <td>{{ ageHeader(tier.age_header_ok) }}</td>
                </tr>
              </tbody>
            </table>
            <p class="muted">
              只检查归档头部是不是 age 格式：解密私钥按设计不在节点上，因此这里不做解密校验，
              也不谎称做过。
            </p>
          </template>
          <Notice v-else tone="warning"
            >备份链路未知：{{ node.report.backup.reason || "未说明原因" }}</Notice
          >
        </div>
      </div>
    </article>
    <Notice v-if="overview && !overview.nodes.length">还没有已登记的节点。</Notice>
  </section>

  <Notice v-if="overview && overview.unknown_count" tone="warning"
    >本页有 {{ overview.unknown_count }} 项未知。未知不是正常，也不等于
    0：请先触发巡检或补齐只读账号配置，再做判断。</Notice
  >
</template>
<style scoped>
.db-node > header {
  flex-wrap: wrap;
}
.db-unknown li {
  color: var(--warning-text, inherit);
}
.spacer {
  flex: 1;
}
.storage-detail h4 {
  font-size: 12px;
  margin: 10px 0 4px;
}
</style>
