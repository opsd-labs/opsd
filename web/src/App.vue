<script setup lang="ts">
import { onMounted, computed } from 'vue'
import { workspace } from './state/workspace'
import ToastProvider from './components/toast/ToastProvider.vue'
import LoginPage from './pages/LoginPage.vue'
import AppShell from './components/layout/AppShell.vue'

onMounted(() => {
  workspace.init()
})

const isLoggedIn = computed(() => workspace.state.session !== null)
const isLoading = computed(() => workspace.state.loading)
</script>

<template>
  <ToastProvider>
    <!-- 初始连接中 -->
    <div v-if="isLoading" class="app-loading" role="status" aria-label="正在连接控制台">
      <div class="app-loading__spinner" aria-hidden="true"></div>
      <span class="app-loading__text">正在连接…</span>
    </div>

    <!-- 未登录 -->
    <LoginPage v-else-if="!isLoggedIn" />

    <!-- 主界面 -->
    <AppShell v-else />
  </ToastProvider>
</template>

<style scoped>
.app-loading {
  min-height: 100dvh;
  display: grid;
  place-items: center;
  gap: var(--sp-3);
  background: var(--color-bg);
}

/* 内部 grid 无法直接用 flex-direction，用嵌套 flex */
.app-loading {
  place-items: center;
}

.app-loading__spinner {
  width: 28px;
  height: 28px;
  border: 2px solid var(--color-border);
  border-top-color: var(--color-accent);
  border-radius: var(--radius-full);
  animation: spin 600ms linear infinite;
}

.app-loading__text {
  font-size: var(--text-sm);
  color: var(--color-muted);
}

@keyframes spin {
  to { transform: rotate(360deg); }
}
</style>
