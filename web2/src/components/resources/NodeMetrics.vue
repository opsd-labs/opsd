<script setup lang="ts">
import { computed, onMounted, ref, watch, inject } from "vue";
import { workspaceKey } from "../../workspace";
import { api } from "../../api";
import Notice from "../ui/Notice.vue";
import Status from "../ui/Status.vue";
import Tag from "../ui/Tag.vue";
import Button from "../ui/Button.vue";
import MetricsChart from "./MetricsChart.vue";
import FileManager from "./FileManager.vue";
import HostShell from "./HostShell.vue";
import {
  METRICS_STALE_SECONDS,
  chooseStep,
  clockSkewExceeded,
  formatBytes,
  formatPercent,
  formatUptime,
  metricsStatus,
  usagePercent,
} from "../../resourcePresentation";

const RANGES = [
  { value: 1, label: "1h" },
  { value: 6, label: "6h" },
  { value: 24, label: "24h" },
  { value: 24 * 7, label: "7d" },
  { value: 24 * 30, label: "30d" },
];

const props = defineProps<{ nodeId: string }>();
const w = inject(workspaceKey)!;
const hours = ref(1);
const points = ref<any[]>([]);
const tier = ref("");
const step = ref(0);
const truncated = ref(false);
const loading = ref(false);
const error = ref("");
const loaded = ref(false);
const nowSeconds = ref(Math.floor(Date.now() / 1000));
setInterval(() => (nowSeconds.value = Math.floor(Date.now() / 1000)), 15000);

const entry = computed(() =>
  w.nodes.find((n) => n.environment === props.nodeId) || null,
);
const record = computed(() => w.metrics[props.nodeId] || null);
const sample = computed(() => record.value?.sample || null);
const status = computed(() =>
  metricsStatus(entry.value?.node, record.value, nowSeconds.value),
);
const skew = computed(() => clockSkewExceeded(record.value, 60));
const age = computed(() => {
  const received = record.value?.received_at;
  if (!received) return null;
  return Math.max(0, nowSeconds.value - received);
});

async function load() {
  if (!props.nodeId) return;
  loading.value = true;
  error.value = "";
  try {
    if (w.demo) {
      const base = Math.floor(Date.now() / 1000);
      const span = hours.value * 3600;
      const count = 120;
      points.value = Array.from({ length: count }, (_, i) => {
        const t = base - span + (i * span) / count;
        return {
          at: Math.round(t),
          cpu_usage: 20 + 15 * Math.sin(i / 7),
          memory_used: 3.2e9 + 4e8 * Math.sin(i / 11),
          memory_total: 8e9,
          load1: 0.4 + 0.3 * Math.sin(i / 5),
          net_rx_bytes_per_second: 4e5 + 3e5 * Math.abs(Math.sin(i / 9)),
          net_tx_bytes_per_second: 2e5 + 1.5e5 * Math.abs(Math.sin(i / 13)),
          disk_read_bytes_per_second: 1e5 * Math.abs(Math.sin(i / 17)),
          disk_write_bytes_per_second: 6e4 * Math.abs(Math.sin(i / 21)),
        };
      });
      tier.value = "演示";
      step.value = Math.round(span / count);
      truncated.value = false;
      loaded.value = true;
      return;
    }
    const to = Math.floor(Date.now() / 1000);
    const from = to - hours.value * 3600;
    const chosen = chooseStep(to - from);
    const result = await api(
      `/nodes/${props.nodeId}/metrics?from=${from}&to=${to}&step=${chosen}`,
    );
    points.value = result.points || [];
    tier.value = result.tier || "";
    step.value = result.step || chosen;
    truncated.value = !!result.truncated;
    loaded.value = true;
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e);
  } finally {
    loading.value = false;
  }
}
watch([() => props.nodeId, hours], load);
onMounted(load);

const memoryPercent = computed(() =>
  usagePercent(sample.value?.memory_used, sample.value?.memory_total),
);
const disks = computed(() => sample.value?.disks || []);
const interfaces = computed(() => sample.value?.interfaces || []);
const peers = computed(() => sample.value?.peers || []);
const peersProbedAt = computed(() => {
  const times = peers.value
    .map((peer: any) => peer.probed_at)
    .filter((at: number) => at > 0);
  return times.length ? Math.max(...times) : 0;
});
const probeAge = computed(() => {
  const age = Math.max(0, nowSeconds.value - peersProbedAt.value);
  return age < 60 ? "刚刚" : `${formatUptime(age)}前`;
});

function series(field: string): number[] {
  return points.value.map((p: any) => p[field]).filter((v: any) => isFinite(v));
}
const bytes = (value: number) => formatBytes(value);
const percent = (value: number) => formatPercent(value);
const perSecond = (value: number) => `${formatBytes(value)}/s`;
</script>

<template>
  <section class="nm">
    <header class="nm-header">
      <div>
        <h2 class="nm-title">{{ entry?.node.name || nodeId }}</h2>
        <div class="nm-badges">
          <Status
            :tone="entry?.connected ? 'success' : 'warning'"
            :mark="entry?.connected ? 'active' : 'pending'"
          >{{ entry?.connected ? "在线" : "离线快照" }}</Status>
          <Status :tone="status.tone" :mark="status.mark">{{ status.text }}</Status>
          <Tag>{{ entry?.node.overlay_address || "未登记地址" }}</Tag>
          <Tag v-if="skew">时钟偏移 {{ record?.clock_offset }} 秒</Tag>
        </div>
      </div>
      <div class="nm-ranges">
        <Button
          v-for="r in RANGES"
          :key="r.value"
          :variant="hours === r.value ? 'default' : 'text'"
          @click="hours = r.value"
        >{{ r.label }}</Button>
      </div>
    </header>

    <Notice v-if="error" tone="danger">{{ error }}</Notice>
    <Notice v-else-if="skew" tone="warning">
      该节点时钟与主控相差 {{ record?.clock_offset }} 秒，曲线的时间轴会随之偏移。
    </Notice>
    <Notice v-else-if="status.state === 'error'" tone="danger">
      指标采集失败：{{ record?.error }}
    </Notice>
    <Notice v-else-if="status.state === 'pending' || status.state === 'unsupported'">
      {{ status.text }}。{{ status.state === "unsupported"
        ? "该 Agent 版本没有指标采样能力，需要升级后才会上报。"
        : "已连接但尚未收到第一份采样。" }}
    </Notice>
    <Notice v-else-if="status.state === 'stale'" tone="warning">
      最近一次采样距今已超过 {{ METRICS_STALE_SECONDS }} 秒，显示的是上次已知值。
    </Notice>

    <div class="nm-tiles">
      <div class="nm-tile">
        <span class="nm-tile-label">CPU 使用率</span>
        <strong class="nm-tile-value">{{ sample ? percent(sample.cpu_usage) : "—" }}</strong>
      </div>
      <div class="nm-tile">
        <span class="nm-tile-label">内存</span>
        <strong class="nm-tile-value">{{ memoryPercent == null ? "—" : percent(memoryPercent) }}</strong>
        <span class="nm-tile-sub" v-if="sample">{{ bytes(sample.memory_used) }} / {{ bytes(sample.memory_total) }}</span>
      </div>
      <div class="nm-tile">
        <span class="nm-tile-label">负载</span>
        <strong class="nm-tile-value">{{ sample ? sample.load1.toFixed(2) : "—" }}</strong>
      </div>
      <div class="nm-tile">
        <span class="nm-tile-label">运行时长</span>
        <strong class="nm-tile-value">{{ sample ? formatUptime(sample.uptime) : "—" }}</strong>
      </div>
      <div class="nm-tile">
        <span class="nm-tile-label">交换</span>
        <strong class="nm-tile-value">{{
          sample ? usagePercent(sample.swap_used, sample.swap_total) == null
            ? "未启用"
            : percent(usagePercent(sample.swap_used, sample.swap_total)!)
            : "—"
        }}</strong>
      </div>
      <div class="nm-tile">
        <span class="nm-tile-label">最近采样</span>
        <strong class="nm-tile-value">{{ age == null ? "—" : `${age} 秒前` }}</strong>
      </div>
    </div>

    <div class="nm-charts">
      <div class="nm-chart">
        <h3 class="nm-chart-title">CPU 使用率</h3>
        <MetricsChart :data="series('cpu_usage')" :threshold="90" label="CPU" unit="%" />
      </div>
      <div class="nm-chart">
        <h3 class="nm-chart-title">内存占用</h3>
        <MetricsChart :data="series('memory_used')" label="内存" />
      </div>
      <div class="nm-chart">
        <h3 class="nm-chart-title">入站速率</h3>
        <MetricsChart :data="series('net_rx_bytes_per_second')" label="入站" />
      </div>
      <div class="nm-chart">
        <h3 class="nm-chart-title">出站速率</h3>
        <MetricsChart :data="series('net_tx_bytes_per_second')" label="出站" />
      </div>
      <p v-if="loading" class="nm-muted">正在读取历史数据…</p>
      <p v-else-if="loaded && !points.length" class="nm-muted">该区间没有历史数据。</p>
      <p v-if="truncated" class="nm-muted">区间数据量超过上限，已按步长抽稀。</p>
    </div>

    <section v-if="sample" class="nm-tables">
      <div v-if="disks.length" class="nm-table-wrap">
        <h3 class="nm-table-title">挂载点</h3>
        <table class="nm-table">
          <thead>
            <tr>
              <th>挂载点</th>
              <th>设备</th>
              <th>已用 / 总量</th>
              <th>使用率</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="disk in disks" :key="disk.mount">
              <td class="nm-mono">{{ disk.mount }}</td>
              <td class="nm-mono">{{ disk.filesystem }}</td>
              <td class="nm-mono">{{ bytes(disk.used) }} / {{ bytes(disk.total) }}</td>
              <td>{{ percent(usagePercent(disk.used, disk.total) ?? 0) }}</td>
            </tr>
          </tbody>
        </table>
      </div>
      <div v-if="interfaces.length" class="nm-table-wrap">
        <h3 class="nm-table-title">网络接口</h3>
        <table class="nm-table">
          <thead>
            <tr>
              <th>接口</th>
              <th>入站速率</th>
              <th>出站速率</th>
              <th>错误</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="nic in interfaces" :key="nic.name">
              <td class="nm-mono">{{ nic.name }}</td>
              <td class="nm-mono">{{ perSecond(nic.rx_bytes_per_second) }}</td>
              <td class="nm-mono">{{ perSecond(nic.tx_bytes_per_second) }}</td>
              <td class="nm-mono">{{ nic.rx_errors + nic.tx_errors }}</td>
            </tr>
          </tbody>
        </table>
      </div>
      <div v-if="peers.length" class="nm-table-wrap">
        <h3 class="nm-table-title">线路质量</h3>
        <table class="nm-table">
          <thead>
            <tr>
              <th>对端</th>
              <th>地址</th>
              <th>延迟</th>
              <th>结果</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="peer in peers" :key="peer.node_id">
              <td>{{ peer.node_id }}</td>
              <td class="nm-mono">{{ peer.address }}</td>
              <td class="nm-mono">
                <template v-if="peer.latency_ms !== null && peer.latency_ms !== undefined">
                  {{ peer.latency_ms.toFixed(1) }} ms
                </template>
                <template v-else><span class="nm-muted">未知</span></template>
              </td>
              <td>
                <Status
                  :tone="peer.reachable ? 'success' : 'warning'"
                  :mark="peer.reachable ? 'active' : 'pending'"
                >{{ peer.reachable ? "可达" : "不可达" }}</Status>
              </td>
            </tr>
          </tbody>
        </table>
        <p v-if="peersProbedAt" class="nm-muted">最近一次探测：{{ probeAge }}</p>
      </div>
    </section>

    <section class="nm-workbench">
      <h3 class="nm-section-title">堡垒机</h3>
      <HostShell :node-id="nodeId" :demo="w.demo" />
      <FileManager :node-id="nodeId" />
    </section>
  </section>
</template>

<style scoped>
.nm {
  display: flex;
  flex-direction: column;
  gap: 24px;
}
.nm-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 16px;
}
.nm-title {
  font-size: 24px;
  font-weight: 700;
  line-height: 1.2;
  margin: 0 0 8px;
}
.nm-badges {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}
.nm-ranges {
  display: flex;
  gap: 4px;
}
.nm-tiles {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(140px, 1fr));
  gap: 16px;
  padding: 16px 0;
  border-bottom: 1px solid var(--line);
}
.nm-tile {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.nm-tile-label {
  font-size: 11px;
  color: var(--muted);
}
.nm-tile-value {
  font-size: 20px;
  font-weight: 700;
  font-variant-numeric: tabular-nums;
}
.nm-tile-sub {
  font-size: 11px;
  color: var(--muted);
}
.nm-charts {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: 24px;
}
.nm-chart {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.nm-chart-title {
  font-size: 11px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--muted);
  margin: 0;
}
.nm-tables {
  display: flex;
  flex-direction: column;
  gap: 24px;
}
.nm-table-wrap {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.nm-table-title {
  font-size: 11px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--muted);
  margin: 0;
}
.nm-table {
  width: 100%;
  border-collapse: collapse;
}
.nm-table th {
  padding: 8px 12px;
  text-align: left;
  font-size: 11px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--muted);
  border-bottom: 2px solid var(--ink);
}
.nm-table td {
  padding: 8px 12px;
  font-size: 12px;
  border-bottom: 1px solid var(--line);
}
.nm-mono {
  font-family: "JetBrains Mono", "Fira Code", monospace;
  font-size: 11px;
}
.nm-workbench {
  display: flex;
  flex-direction: column;
  gap: 16px;
  padding-top: 24px;
  border-top: 1px solid var(--line);
}
.nm-section-title {
  font-size: 11px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--muted);
  margin: 0;
}
.nm-muted {
  font-size: 12px;
  color: var(--muted);
  margin: 0;
}
@media (max-width: 1000px) {
  .nm-charts {
    grid-template-columns: 1fr;
  }
}
</style>
