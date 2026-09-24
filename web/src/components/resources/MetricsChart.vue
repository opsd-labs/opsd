<script setup lang="ts">
import { computed, ref } from "vue";
import { finiteNumber, finiteSeries, segmentSeries } from "../../resourcePresentation";
/**
 * 指标折线图。用内联 SVG 手绘，避免引入图表库。
 *
 * 缺失区间**断开**而不是补零：采样点之间按实际时间间隔绘制，
 * 间隔超过两倍步长的相邻点之间不连线，避免把离线时段画成一条平线。
 */
const props = withDefaults(
  defineProps<{
    points: { at: number; [key: string]: number }[];
    field: string;
    step: number;
    height?: number;
    color?: string;
    /** 纵轴上限，用于百分比类指标固定到 100。 */
    max?: number;
    label?: string;
    format?: (value: number) => string;
  }>(),
  { height: 120, max: 0, label: "", color: "var(--accent)" },
);
const hovered = ref<number | null>(null);
const width = 720;
const padding = { top: 8, right: 4, bottom: 16, left: 34 };

const values = computed(() => finiteSeries(props.points, props.field));
const bounds = computed(() => {
  const from = props.points[0]?.at ?? 0;
  const to = props.points[props.points.length - 1]?.at ?? from + 1;
  const high = props.max > 0 ? props.max : Math.max(1, ...values.value);
  return { from, to: Math.max(to, from + 1), high };
});
/** 相邻点间隔超过两倍步长即视为断档，中间不连线；抽稀逻辑见 resourcePresentation。 */
const segments = computed(() =>
  segmentSeries(props.points, props.field, props.step),
);
function x(at: number) {
  const { from, to } = bounds.value;
  const ratio = (at - from) / (to - from);
  return padding.left + ratio * (width - padding.left - padding.right);
}
function y(value: number) {
  const ratio = Math.min(1, Math.max(0, value / bounds.value.high));
  return (
    padding.top + (1 - ratio) * (props.height - padding.top - padding.bottom)
  );
}
function linePath(segment: { at: number; value: number }[]) {
  return segment
    .map(
      (p, i) =>
        `${i ? "L" : "M"}${x(p.at).toFixed(1)},${y(p.value).toFixed(1)}`,
    )
    .join(" ");
}
const paths = computed(() =>
  segments.value
    .filter((segment) => segment.length > 1)
    .map((segment) => linePath(segment)),
);
/** 只有一个点的区段用圆点表示，否则会被误认为没有数据。 */
const isolated = computed(() =>
  segments.value.filter((segment) => segment.length === 1).map((s) => s[0]),
);
/** 填充区域由折线闭合到基线构成，逐段生成以免跨断档连接。 */
const areas = computed(() =>
  segments.value
    .filter((segment) => segment.length > 1)
    .map((segment) => {
      const baseline = y(0).toFixed(1);
      const first = x(segment[0].at).toFixed(1);
      const last = x(segment[segment.length - 1].at).toFixed(1);
      return `${linePath(segment)} L${last},${baseline} L${first},${baseline} Z`;
    }),
);
const format = computed(
  () => props.format || ((value: number) => value.toFixed(0)),
);
const gridLines = computed(() =>
  [0, 0.5, 1].map((ratio) => ({
    y: padding.top + ratio * (props.height - padding.top - padding.bottom),
    value: bounds.value.high * (1 - ratio),
  })),
);
const sampled = computed(() =>
  finiteSeries(props.points, props.field),
);
const stats = computed(() => {
  const list = sampled.value;
  if (!list.length) return null;
  return {
    min: Math.min(...list),
    max: Math.max(...list),
    last: list[list.length - 1],
    count: list.length,
  };
});
function onMove(event: MouseEvent) {
  const target = event.currentTarget as SVGGElement;
  const rect = target.getBoundingClientRect();
  const scale = width / rect.width;
  const pointer = (event.clientX - rect.left) * scale;
  let nearest: number | null = null;
  let best = Infinity;
  props.points.forEach((p, index) => {
    if (finiteNumber(p[props.field]) === null) return;
    const distance = Math.abs(x(p.at) - pointer);
    if (distance < best) {
      best = distance;
      nearest = index;
    }
  });
  hovered.value = nearest;
}
const hoveredPoint = computed(() =>
  hovered.value == null ? null : props.points[hovered.value],
);
</script>
<template>
  <figure class="metrics-chart">
    <figcaption>
      <span>{{ label }}</span>
      <span v-if="stats" class="muted"
        >当前 {{ format(stats.last) }} · 最低 {{ format(stats.min) }} · 最高
        {{ format(stats.max) }} · {{ stats.count }} 个采样点</span
      >
      <span v-else class="muted">区间内没有采样数据</span>
    </figcaption>
    <svg
      :viewBox="`0 0 ${width} ${height}`"
      preserveAspectRatio="none"
      role="img"
      :aria-label="`${label} 曲线`"
      @mousemove="onMove"
      @mouseleave="hovered = null"
    >
      <g class="grid">
        <template v-for="line in gridLines" :key="line.y">
          <line
            :x1="padding.left"
            :x2="width - padding.right"
            :y1="line.y"
            :y2="line.y"
          />
          <text :x="0" :y="line.y + 3">{{ format(line.value) }}</text>
        </template>
      </g>
      <path
        v-for="(path, index) in areas"
        :key="`area-${index}`"
        :d="path"
        class="area"
        :style="{ fill: color }"
      />
      <path
        v-for="(path, index) in paths"
        :key="`line-${index}`"
        :d="path"
        class="line"
        :style="{ stroke: color }"
      />
      <circle
        v-for="point in isolated"
        :key="`dot-${point.at}`"
        :cx="x(point.at)"
        :cy="y(point.value)"
        r="2"
        :style="{ fill: color }"
      />
      <line
        v-if="hoveredPoint"
        class="cursor"
        :x1="x(hoveredPoint.at)"
        :x2="x(hoveredPoint.at)"
        :y1="padding.top"
        :y2="height - padding.bottom"
      />
    </svg>
    <p v-if="hoveredPoint" class="readout mono">
      {{ new Date(hoveredPoint.at * 1000).toLocaleString("zh-CN", { hour12: false }) }}
      · {{ format(Number(hoveredPoint[field])) }}
    </p>
  </figure>
</template>
