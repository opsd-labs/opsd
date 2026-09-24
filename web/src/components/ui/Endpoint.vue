<script setup lang="ts">
import { computed } from "vue";
import { firewallPorts, protocolLabel } from "../../resourcePresentation";
const props = defineProps<{
  protocol?: string | null;
  ports?: (number | string)[] | null;
}>();
const label = computed(() => protocolLabel(props.protocol));
const value = computed(() => firewallPorts(props.ports));
</script>
<template>
  <!-- 防火墙端口：端口为主要数值，协议作为辅助标识，列表与详情共用同一格式 -->
  <span class="endpoint">
    <span class="port-number">{{ value }}</span>
    <span class="port-protocol">{{ label }}</span>
  </span>
</template>
