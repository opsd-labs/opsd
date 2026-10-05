<script setup lang="ts">
import { inject } from "vue";
import { workspaceKey, navigation } from "../../workspace";
import Divider from "../ui/Divider.vue";

const w = inject(workspaceKey)!;
</script>

<template>
  <nav class="nav" aria-label="主导航">
    <div class="nav-brand">opsd</div>
    <Divider />
    <ul class="nav-list">
      <li v-for="(item, i) in navigation" :key="item.id" class="nav-item">
        <button
          class="nav-link"
          :class="{ 'nav-link--active': w.page === item.id }"
          @click="w.page = item.id"
        >
          <span class="nav-num">{{ String(i + 1).padStart(2, "0") }}</span>
          <span class="nav-label">{{ item.label }}</span>
        </button>
      </li>
    </ul>
    <div class="nav-footer">
      <Divider />
      <button class="nav-logout" @click="w.logout()">退出</button>
    </div>
  </nav>
</template>

<style scoped>
.nav {
  width: 200px;
  height: 100vh;
  background: var(--surface);
  border-right: 1px solid var(--line);
  display: flex;
  flex-direction: column;
  position: fixed;
  left: 0;
  top: 0;
  z-index: 100;
}
.nav-brand {
  padding: 16px 20px;
  font-size: 19px;
  font-weight: 700;
  letter-spacing: -0.01em;
  color: var(--ink);
}
.nav-list {
  list-style: none;
  margin: 0;
  padding: 8px 0;
  flex: 1;
}
.nav-item {
  margin: 0;
}
.nav-link {
  display: flex;
  align-items: baseline;
  gap: 12px;
  width: 100%;
  padding: 10px 20px;
  background: none;
  border: none;
  border-left: 3px solid transparent;
  font: inherit;
  font-size: 13px;
  color: var(--ink);
  text-align: left;
  cursor: pointer;
  transition: background 0.1s ease;
}
.nav-link:hover {
  background: var(--bg);
}
.nav-link--active {
  border-left-color: var(--accent);
  font-weight: 700;
}
.nav-num {
  font-size: 11px;
  color: var(--muted);
  font-variant-numeric: tabular-nums;
  min-width: 16px;
}
.nav-link--active .nav-num {
  color: var(--accent);
}
.nav-label {
  font-size: 13px;
}
.nav-footer {
  padding: 0;
}
.nav-logout {
  display: block;
  width: 100%;
  padding: 12px 20px;
  background: none;
  border: none;
  font: inherit;
  font-size: 12px;
  color: var(--muted);
  text-align: left;
  cursor: pointer;
}
.nav-logout:hover {
  color: var(--ink);
}
</style>
