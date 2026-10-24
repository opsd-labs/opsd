<script setup lang="ts">
/**
 * AppShell — 主布局骨架
 * grid: sidebar + main(topbar + content)
 */
import { computed, nextTick, watch } from 'vue'
import { workspace } from '../../state/workspace'
import Sidebar from './Sidebar.vue'
import Topbar from './Topbar.vue'
import ConsolePage from '../../pages/ConsolePage.vue'
import NodesPage from '../../pages/NodesPage.vue'
import FirewallPage from '../../pages/FirewallPage.vue'
import DatabasePage from '../../pages/DatabasePage.vue'
import DockerPage from '../../modules/docker/DockerPage.vue'
import NetworkPage from '../../modules/network/NetworkPage.vue'
import StoragePage from '../../modules/storage/StoragePage.vue'
import SettingsPage from '../../pages/SettingsPage.vue'

const page = computed(() => workspace.state.page)
const collapsed = computed(() => workspace.state.sidebarCollapsed)
const moved = new URLSearchParams(location.search).get('notice') === 'modules-moved'
watch(() => workspace.state.mobileNavOpen, async open => {
  await nextTick()
  document.querySelector<HTMLButtonElement>(open ? '.sidebar .nav-item' : '.mobile-menu')?.focus()
})
</script>

<template>
  <div class="shell" :class="{ 'shell--collapsed': collapsed }" @keydown.esc="workspace.setMobileNav(false)">
    <button v-if="workspace.state.mobileNavOpen" class="navigation-overlay" aria-label="关闭导航" @click="workspace.setMobileNav(false)" />
    <Sidebar />

    <div class="shell__main">
      <Topbar />

      <main class="shell__content">
        <p v-if="moved && page === 'console'" class="route-notice">模块管理已归入对应业务页面，请从 Docker、网络或存储菜单进入。</p>
        <p v-if="workspace.state.error" role="alert" class="shell__error">{{ workspace.state.error }}</p>
        <ConsolePage  v-if="page === 'console'" />
        <NodesPage    v-else-if="page === 'nodes'" />
        <FirewallPage v-else-if="page === 'firewall'" />
        <DatabasePage v-else-if="page === 'database'" />
        <DockerPage v-else-if="page === 'docker'" />
        <NetworkPage v-else-if="page === 'network'" />
        <StoragePage v-else-if="page === 'storage'" />
        <SettingsPage v-else-if="page === 'settings'" />
      </main>
    </div>
  </div>
</template>

<style scoped>
.navigation-overlay { display: none; }
.route-notice { color: var(--color-secondary); margin-bottom: var(--sp-3); }
@media (width < 900px) {
  .navigation-overlay { display: block; position: fixed; inset: 0; z-index: 49; border: 0; background: var(--color-overlay); }
  :deep(.sidebar) { visibility: hidden; }
  :deep(.sidebar--mobile-open) { visibility: visible; position: fixed; width: var(--sidebar-w); z-index: 50; left: 0; top: 0; }
}
@media (900px <= width < 1200px) { :deep(.sidebar) { width: var(--sidebar-w-collapsed); } :deep(.sidebar .nav-item__label), :deep(.sidebar .brand-name) { display: none; } }
.shell__error { color: var(--color-danger); margin-bottom: var(--sp-3); }
.shell {
  display: grid;
  grid-template-columns: var(--sidebar-w) 1fr;
  min-height: 100dvh;
  background: var(--color-bg);
  transition: grid-template-columns var(--duration-base) var(--ease-out);
}

.shell--collapsed {
  grid-template-columns: var(--sidebar-w-collapsed) 1fr;
}

.shell__main {
  display: flex;
  flex-direction: column;
  min-height: 100dvh;
  overflow: hidden;
}

.shell__content {
  flex: 1;
  overflow-y: auto;
  padding: var(--page-pad);
}

/* 中等屏幕：侧栏默认折叠 */
@media (width < 1200px) {
  .shell {
    grid-template-columns: var(--sidebar-w-collapsed) 1fr;
  }
}

/* 小屏幕：侧栏完全隐藏，通过 JS + overlay 展开 */
@media (width < 900px) {
  .shell {
    grid-template-columns: 0 1fr;
  }

  .shell--collapsed {
    grid-template-columns: 0 1fr;
  }
}
</style>
