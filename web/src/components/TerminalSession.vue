<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { apiPath, getCsrf } from "../api";
const props = defineProps<{
  container: any;
  terminal: boolean;
  demo: boolean;
}>();
const element = ref<HTMLElement>();
let terminal: Terminal | undefined,
  socket: WebSocket | undefined,
  observer: ResizeObserver | undefined;
onMounted(() => {
  const style = getComputedStyle(document.documentElement);
  terminal = new Terminal({
    convertEol: true,
    fontSize: 13,
    fontFamily: "Cascadia Code, Consolas, monospace",
    cursorBlink: props.terminal,
    theme: {
      background: style.getPropertyValue("--terminal-bg").trim(),
      foreground: style.getPropertyValue("--terminal-text").trim(),
    },
  });
  const fit = new FitAddon();
  terminal.loadAddon(fit);
  terminal.open(element.value!);
  observer = new ResizeObserver(() => {
    if (element.value?.clientWidth) fit.fit();
  });
  observer.observe(element.value!);
  if (props.demo) {
    terminal.writeln("演示模式 · 未连接服务器");
    return;
  }
  const path = apiPath(
    `/nodes/${props.container.node_id}/stream?kind=container&container=${encodeURIComponent(props.container.id)}&csrf=${encodeURIComponent(getCsrf())}`,
  );
  socket = new WebSocket(
    `${location.protocol === "https:" ? "wss" : "ws"}://${location.host}${path}`,
  );
  socket.onmessage = (e) => {
    try {
      terminal?.write(Uint8Array.from(atob(e.data), (c) => c.charCodeAt(0)));
    } catch {
      terminal?.writeln("\r\n" + e.data);
    }
  };
  socket.onclose = () => terminal?.writeln("\r\n[连接已关闭]");
  socket.onerror = () => terminal?.writeln("\r\n[连接失败，请检查节点和权限]");
  if (props.terminal)
    terminal.onData((data) => {
      if (socket?.readyState === WebSocket.OPEN)
        socket.send(
          btoa(String.fromCharCode(...new TextEncoder().encode(data))),
        );
    });
});
onUnmounted(() => {
  observer?.disconnect();
  socket?.close();
  terminal?.dispose();
});
</script>
<template>
  <div
    ref="element"
    class="terminal-surface"
    :aria-label="terminal ? '容器终端输出' : '容器日志输出'"
  />
</template>
