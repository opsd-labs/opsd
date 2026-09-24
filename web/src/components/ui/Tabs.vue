<script setup lang="ts">
import { TabsRoot, TabsList, TabsTrigger } from "reka-ui";
defineProps<{
  modelValue: string;
  items: { value: string; label: string }[];
}>();
defineEmits<{ "update:modelValue": [value: string] }>();
</script>
<template>
  <TabsRoot
    :model-value="modelValue"
    @update:model-value="$emit('update:modelValue', String($event))"
    ><TabsList class="ui-tabs" aria-label="资源分类"
      ><TabsTrigger
        v-for="item in items"
        :key="item.value"
        :value="item.value"
        class="ui-tab"
        >{{ item.label }}</TabsTrigger
      ></TabsList
    ></TabsRoot
  >
</template>
