<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { apiPath, getCsrf } from "../../api";
import Button from "../ui/Button.vue";
import Notice from "../ui/Notice.vue";
import Select from "../ui/Select.vue";

const props = defineProps<{ nodeId: string; demo: boolean }>();
const element = ref<HTMLElement>();
const workdir = ref("/");
let terminal: Terminal | undefined;
let socket: WebSocket | undefined;
let observer: ResizeObserver | undefined;
let fit: FitAddon | undefined;

const WORKDIRS = [
  { value: "/", label: "/" },
  { value: "/root", label: "/root" },
  { value: "/etc", label: "/etc" },
  { value: "/var/log", label: "/var/log" },
  { value: "/opt", label: "/opt" },
];

function connect() {
  if (!terminal || props.demo) return;
  socket?.close();
  terminal.clear();
  terminal.writeln(`正在建立到 ${props.nodeId} 的宿主机会话…`);
  const url = apiPath(
    `/nodes/${props.nodeId}/stream?kind=shell&workdir=${encodeURIComponent(workdir.value)}&csrf=${encodeURIComponent(getCsrf())}`,
  );
  socket = new WebSocket(
    `${location.protocol === "https:" ? "wss" : "ws"}://${location.host}${url}`,
  );
  socket.onmessage = (event) => {
    try {
      terminal?.write(Uint8Array.from(atob(event.data), (c) => c.charCodeAt(0)));
    } catch {
      terminal?.writeln(`\r\n${event.data}`);
    }
  };
  socket.onclose = () => terminal?.writeln("\r\n[会话已结束]");
  socket.onerror = () => terminal?.writeln("\r\n[连接失败]");
}

function disconnect() {
  socket?.close();
  socket = undefined;
}

onMounted(() => {
  const style = getComputedStyle(document.documentElement);
  terminal = new Terminal({
    convertEol: true,
    fontSize: 13,
    cursorBlink: true,
    fontFamily: "JetBrains Mono, Cascadia Code, Consolas, monospace",
    theme: {
      background: style.getPropertyValue("--terminal-bg").trim() || "#1a1a1a",
      foreground: style.getPropertyValue("--terminal-text").trim() || "#f5f3ee",
    },
  });
  fit = new FitAddon();
  terminal.loadAddon(fit);
  terminal.open(element.value!);
  observer = new ResizeObserver(() => {
    if (element.value?.clientWidth) fit?.fit();
  });
  observer.observe(element.value!);
  terminal.onData((data) => {
    const bytes = new TextEncoder().encode(data);
    let text = "";
    for (const byte of bytes) text += String.fromCharCode(byte);
    socket?.send(btoa(text));
  });
  if (props.demo) {
    terminal.writeln("演示模式 · 未连接服务器");
    return;
  }
  connect();
});
onUnmounted(() => {
  disconnect();
  observer?.disconnect();
  terminal?.dispose();
});
</script>

<template>
  <section class="hs">
    <Notice tone="warning">
      宿主机终端以 Agent 的权限运行。会话的建立与结束都会写入审计。
    </Notice>
    <div class="hs-toolbar">
      <Select v-model="workdir" :options="WORKDIRS" />
      <div class="hs-actions">
        <Button :disabled="demo" @click="connect">重新连接</Button>
        <Button variant="danger" :disabled="demo" @click="disconnect">结束会话</Button>
      </div>
    </div>
    <div ref="element" class="hs-terminal" />
  </section>
</template>

<style scoped>
.hs {
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.hs-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}
.hs-actions {
  display: flex;
  gap: 8px;
}
.hs-terminal {
  height: 320px;
  border: 1px solid var(--line);
  border-radius: var(--r);
  overflow: hidden;
}
</style>
