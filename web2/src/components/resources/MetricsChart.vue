<script setup lang="ts">
import { computed } from "vue";

const props = defineProps<{
  data: number[];
  threshold?: number;
  label?: string;
  unit?: string;
  height?: number;
}>();

const h = computed(() => props.height || 120);
const padding = 8;

const bounds = computed(() => {
  if (!props.data.length) return { min: 0, max: 100 };
  const values = props.data.filter((v) => isFinite(v));
  if (!values.length) return { min: 0, max: 100 };
  let min = Math.min(...values);
  let max = Math.max(...values);
  if (props.threshold !== undefined) {
    max = Math.max(max, props.threshold);
  }
  if (min === max) {
    min = min - 1;
    max = max + 1;
  }
  return { min, max };
});

const points = computed(() => {
  const { min, max } = bounds.value;
  const range = max - min;
  const step = 100 / Math.max(1, props.data.length - 1);
  return props.data
    .map((v, i) => {
      if (!isFinite(v)) return null;
      const x = padding + ((100 - 2 * padding) * i * step) / 100;
      const y = padding + ((100 - 2 * padding) * (max - v)) / range;
      return `${x},${y}`;
    })
    .filter(Boolean)
    .join(" ");
});

const thresholdY = computed(() => {
  if (props.threshold === undefined) return -1;
  const { min, max } = bounds.value;
  const range = max - min;
  return padding + ((100 - 2 * padding) * (max - props.threshold)) / range;
});

const gridLines = computed(() => {
  const lines = [];
  const count = 4;
  for (let i = 0; i <= count; i++) {
    const y = padding + ((100 - 2 * padding) * i) / count;
    lines.push(y);
  }
  return lines;
});

function formatValue(v: number): string {
  if (props.unit === "%") return `${v.toFixed(0)}%`;
  if (v >= 1e9) return `${(v / 1e9).toFixed(1)}G`;
  if (v >= 1e6) return `${(v / 1e6).toFixed(1)}M`;
  if (v >= 1e3) return `${(v / 1e3).toFixed(1)}K`;
  return v.toFixed(0);
}
</script>

<template>
  <div class="mc" role="img" :aria-label="label || '指标图表'">
    <svg class="mc-svg" viewBox="0 0 100 100" preserveAspectRatio="none">
      <line
        v-for="y in gridLines"
        :key="y"
        :x1="padding"
        :y1="y"
        :x2="100 - padding"
        :y2="y"
        stroke="var(--line)"
        stroke-width="0.3"
      />
      <line
        v-if="threshold !== undefined && thresholdY >= 0"
        :x1="padding"
        :y1="thresholdY"
        :x2="100 - padding"
        :y2="thresholdY"
        stroke="var(--accent)"
        stroke-width="0.5"
        stroke-dasharray="2,2"
      />
      <polyline
        v-if="points"
        :points="points"
        fill="none"
        stroke="var(--ink)"
        stroke-width="1.5"
        stroke-linejoin="round"
        stroke-linecap="round"
      />
    </svg>
    <div class="mc-axis">
      <span class="mc-max">{{ formatValue(bounds.max) }}</span>
      <span class="mc-min">{{ formatValue(bounds.min) }}</span>
    </div>
  </div>
</template>

<style scoped>
.mc {
  position: relative;
  width: 100%;
}
.mc-svg {
  width: 100%;
  height: v-bind("h + 'px'");
  display: block;
}
.mc-axis {
  position: absolute;
  right: 0;
  top: 0;
  bottom: 0;
  display: flex;
  flex-direction: column;
  justify-content: space-between;
  padding: 4px 0;
  pointer-events: none;
}
.mc-max,
.mc-min {
  font-size: 10px;
  font-variant-numeric: tabular-nums;
  color: var(--muted);
  line-height: 1;
}
</style>
