<script setup lang="ts">
import { computed } from "vue";
import { finiteSeries, segmentSeries } from "../resourcePresentation";

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
  <figure class="share-chart">
    <figcaption>
      <span class="share-chart-label">{{ label }}</span>
      <span v-if="stats" class="share-chart-stats"
        >当前 {{ stats.last }} · {{ stats.count }} 点</span
      >
      <span v-else class="share-chart-stats">区间内没有数据</span>
    </figcaption>
    <svg
      :viewBox="`0 0 ${width} ${height}`"
      preserveAspectRatio="none"
      role="img"
      :aria-label="`${label} 曲线`"
    >
      <g class="share-chart-grid">
        <template v-for="line in grid" :key="line.y">
          <line
            :x1="padding.left"
            :x2="width - padding.right"
            :y1="line.y"
            :y2="line.y"
          />
          <text :x="0" :y="line.y + 3">
            {{ (format || ((v: number) => v.toFixed(0)))(line.value) }}
          </text>
        </template>
      </g>
      <path
        v-for="(path, index) in paths"
        :key="index"
        :d="path"
        class="share-chart-line"
      />
      <circle
        v-for="point in isolated"
        :key="`d-${point.at}`"
        :cx="x(point.at)"
        :cy="y(point.value)"
        r="2"
        class="share-chart-dot"
      />
    </svg>
  </figure>
</template>

<style scoped>
.share-chart {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.share-chart figcaption {
  display: flex;
  justify-content: space-between;
  align-items: baseline;
  font-size: 11px;
}
.share-chart-label {
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.05em;
}
.share-chart-stats {
  color: var(--ink-3);
  font-size: 11px;
  font-variant-numeric: tabular-nums;
}
.share-chart svg {
  width: 100%;
  height: auto;
  display: block;
}
.share-chart-grid line {
  stroke: var(--line);
  stroke-width: 0.5;
}
.share-chart-grid text {
  font-family: var(--mono);
  font-size: 9px;
  fill: var(--ink-3);
}
.share-chart-line {
  fill: none;
  stroke: var(--ink);
  stroke-width: 1.5;
}
.share-chart-dot {
  fill: var(--accent);
}
</style>
