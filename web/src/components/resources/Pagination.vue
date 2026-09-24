<script setup lang="ts">
import Button from "../ui/Button.vue";
import { ChevronLeft, ChevronRight } from "lucide-vue-next";
defineProps<{ page: number; pages: number; total: number; size: number }>();
defineEmits<{ "update:page": [value: number] }>();
</script>
<template>
  <div class="table-pagination">
    <span>共 {{ total }} 条 · {{ size }} 条/页</span>
    <div class="inline">
      <Button
        icon
        aria-label="上一页"
        :disabled="page <= 1"
        @click="$emit('update:page', page - 1)"
        ><ChevronLeft /></Button
      ><span>{{ page }} / {{ pages }}</span
      ><Button
        icon
        aria-label="下一页"
        :disabled="page >= pages"
        @click="$emit('update:page', page + 1)"
        ><ChevronRight
      /></Button>
    </div>
  </div>
</template>
