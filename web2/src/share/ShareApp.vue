<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import {
  chooseStep,
  formatBytes,
  formatPercent,
  formatUptime,
  segmentSeries,
  usagePercent,
} from "../resourcePresentation";
import ShareChart from "./ShareChart.vue";

type PublicNode = {
  id: string;
  name: string;
  online: boolean;
  region: string | null;
  group: string | null;
  tags: string[];
  weight: number;
  remark: string | null;
  cpu_usage: number | null;
  memory_used: number | null;
  memory_total: number | null;
  disk_used: number | null;
  disk_total: number | null;
  load1: number | null;
  net_rx_bytes_per_second: number | null;
  net_tx_bytes_per_second: number | null;
  uptime: number | null;
  metrics_at: number | null;
};
type Site = { name: string; description: string; footer: string };

const segment = location.pathname.split("/").filter(Boolean);
const token = segment[1] || "";
const prefix = `/share/${token}`;
const demo = new URLSearchParams(location.search).has("demo");

const site = ref<Site>({ name: "opsd", description: "节点状态", footer: "" });
const nodes = ref<PublicNode[]>([]);
const summary = ref<{ node_count: number; online_count: number; generated_at: number }>(
  { node_count: 0, online_count: 0, generated_at: 0 },
);
const error = ref("");
const loading = ref(true);
const selected = ref("");
const theme = ref(localStorage.getItem("opsd.share.theme") || "system");

const series = ref<any[]>([]);
const seriesTier = ref("");
const seriesStep = ref(0);
const rangeHours = ref(6);
const RANGES = [
  { value: 1, label: "1 小时" },
  { value: 6, label: "6 小时" },
  { value: 24, label: "24 小时" },
  { value: 72, label: "3 天" },
];

function applyTheme() {
  document.documentElement.dataset.theme =
    theme.value === "system"
      ? matchMedia("(prefers-color-scheme: dark)").matches
        ? "dark"
        : "light"
      : theme.value;
  localStorage.setItem("opsd.share.theme", theme.value);
}

async function load() {
  if (demo) {
    site.value = { name: "十二节点状态", description: "静态演示 · 不连接服务器", footer: "" };
    nodes.value = demoNodes();
    summary.value = {
      node_count: nodes.value.length,
      online_count: nodes.value.filter((n) => n.online).length,
      generated_at: Math.floor(Date.now() / 1000),
    };
    document.title = `${site.value.name} · ${site.value.description}`;
    loading.value = false;
    return;
  }
  try {
    const [s, n] = await Promise.all([
      fetch(`${prefix}/data/summary`).then(read),
      fetch(`${prefix}/data/nodes`).then(read),
    ]);
    site.value = s.site || site.value;
    summary.value = s;
    nodes.value = n.nodes || [];
    document.title = `${site.value.name} · ${site.value.description}`;
    error.value = "";
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e);
  } finally {
    loading.value = false;
  }
}

async function read(response: Response) {
  const data = await response.json().catch(() => ({}));
  if (!response.ok) throw new Error(data.error || `请求失败：${response.status}`);
  return data;
}

async function loadSeries() {
  if (!selected.value) return;
  if (demo) {
    const to = Math.floor(Date.now() / 1000);
    const span = rangeHours.value * 3600;
    const count = 100;
    const seed = selected.value.charCodeAt(selected.value.length - 1) || 1;
    series.value = Array.from({ length: count }, (_, i) => {
      const at = Math.round(to - span + (i * span) / count);
      return {
        at,
        cpu_usage: 15 + 20 * Math.abs(Math.sin(i / 6 + seed)),
        memory_used: 3.5e9 + 6e8 * Math.abs(Math.sin(i / 9 + seed)),
        net_rx_bytes_per_second: 2e6 * Math.abs(Math.sin(i / 5 + seed)),
        net_tx_bytes_per_second: 1e6 * Math.abs(Math.sin(i / 8 + seed)),
      };
    });
    seriesTier.value = "演示";
    seriesStep.value = Math.round(span / count);
    return;
  }
  const to = Math.floor(Date.now() / 1000);
  const from = to - rangeHours.value * 3600;
  const step = chooseStep(to - from);
  try {
    const data = await fetch(
      `${prefix}/data/records?node=${encodeURIComponent(selected.value)}&from=${from}&to=${to}&step=${step}`,
    ).then(read);
    series.value = data.points || [];
    seriesTier.value = data.tier || "";
    seriesStep.value = data.step || step;
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e);
  }
}

function select(id: string) {
  selected.value = selected.value === id ? "" : id;
  series.value = [];
  if (selected.value) loadSeries();
}

const selectedNode = computed(
  () => nodes.value.find((n) => n.id === selected.value) || null,
);
const groups = computed(() => {
  const map = new Map<string, PublicNode[]>();
  for (const node of nodes.value) {
    const key = node.group || "未分组";
    if (!map.has(key)) map.set(key, []);
    map.get(key)!.push(node);
  }
  return Array.from(map, ([name, list]) => ({ name, list }));
});
const bytes = (v?: number | null) => formatBytes(v ?? null);
const percent = (v?: number | null) => formatPercent(v ?? null);
const perSecond = (v?: number | null) =>
  v == null ? "—" : `${formatBytes(v)}/s`;
const memoryPercent = (n: PublicNode) => usagePercent(n.memory_used, n.memory_total);
const diskPercent = (n: PublicNode) => usagePercent(n.disk_used, n.disk_total);
function barWidth(value: number | null) {
  return `${value == null ? 0 : Math.min(100, Math.max(0, value))}%`;
}
let timer: ReturnType<typeof setInterval> | undefined;
const media = matchMedia("(prefers-color-scheme: dark)");

function demoNodes(): PublicNode[] {
  const rows: [string, string, string, boolean][] = [
    ["C001", "美国 · 洛杉矶", "数据库", true],
    ["C011", "台湾", "边缘", true],
    ["C021", "香港", "边缘", true],
    ["C041", "美国 · 佛罗里达", "数据库", true],
    ["C051", "广州", "监控", true],
    ["C052", "日本 · 东京", "数据库", true],
    ["C061", "广州", "边缘", false],
    ["C062", "深圳", "边缘", true],
    ["C071", "美国 · 洛杉矶", "数据库", true],
    ["C081", "新加坡", "边缘", true],
    ["C091", "日本 · 东京", "数据库", true],
    ["C101", "香港", "管理", true],
  ];
  return rows.map(([id, region, group, online], index) => {
    const seed = index + 3;
    const memoryTotal = 8 * 1024 ** 3;
    return {
      id,
      name: id,
      online,
      region,
      group,
      tags: group === "数据库" ? ["galera"] : [],
      weight: 0,
      remark: null,
      cpu_usage: 12 + ((seed * 7) % 45),
      memory_used: memoryTotal * (0.3 + ((seed * 3) % 40) / 100),
      memory_total: memoryTotal,
      disk_used: (30 + ((seed * 11) % 50)) * 1024 ** 3,
      disk_total: 100 * 1024 ** 3,
      load1: 0.4 + ((seed * 5) % 20) / 10,
      net_rx_bytes_per_second: (2 + (seed % 20)) * 1024 ** 2,
      net_tx_bytes_per_second: (1 + (seed % 12)) * 1024 ** 2,
      uptime: 3600 * (24 * (seed % 90) + (seed % 24)),
      metrics_at: Math.floor(Date.now() / 1000) - (online ? 5 : 900),
    };
  });
}

onMounted(() => {
  applyTheme();
  media.addEventListener("change", applyTheme);
  load();
  if (!demo) timer = setInterval(load, 30000);
});
onUnmounted(() => {
  if (timer) clearInterval(timer);
  media.removeEventListener("change", applyTheme);
});
</script>
<template>
  <div class="share-shell">
    <header class="share-header">
      <div class="share-title">
        <span class="share-number">00</span>
        <div>
          <h1>{{ site.name }}</h1>
          <p class="share-desc">{{ site.description }}</p>
        </div>
      </div>
      <div class="share-controls">
        <span class="share-summary"
          >在线 {{ summary.online_count }} / {{ summary.node_count }}</span
        >
        <select v-model="theme" aria-label="明暗主题" class="share-select">
          <option value="system">跟随系统</option>
          <option value="light">亮色</option>
          <option value="dark">深色</option>
        </select>
      </div>
    </header>

    <p v-if="loading" class="share-loading">正在加载…</p>
    <div v-else-if="error" class="share-error" role="alert">{{ error }}</div>
    <p v-else-if="!nodes.length" class="share-loading">
      当前没有可展示的节点。可能是分享范围被限定，或所有节点都被标记为隐藏。
    </p>

    <section v-for="(group, gi) in groups" :key="group.name" class="share-group">
      <h2><span class="share-group-num">{{ String(gi + 1).padStart(2, "0") }}</span>{{ group.name }}</h2>
      <div class="share-cards">
        <article
          v-for="node in group.list"
          :key="node.id"
          class="share-card"
          :class="{ 'is-selected': selected === node.id }"
        >
          <header class="share-card-head">
            <button type="button" class="share-name" @click="select(node.id)">
              {{ node.name }}
            </button>
            <span
              class="share-status"
              :class="node.online ? 'is-online' : 'is-offline'"
            ><span class="share-dot" />{{ node.online ? "在线" : "离线" }}</span
            >
          </header>
          <p v-if="node.region || node.remark" class="share-meta">
            <span v-if="node.region">{{ node.region }}</span>
            <span v-if="node.remark">{{ node.remark }}</span>
          </p>
          <dl class="share-metrics">
            <div>
              <dt>CPU</dt>
              <dd>{{ percent(node.cpu_usage) }}</dd>
              <div class="share-bar"><span :style="{ width: barWidth(node.cpu_usage) }" /></div>
            </div>
            <div>
              <dt>内存</dt>
              <dd>{{ percent(memoryPercent(node)) }}</dd>
              <div class="share-bar"><span :style="{ width: barWidth(memoryPercent(node)) }" /></div>
            </div>
            <div>
              <dt>磁盘</dt>
              <dd>{{ percent(diskPercent(node)) }}</dd>
              <div class="share-bar"><span :style="{ width: barWidth(diskPercent(node)) }" /></div>
            </div>
          </dl>
          <dl class="share-facts">
            <div>
              <dt>负载</dt>
              <dd>{{ node.load1 == null ? "—" : node.load1.toFixed(2) }}</dd>
            </div>
            <div>
              <dt>入站</dt>
              <dd>{{ perSecond(node.net_rx_bytes_per_second) }}</dd>
            </div>
            <div>
              <dt>出站</dt>
              <dd>{{ perSecond(node.net_tx_bytes_per_second) }}</dd>
            </div>
            <div>
              <dt>运行</dt>
              <dd>{{ formatUptime(node.uptime) }}</dd>
            </div>
          </dl>
          <p v-if="node.metrics_at == null" class="share-foot">
            该节点尚未上报指标，因此不显示读数。
          </p>
          <ul v-if="node.tags.length" class="share-tags">
            <li v-for="tag in node.tags" :key="tag">{{ tag }}</li>
          </ul>
        </article>
      </div>
    </section>

    <section v-if="selectedNode" class="share-detail">
      <header class="share-header">
        <h2><span class="share-group-num">—</span>{{ selectedNode.name }} · 历史</h2>
        <div class="share-ranges">
          <button
            v-for="r in RANGES"
            :key="r.value"
            type="button"
            class="share-range-btn"
            :class="{ 'is-active': rangeHours === r.value }"
            @click="rangeHours = r.value; loadSeries();"
          >{{ r.label }}</button>
        </div>
      </header>
      <div class="share-charts">
        <ShareChart
          :points="series"
          field="cpu_usage"
          :step="seriesStep || 300"
          :max="100"
          label="CPU 使用率"
          :format="percent"
        />
        <ShareChart
          :points="series"
          field="memory_used"
          :step="seriesStep || 300"
          label="内存占用"
          :format="bytes"
        />
        <ShareChart
          :points="series"
          field="net_rx_bytes_per_second"
          :step="seriesStep || 300"
          label="入站速率"
          :format="perSecond"
        />
        <ShareChart
          :points="series"
          field="net_tx_bytes_per_second"
          :step="seriesStep || 300"
          label="出站速率"
          :format="perSecond"
        />
      </div>
      <p class="share-foot">
        层级 {{ seriesTier || "—" }} · 步长 {{ seriesStep || "—" }} 秒。没有数据的时段曲线会断开，不做补点。
      </p>
    </section>

    <footer class="share-footer">
      <p>{{ site.footer || "本页为只读状态展示，不包含任何管理入口。" }}</p>
      <p>
        更新于
        {{
          summary.generated_at
            ? new Date(summary.generated_at * 1000).toLocaleString("zh-CN", {
                hour12: false,
              })
            : "—"
        }}
      </p>
    </footer>
  </div>
</template>
