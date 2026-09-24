<script setup lang="ts">
import { ref } from "vue";
import { useWorkspace } from "../workspace";
import Drawer from "./ui/Drawer.vue";
import Tabs from "./ui/Tabs.vue";
import Button from "./ui/Button.vue";
import TerminalSession from "./TerminalSession.vue";
const w = useWorkspace(),
  host = ref<HTMLElement>(),
  parking = ref<HTMLElement>();
</script>
<template>
  <div ref="parking" hidden />
  <Teleport v-if="parking" :to="w.terminalVisible && host ? host : parking"
    ><TerminalSession
      v-for="s in w.sessions"
      v-show="s.id === w.activeSession"
      :key="s.id"
      :container="s.container"
      :terminal="s.terminal"
      :demo="w.demo" /></Teleport
  ><Drawer
    v-model:open="w.terminalVisible"
    title="容器会话"
    terminal
    description="收起后保留会话；结束会话才关闭连接。"
    ><Tabs
      v-model="w.activeSession"
      :items="
        w.sessions.map((s) => ({
          value: s.id,
          label: `${s.container.node_name} / ${s.container.names?.[0]} · ${s.terminal ? '终端' : '日志'}`,
        }))
      "
    />
    <div ref="host" class="terminal-host" />
    <template #footer
      ><Button @click="w.terminalVisible = false">收起会话</Button
      ><Button variant="danger" @click="w.endStream(w.activeSession)"
        >结束当前会话</Button
      ></template
    ></Drawer
  >
</template>
