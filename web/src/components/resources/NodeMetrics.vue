<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useWorkspace } from "../../workspace";
import { api } from "../../api";
import Notice from "../ui/Notice.vue";
import Status from "../ui/Status.vue";
import Tag from "../ui/Tag.vue";
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

/** 可选区间与它们对应的分钟数。 */
const RANGES = [
  { value: 1, label: "近 1 小时" },
  { value: 6, label: "近 6 小时" },
  { value: 24, label: "近 24 小时" },
  { value: 24 * 7, label: "近 7 天" },
  { value: 24 * 30, label: "近 30 天" },
];
const props = defineProps<{ nodeId: string }>();
const w = useWorkspace();
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

const entry = computed(
  () => w.entries.find((e) => e.node.id === props.nodeId) || null,
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
      // 演示模式不发起请求，用一条可辨认的静态曲线占位
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
/** 对端延迟。空列表表示这一轮还没测到，界面据此不显示这一节。 */
const peers = computed(() => sample.value?.peers || []);
const peersProbedAt = computed(() => {
  const times = peers.value
    .map((peer: any) => peer.probed_at)
    .filter((at: number) => at > 0);
  return times.length ? Math.max(...times) : 0;
});
/** 探测比采样稀疏，因此要说清"这个数字是什么时候测的"。 */
const probeAge = computed(() => {
  const age = Math.max(0, nowSeconds.value - peersProbedAt.value);
  return age < 60 ? "刚刚" : `${formatUptime(age)}前`;
});
const bytes = (value: number) => formatBytes(value);
const percent = (value: number) => formatPercent(value);
const perSecond = (value: number) => `${formatBytes(value)}/s`;
</script>
<template>
  <section class="node-detail">
    <header class="node-detail-header">
      <div>
        <h2>{{ entry?.node.name || nodeId }}</h2>
        <div class="inline">
          <Status
            :tone="
              entry?.connected
                ? 'success'
                : 'warning'
            "
            :mark="entry?.connected ? 'active' : 'pending'"
            >{{ entry?.connected ? "在线" : "离线快照" }}</Status
          >
          <Status :tone="status.tone" :mark="status.mark">{{
            status.text
          }}</Status>
          <Tag>{{ entry?.node.overlay_address || "未登记地址" }}</Tag>
          <Tag v-if="skew">时钟偏移 {{ record?.clock_offset }} 秒</Tag>
        </div>
      </div>
      <div class="range-picker">
        <button
          v-for="r in RANGES"
          :key="r.value"
          type="button"
          class="ui-button"
          :class="{ 'is-primary': hours === r.value }"
          @click="hours = r.value"
        >
          {{ r.label }}
        </button>
      </div>
    </header>

    <Notice v-if="error" tone="danger">{{ error }}</Notice>
    <Notice v-else-if="skew" tone="warning"
      >该节点时钟与主控相差 {{ record?.clock_offset }} 秒，曲线的时间轴会随之偏移。该估计包含单向网络时延，只用于识别明显偏差。</Notice
    >
    <Notice v-else-if="status.state === 'error'" tone="danger"
      >指标采集失败：{{ record?.error }}</Notice
    >
    <Notice
      v-else-if="status.state === 'pending' || status.state === 'unsupported'"
      >{{ status.text }}。{{ status.state === "unsupported"
        ? "该 Agent 版本没有指标采样能力，需要升级后才会上报。"
        : "已连接但尚未收到第一份采样。" }}
      此处不显示 0，以免把缺失误读为真实读数。</Notice
    >
    <Notice v-else-if="status.state === 'stale'" tone="warning"
      >最近一次采样距今已超过 {{ METRICS_STALE_SECONDS }} 秒，显示的是上次已知值。</Notice
    >

    <div class="metric-summary">
      <div class="metric-tile">
        <span class="muted">CPU 使用率</span>
        <strong>{{ sample ? percent(sample.cpu_usage) : "—" }}</strong>
        <span class="muted" v-if="sample"
          >每核 {{ sample.cpu_per_core.length }} 个</span
        >
      </div>
      <div class="metric-tile">
        <span class="muted">内存</span>
        <strong>{{ memoryPercent == null ? "—" : percent(memoryPercent) }}</strong>
        <span class="muted" v-if="sample"
          >{{ bytes(sample.memory_used) }} / {{ bytes(sample.memory_total) }}</span
        >
      </div>
      <div class="metric-tile">
        <span class="muted">负载</span>
        <strong>{{ sample ? sample.load1.toFixed(2) : "—" }}</strong>
        <span class="muted" v-if="sample"
          >5 分 {{ sample.load5.toFixed(2) }} · 15 分
          {{ sample.load15.toFixed(2) }}</span
        >
      </div>
      <div class="metric-tile">
        <span class="muted">运行时长</span>
        <strong>{{ sample ? formatUptime(sample.uptime) : "—" }}</strong>
        <span class="muted" v-if="sample"
          >进程 {{ sample.processes }} · TCP
          {{ sample.tcp_connections }}</span
        >
      </div>
      <div class="metric-tile">
        <span class="muted">交换</span>
        <strong>{{
          sample ? usagePercent(sample.swap_used, sample.swap_total) == null
            ? "未启用"
            : percent(usagePercent(sample.swap_used, sample.swap_total)!)
            : "—"
        }}</strong>
        <span class="muted" v-if="sample && sample.swap_total"
          >{{ bytes(sample.swap_used) }} / {{ bytes(sample.swap_total) }}</span
        >
      </div>
      <div class="metric-tile">
        <span class="muted">最近采样</span>
        <strong>{{ age == null ? "—" : `${age} 秒前` }}</strong>
        <span class="muted" v-if="tier">层级 {{ tier }} · 步长 {{ step }} 秒</span>
      </div>
    </div>

    <div class="charts">
      <MetricsChart
        :points="points"
        field="cpu_usage"
        :step="step || 60"
        :max="100"
        label="CPU 使用率"
        :format="percent"
      />
      <MetricsChart
        :points="points"
        field="memory_used"
        :step="step || 60"
        label="内存占用"
        :format="bytes"
      />
      <MetricsChart
        :points="points"
        field="net_rx_bytes_per_second"
        :step="step || 60"
        label="入站速率"
        :format="perSecond"
      />
      <MetricsChart
        :points="points"
        field="net_tx_bytes_per_second"
        :step="step || 60"
        label="出站速率"
        :format="perSecond"
      />
      <MetricsChart
        :points="points"
        field="disk_read_bytes_per_second"
        :step="step || 60"
        label="磁盘读取"
        :format="perSecond"
      />
      <MetricsChart
        :points="points"
        field="disk_write_bytes_per_second"
        :step="step || 60"
        label="磁盘写入"
        :format="perSecond"
      />
      <p v-if="loading" class="muted">正在读取历史数据…</p>
      <p v-else-if="loaded && !points.length" class="muted">
        该区间没有历史数据。未上报的时段不会补点，因此曲线会直接断开。
      </p>
      <p v-if="truncated" class="muted">
        区间数据量超过上限，已按步长抽稀；可缩小时间范围查看细节。
      </p>
    </div>

    <section v-if="sample" class="metric-tables">
      <div v-if="disks.length">        <h3>挂载点</h3>
        <table>
          <thead>
            <tr>
              <th>挂载点</th>
              <th>设备</th>
              <th>已用 / 总量</th>
              <th>使用率</th>
              <th>inode</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="disk in disks" :key="disk.mount">
              <td class="mono">{{ disk.mount }}</td>
              <td class="mono">{{ disk.filesystem }}</td>
              <td class="mono">
                {{ bytes(disk.used) }} / {{ bytes(disk.total) }}
              </td>
              <td>{{ percent(usagePercent(disk.used, disk.total) ?? 0) }}</td>
              <td class="mono">
                {{
                  disk.inode_total
                    ? percent(usagePercent(disk.inode_used, disk.inode_total) ?? 0)
                    : "—"
                }}
              </td>
            </tr>
          </tbody>
        </table>
      </div>
      <div v-if="interfaces.length">
        <h3>网络接口</h3>
        <table>
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
              <td class="mono">{{ nic.name }}</td>
              <td class="mono">{{ perSecond(nic.rx_bytes_per_second) }}</td>
              <td class="mono">{{ perSecond(nic.tx_bytes_per_second) }}</td>
              <td class="mono">
                {{ nic.rx_errors + nic.tx_errors }}
              </td>
            </tr>
          </tbody>
        </table>
      </div>
      <div v-if="peers.length">
        <h3>线路质量</h3>
        <p class="muted">
          本节点到各个对端的延迟，默认每 60 秒探测一次（ICMP，不产生服务端日志）。
          <strong>没测到就是未知</strong>：0 毫秒是"同机"，与"探测失败"不是一回事，
          因此这里不会出现 0。
        </p>
        <table>
          <thead>
            <tr>
              <th>对端</th>
              <th>地址</th>
              <th>方式</th>
              <th>延迟</th>
              <th>结果</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="peer in peers" :key="peer.node_id">
              <td>{{ peer.node_id }}</td>
              <td class="mono">{{ peer.address }}</td>
              <td>{{ peer.method === "tcp" ? "TCP" : "ICMP" }}</td>
              <td class="mono">
                <template v-if="peer.latency_ms !== null && peer.latency_ms !== undefined">
                  {{ peer.latency_ms.toFixed(1) }} ms
                </template>
                <template v-else><span class="muted">未知</span></template>
              </td>
              <td>
                <Status
                  :tone="peer.reachable ? 'success' : 'warning'"
                  :mark="peer.reachable ? 'active' : 'pending'"
                  >{{ peer.reachable ? "可达" : "不可达" }}</Status
                >
                <span v-if="peer.error" class="muted">{{ peer.error }}</span>
              </td>
            </tr>
          </tbody>
        </table>
        <p v-if="peersProbedAt" class="muted">
          最近一次探测：{{ probeAge }}
        </p>
      </div>
    </section>

    <section class="node-workbench">
      <h3>堡垒机</h3>
      <p class="muted">
        终端与文件管理都通过既有的 Agent 流通道，不开放任意宿主机命令接口。
        会话与文件变更均记入审计。
      </p>
      <HostShell :node-id="nodeId" :demo="w.demo" />
      <FileManager :node-id="nodeId" />
    </section>
  </section>
</template>
