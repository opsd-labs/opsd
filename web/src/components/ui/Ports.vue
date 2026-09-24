<script setup lang="ts">
import { computed } from "vue";
import {
  parseDockerPorts,
  parseUnpublishedPorts,
} from "../../resourcePresentation";
const props = defineProps<{ ports?: string | null }>();
/** 通配绑定不必占用列表宽度，只保留在 title 里；具体绑定地址需要显示。 */
function concrete(address?: string) {
  if (!address) return false;
  const value = address.toLowerCase();
  return !["0.0.0.0", "*", "::", "[::]"].includes(value);
}
const raw = computed(() => String(props.ports ?? "").trim());
const mappings = computed(() => parseDockerPorts(raw.value));
const unpublished = computed(() =>
  mappings.value === null ? parseUnpublishedPorts(raw.value) : null,
);
</script>
<template>
  <span v-if="!raw" class="muted">未发布</span>
  <span v-else-if="mappings && mappings.length" class="port-list" :title="raw">
    <span v-for="(m, i) in mappings" :key="i" class="port-map">
      <span v-if="concrete(m.address)" class="port-number muted">{{ m.address }}</span>
      <span class="port-number host-port">{{ m.host }}</span>
      <span class="port-arrow" aria-hidden="true">→</span>
      <span class="port-number container-port">{{ m.container }}</span>
      <span class="port-divider" aria-hidden="true">·</span>
      <span class="port-protocol">{{ m.protocol.toUpperCase() }}</span>
    </span>
  </span>
  <span
    v-else-if="unpublished && unpublished.length"
    class="port-list"
    :title="raw"
  >
    <span v-for="(p, i) in unpublished" :key="i" class="port-map">
      <span class="port-number container-port">{{ p.port }}</span>
      <span class="port-divider" aria-hidden="true">·</span>
      <span class="port-protocol">{{ p.protocol.toUpperCase() }}</span>
      <span class="port-protocol">未发布</span>
    </span>
  </span>
  <!-- 无法解析时保留原文：不伪造结构化映射，原文仍可被搜索与诊断 -->
  <span v-else class="port-raw" :title="raw">{{ raw }}</span>
</template>
