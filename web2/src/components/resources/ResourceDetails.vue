<script setup lang="ts">
import { computed } from "vue";

const props = withDefaults(
  defineProps<{ value: any; labels?: Record<string, string>; omit?: string[] }>(),
  { labels: () => ({}), omit: () => [] },
);

const fields = computed(() =>
  props.value && typeof props.value === "object"
    ? Object.entries(props.value).filter(([key]) => !props.omit.includes(key))
    : [["内容", props.value]],
);

function display(value: any): string {
  if (value == null) return "—";
  if (typeof value === "boolean") return value ? "是" : "否";
  if (Array.isArray(value)) return value.map(display).join("；") || "无";
  if (typeof value === "object")
    return Object.entries(value)
      .map(([key, v]) => `${props.labels[key] || key}：${display(v)}`)
      .join(" · ");
  return String(value);
}
</script>

<template>
  <dl class="rd">
    <div v-for="[key, value] in fields" :key="String(key)" class="rd-row">
      <dt class="rd-key">{{ props.labels[String(key)] || key }}</dt>
      <dd class="rd-value">{{ display(value) }}</dd>
    </div>
  </dl>
  <details class="rd-raw">
    <summary>原始诊断数据</summary>
    <pre>{{ JSON.stringify(value, null, 2) }}</pre>
  </details>
</template>

<style scoped>
.rd {
  margin: 0;
  padding: 0;
}
.rd-row {
  display: grid;
  grid-template-columns: 180px 1fr;
  gap: 16px;
  padding: 8px 0;
  border-bottom: 1px solid var(--line);
}
.rd-row:last-child {
  border-bottom: none;
}
.rd-key {
  font-size: 11px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--muted);
  margin: 0;
}
.rd-value {
  font-size: 13px;
  color: var(--ink);
  margin: 0;
  word-break: break-word;
}
.rd-raw {
  margin-top: 24px;
  padding-top: 16px;
  border-top: 1px solid var(--line);
}
.rd-raw summary {
  font-size: 11px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--muted);
  cursor: pointer;
  user-select: none;
}
.rd-raw summary:hover {
  color: var(--ink);
}
.rd-raw pre {
  margin: 12px 0 0;
  padding: 12px;
  background: var(--bg);
  border: 1px solid var(--line);
  border-radius: var(--r);
  font-family: "JetBrains Mono", "Fira Code", monospace;
  font-size: 11px;
  line-height: 1.5;
  overflow-x: auto;
  color: var(--ink);
}
</style>
