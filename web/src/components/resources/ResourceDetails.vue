<script setup lang="ts">
import { computed } from "vue";
const props = withDefaults(
  defineProps<{ value: any; labels?: Record<string, string>; omit?: string[] }>(),
  { labels: () => ({}), omit: () => [] },
);

const fields = computed(() =>
  props.value && typeof props.value === "object"
    ? Object.entries(props.value).filter(
        ([key]) => !props.omit.includes(key),
      )
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
  <dl class="resource-details">
    <template v-for="[key, value] in fields" :key="String(key)"
      ><dt>{{ props.labels[String(key)] || key }}</dt>
      <dd>{{ display(value) }}</dd></template
    >
  </dl>
  <details class="diagnostic">
    <summary>原始诊断数据</summary>
    <pre>{{ JSON.stringify(value, null, 2) }}</pre>
  </details>
</template>
