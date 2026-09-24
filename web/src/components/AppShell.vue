<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useWorkspace, navigation } from "../workspace";
import {
  LayoutDashboard,
  Server,
  Database,
  Container,
  Shield,
  HardDrive,
  Settings,
  PanelLeftClose,
  PanelLeftOpen,
  LogOut,
  Activity,
  Terminal as TerminalIcon,
} from "lucide-vue-next";
import Button from "./ui/Button.vue";
import Select from "./ui/Select.vue";
import Status from "./ui/Status.vue";
import Notice from "./ui/Notice.vue";
import Hint from "./ui/Hint.vue";
const w = useWorkspace(),
  collapsed = ref(localStorage.getItem("opsd.collapsed") === "true");
watch(collapsed, (v) => localStorage.setItem("opsd.collapsed", String(v)));
/** 与 navigation 顺序一一对应。 */
const icons = [
  LayoutDashboard,
  Server,
  Database,
  Container,
  Shield,
  HardDrive,
  Settings,
];
const title = computed(
  () => navigation.find((n) => n.id === w.page)?.label || "组件样板",
);
/** 任务面板入口只显示数量，具体状态与结果在面板内查看。 */
const pending = computed(
  () => w.activeTasks.length + w.failures.length,
);
</script>
<template>
  <div class="app-shell" :class="{ 'sidebar-collapsed': collapsed }">
    <aside class="sidebar">
      <div class="brand"><Shield /><strong>opsd</strong></div>
      <nav aria-label="主导航">
        <Hint v-for="(n, i) in navigation" :key="n.id" :text="n.label"
          ><Button
            variant="text"
            :aria-label="n.label"
            :aria-current="w.page === n.id ? 'page' : undefined"
            @click="w.page = n.id"
            ><component :is="icons[i]" /><span>{{ n.label }}</span></Button
          ></Hint
        >
      </nav>
      <div class="sidebar-bottom">
        <Button
          variant="text"
          :aria-label="collapsed ? '展开侧栏' : '收起侧栏'"
          @click="collapsed = !collapsed"
          ><PanelLeftOpen v-if="collapsed" /><PanelLeftClose v-else /><span
            >收起侧栏</span
          ></Button
        ><Button variant="text" aria-label="退出登录" @click="w.logout"
          ><LogOut /><span>退出登录</span></Button
        >
      </div>
    </aside>
    <section class="main">
      <header class="topbar">
        <h1>{{ title }}</h1>
        <div class="topbar-controls">
          <Select
            v-model="w.environment"
            aria-label="节点环境"
            :options="[
              { value: '', label: '全部节点' },
              ...w.nodes
                .filter((e) => !e.node.revoked)
                .map((e) => ({ value: e.node.id, label: e.node.name })),
            ]"
          /><Button
            v-if="pending"
            variant="text"
            :aria-label="`任务与异常，共 ${pending} 项`"
            @click="w.taskPanel = true"
            ><Activity /><span>{{ pending }}</span></Button
          ><Button
            v-else
            variant="text"
            aria-label="任务记录"
            @click="w.taskPanel = true"
            ><Activity /><span>任务</span></Button
          ><Button
            v-if="w.sessions.length && !w.terminalVisible"
            variant="text"
            :aria-label="`容器会话 ${w.sessions.length} 个`"
            @click="w.terminalVisible = true"
            ><TerminalIcon /><span>{{ w.sessions.length }}</span></Button
          ><Status v-if="w.demo" tone="warning">演示 · 不执行操作</Status
          ><Status v-else>管理员</Status>
        </div>
      </header>
      <main class="content">
        <Notice
          v-if="w.error && !w.modal"
          tone="danger"
          dismissible
          @dismiss="w.error = ''"
          >{{ w.error }}</Notice
        ><Notice v-if="w.notice" dismissible @dismiss="w.notice = ''">{{
          w.notice
        }}</Notice>
        <slot />
      </main>
    </section>
  </div>
</template>
