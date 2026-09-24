<script setup lang="ts">
import ResourceDetails from "./ResourceDetails.vue";
defineProps<{ value: any; labels?: Record<string, string> }>();
</script>
<template>
  <div v-if="value?.before && value?.after" class="change-diff">
    <section>
      <h3>变更前</h3>
      <ResourceDetails :labels="labels" :value="value.before" />
    </section>
    <section>
      <h3>变更后</h3>
      <ResourceDetails :labels="labels" :value="value.after" />
    </section>
  </div>
  <ResourceDetails :labels="labels" v-else :value="value" />
</template>
