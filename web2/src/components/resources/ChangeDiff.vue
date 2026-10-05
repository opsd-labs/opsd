<script setup lang="ts">
import ResourceDetails from "./ResourceDetails.vue";

defineProps<{ value: any; labels?: Record<string, string> }>();
</script>

<template>
  <div v-if="value?.before && value?.after" class="cd">
    <section class="cd-section">
      <h3 class="cd-title">变更前</h3>
      <ResourceDetails :labels="labels" :value="value.before" />
    </section>
    <section class="cd-section">
      <h3 class="cd-title">变更后</h3>
      <ResourceDetails :labels="labels" :value="value.after" />
    </section>
  </div>
  <ResourceDetails :labels="labels" v-else :value="value" />
</template>

<style scoped>
.cd {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 32px;
}
.cd-section {
  min-width: 0;
}
.cd-title {
  font-size: 11px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--muted);
  margin: 0 0 12px;
  padding-bottom: 8px;
  border-bottom: 1px solid var(--line);
}
@media (max-width: 700px) {
  .cd {
    grid-template-columns: 1fr;
    gap: 24px;
  }
}
</style>
