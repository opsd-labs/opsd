<script setup lang="ts">
import {
  DropdownMenuRoot,
  DropdownMenuTrigger,
  DropdownMenuPortal,
  DropdownMenuContent,
  DropdownMenuItem,
} from "reka-ui";
import { Ellipsis } from "lucide-vue-next";
import Button from "./Button.vue";
defineProps<{
  items: { id: string; label: string; disabled?: boolean; danger?: boolean }[];
  label?: string;
}>();
defineEmits<{ select: [id: string] }>();
</script>
<template>
  <DropdownMenuRoot
    ><DropdownMenuTrigger as-child
      ><Button icon variant="text" :aria-label="label || '更多操作'"
        ><Ellipsis /></Button></DropdownMenuTrigger
    ><DropdownMenuPortal
      ><DropdownMenuContent class="ui-menu" :side-offset="4" align="end"
        ><DropdownMenuItem
          v-for="item in items"
          :key="item.id"
          class="ui-menu-item"
          :class="{ 'tone-danger': item.danger }"
          :disabled="item.disabled"
          @select="$emit('select', item.id)"
          >{{ item.label }}</DropdownMenuItem
        ></DropdownMenuContent
      ></DropdownMenuPortal
    ></DropdownMenuRoot
  >
</template>
