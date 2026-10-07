<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import {
  chooseStep,
  formatBytes,
  formatPercent,
  formatUptime,
  segmentSeries,
  usagePercent,
} from "./resourcePresentation";
import ShareChart from "./ShareChart.vue";

/**
 * 分享页。只读，且只使用 `/share/{令牌}/data/` 下的接口。
 * 这里**不加载控制台的工作台状态、不发送任何变更请求**，
 * 令牌之外不持有任何凭据。
 */
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

/** 令牌是路径的第二段；页面的一切请求都以它为前缀。 */
const segment = location.pathname.split("/").filter(Boolean);
const token = segment[1] || "";
const prefix = `/share/${token}`;

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

/** 统一处理非 2xx：后端会用中文错误说明为什么不可访问。 */
async function read(response: Response) {
  const data = await response.json().catch(() => ({}));
  if (!response.ok) throw new Error(data.error || `请求失败：${response.status}`);
  return data;
}

async function loadSeries() {
  if (!selected.value) return;
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
const bytes = (v?: number | null) => formatBytes(v ?? null);const percent = (v?: number | null) => formatPercent(v ?? null);
const perSecond = (v?: number | null) =>
  v == null ? "—" : `${formatBytes(v)}/s`;
const memoryPercent = (n: PublicNode) => usagePercent(n.memory_used, n.memory_total);
const diskPercent = (n: PublicNode) => usagePercent(n.disk_used, n.disk_total);
function barStyle(value: number | null) {
  return { width: `${value == null ? 0 : Math.min(100, Math.max(0, value))}%` };
}
let timer: ReturnType<typeof setInterval> | undefined;
const media = matchMedia("(prefers-color-scheme: dark)");

onMounted(() => {
  applyTheme();
  media.addEventListener("change", applyTheme);
  load();
  timer = setInterval(load, 30000);
});
onUnmounted(() => {
  if (timer) clearInterval(timer);
  media.removeEventListener("change", applyTheme);
});
</script>
<template>
  <div class="share-shell">
    <header class="share-header">
      <div>
        <h1>{{ site.name }}</h1>
        <p class="muted">{{ site.description }}</p>
      </div>
      <div class="inline">
        <span class="muted"
          >在线 {{ summary.online_count }} / {{ summary.node_count }}</span
        >
        <select v-model="theme" aria-label="明暗主题" class="ui-select">
          <option value="system">跟随系统</option>
          <option value="light">亮色</option>
          <option value="dark">深色</option>
        </select>
      </div>
    </header>

    <p v-if="loading" class="muted">正在加载…</p>
    <p v-else-if="error" class="share-error" role="alert">{{ error }}</p>
    <p v-else-if="!nodes.length" class="muted">
      当前没有可展示的节点。可能是分享范围被限定，或所有节点都被标记为隐藏。
    </p>

    <section v-for="group in groups" :key="group.name" class="share-group">
      <h2>{{ group.name }}</h2>
      <div class="share-cards">
        <article
          v-for="node in group.list"
          :key="node.id"
          class="share-card"
          :class="{ 'is-selected': selected === node.id }"
        >
          <header>
            <button type="button" class="share-name" @click="select(node.id)">
              {{ node.name }}
            </button>
            <span
              class="ui-status"
              :class="node.online ? 'tone-success' : 'tone-warning'"
              ><span
                class="status-dot"
                aria-hidden="true"
                :style="{
                  background: node.online ? 'var(--positive)' : 'var(--warning)',
                }"
              />{{ node.online ? "在线" : "离线" }}</span
            >
          </header>
          <p v-if="node.region || node.remark" class="muted share-meta">
            <span v-if="node.region">{{ node.region }}</span>
            <span v-if="node.remark">{{ node.remark }}</span>
          </p>
          <dl class="share-metrics">
            <div>
              <dt>CPU</dt>
              <dd>{{ percent(node.cpu_usage) }}</dd>
              <div class="share-bar">
                <span :style="barStyle(node.cpu_usage)" />
              </div>
            </div>
            <div>
              <dt>内存</dt>
              <dd>{{ percent(memoryPercent(node)) }}</dd>
              <div class="share-bar">
                <span
                  :style="barStyle(memoryPercent(node))"
                />
              </div>
            </div>
            <div>
              <dt>磁盘</dt>
              <dd>{{ percent(diskPercent(node)) }}</dd>
              <div class="share-bar">
                <span :style="barStyle(diskPercent(node))" />
              </div>
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
          <p v-if="node.metrics_at == null" class="muted share-foot">
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
        <h2>{{ selectedNode.name }} · 历史</h2>
        <div class="inline">
          <button
            v-for="r in RANGES"
            :key="r.value"
            type="button"
            class="ui-button"
            :class="{ 'is-primary': rangeHours === r.value }"
            @click="
              rangeHours = r.value;
              loadSeries();
            "
          >
            {{ r.label }}
          </button>
        </div>
      </header>
      <div class="charts">
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
      <p class="muted">
        层级 {{ seriesTier || "—" }} · 步长 {{ seriesStep || "—" }} 秒。没有数据的时段曲线会断开，不做补点。
      </p>
    </section>

    <footer class="share-footer">
      <p class="muted">
        {{ site.footer || "本页为只读状态展示，不包含任何管理入口。" }}
      </p>
      <p class="muted">
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
