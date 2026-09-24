<script setup lang="ts">
import { defineAsyncComponent, provide } from "vue";
import { createWorkspace, workspaceKey } from "./workspace";
import AppShell from "./components/AppShell.vue";
import OperationDrawer from "./components/OperationDrawer.vue";
const TerminalDock = defineAsyncComponent(
  () => import("./components/TerminalDock.vue"),
);
import TaskPanel from "./components/TaskPanel.vue";
import LoginPage from "./pages/LoginPage.vue";
import ConsolePage from "./pages/ConsolePage.vue";
import NodesPage from "./pages/NodesPage.vue";
import DatabasePage from "./pages/DatabasePage.vue";
import DockerPage from "./pages/DockerPage.vue";
import FirewallPage from "./pages/FirewallPage.vue";
import StoragePage from "./pages/StoragePage.vue";
import SettingsPage from "./pages/SettingsPage.vue";
const w = createWorkspace();
provide(workspaceKey, w);
const showcase =
  import.meta.env.DEV && new URLSearchParams(location.search).has("components");
const Showcase = import.meta.env.DEV
  ? defineAsyncComponent(() => import("./pages/ComponentShowcase.vue"))
  : null;
</script>
<template>
  <div v-if="w.loading" class="loading" role="status">正在连接控制台…</div>
  <LoginPage v-else-if="!w.authenticated" />
  <template v-else
    ><AppShell
      ><component :is="Showcase" v-if="showcase" /><ConsolePage
        v-else-if="w.page === 'console'" /><NodesPage
        v-else-if="w.page === 'nodes'" /><DatabasePage
        v-else-if="w.page === 'database'" /><DockerPage
        v-else-if="w.page === 'docker'" /><FirewallPage
        v-else-if="w.page === 'firewall'" /><StoragePage
        v-else-if="w.page === 'storage'" /><SettingsPage v-else /></AppShell
    ><OperationDrawer /><TaskPanel /><TerminalDock v-if="w.sessions.length"
  /></template>
</template>
