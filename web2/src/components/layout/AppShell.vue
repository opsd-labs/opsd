<script setup lang="ts">
import Navigation from "./Navigation.vue";
import TopBar from "./TopBar.vue";
</script>

<template>
  <div class="shell">
    <Navigation />
    <div class="shell-main">
      <TopBar />
      <main class="shell-content">
        <slot />
      </main>
    </div>
  </div>
</template>

<style scoped>
.shell {
  display: flex;
  min-height: 100vh;
  background: var(--bg);
}
.shell-main {
  flex: 1;
  margin-left: 200px;
  display: flex;
  flex-direction: column;
  min-height: 100vh;
}
.shell-content {
  flex: 1;
  padding: 32px;
}
@media (max-width: 1000px) {
  .shell-main {
    margin-left: 0;
  }
}
</style>
