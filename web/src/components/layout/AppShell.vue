<script setup lang="ts">
/**
 * AppShell — 主布局骨架
 * grid: sidebar + main(topbar + content)
 */
import { computed } from 'vue'
import { workspace } from '../../state/workspace'
import Sidebar from './Sidebar.vue'
import Topbar from './Topbar.vue'
import ConsolePage from '../../pages/ConsolePage.vue'
import NodesPage from '../../pages/NodesPage.vue'
import DockerPage from '../../pages/DockerPage.vue'
import FirewallPage from '../../pages/FirewallPage.vue'
import DatabasePage from '../../pages/DatabasePage.vue'
import StoragePage from '../../pages/StoragePage.vue'
import SettingsPage from '../../pages/SettingsPage.vue'

const page = computed(() => workspace.state.page)
const collapsed = computed(() => workspace.state.sidebarCollapsed)
</script>

<template>
  <div class="shell" :class="{ 'shell--collapsed': collapsed }">
    <Sidebar />

    <div class="shell__main">
      <Topbar />

      <main class="shell__content">
        <p v-if="workspace.state.error" role="alert" class="shell__error">{{ workspace.state.error }}</p>
        <ConsolePage  v-if="page === 'console'" />
        <NodesPage    v-else-if="page === 'nodes'" />
        <DockerPage   v-else-if="page === 'docker'" />
        <FirewallPage v-else-if="page === 'firewall'" />
        <DatabasePage v-else-if="page === 'database'" />
        <StoragePage  v-else-if="page === 'storage'" />
        <SettingsPage v-else-if="page === 'settings'" />
      </main>
    </div>
  </div>
</template>

<style scoped>
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
