<script setup lang="ts">
import { computed } from "vue";
import { finiteNumber, finiteSeries, segmentSeries } from "../resourcePresentation";
/**
 * 分享页的折线图。刻意与控制台的图表分开实现：
 * 分享页不加载工作台状态，也不依赖任何管理侧组件。
 * 分段规则与内部分享同一套：间隔超过两倍步长即断开，缺失值不补零。
 */
const props = withDefaults(
  defineProps<{
    points: { at: number; [key: string]: number }[];
    field: string;
    step: number;
    height?: number;
    max?: number;
    label?: string;
    format?: (value: number) => string;
  }>(),
  { max: 0, label: "", height: 110 },
);
const width = 640;
const padding = { top: 8, right: 6, bottom: 14, left: 40 };
const segments = computed(() =>
  segmentSeries(props.points, props.field, props.step),
);
const values = computed(() => finiteSeries(props.points, props.field));
const bounds = computed(() => {
  const from = props.points[0]?.at ?? 0;
  const to = props.points[props.points.length - 1]?.at ?? from + 1;
  return {
    from,
    to: Math.max(to, from + 1),
    high: props.max > 0 ? props.max : Math.max(1, ...values.value),
  };
});
function x(at: number) {
  const { from, to } = bounds.value;
  return padding.left + ((at - from) / (to - from)) * (width - padding.left - padding.right);
}
function y(value: number) {
  const ratio = Math.min(1, Math.max(0, value / bounds.value.high));
  return padding.top + (1 - ratio) * (props.height - padding.top - padding.bottom);
}
const paths = computed(() =>
  segments.value
    .filter((s) => s.length > 1)
    .map((s) =>
      s
        .map((p, i) => `${i ? "L" : "M"}${x(p.at).toFixed(1)},${y(p.value).toFixed(1)}`)
        .join(" "),
    ),
);
const isolated = computed(() =>
  segments.value.filter((s) => s.length === 1).map((s) => s[0]),
);
const stats = computed(() => {
  const list = values.value;
  if (!list.length) return null;
  const format = props.format || ((v: number) => v.toFixed(0));
  return {
    last: format(list[list.length - 1]),
    min: format(Math.min(...list)),
    max: format(Math.max(...list)),
    count: list.length,
  };
});
const grid = computed(() =>
  [0, 0.5, 1].map((ratio) => ({
    y: padding.top + ratio * (props.height - padding.top - padding.bottom),
    value: bounds.value.high * (1 - ratio),
  })),
);
</script>
<template>
  <figure class="metrics-chart">
    <figcaption>
      <span>{{ label }}</span>
      <span v-if="stats" class="muted"
        >当前 {{ stats.last }} · {{ stats.count }} 点</span
      >
      <span v-else class="muted">区间内没有数据</span>
    </figcaption>
    <svg :viewBox="`0 0 ${width} ${height}`" preserveAspectRatio="none" role="img" :aria-label="`${label} 曲线`">
      <g class="grid">
        <template v-for="line in grid" :key="line.y">
          <line :x1="padding.left" :x2="width - padding.right" :y1="line.y" :y2="line.y" />
          <text :x="0" :y="line.y + 3">
            {{ (format || ((v: number) => v.toFixed(0)))(line.value) }}
          </text>
        </template>
      </g>
      <path
        v-for="(path, index) in paths"
        :key="index"
        :d="path"
        class="line"
        style="stroke: var(--accent)"
      />
      <circle
        v-for="point in isolated"
        :key="`d-${point.at}`"
        :cx="x(point.at)"
        :cy="y(point.value)"
        r="2"
        style="fill: var(--accent)"
      />
    </svg>
  </figure>
</template>
