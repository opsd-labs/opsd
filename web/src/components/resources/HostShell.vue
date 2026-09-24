<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { apiPath, getCsrf } from "../../api";
import Button from "../ui/Button.vue";
import Notice from "../ui/Notice.vue";
import Select from "../ui/Select.vue";

/**
 * 宿主机终端。会话建立在既有流通道上，与容器终端共用一套机制。
 *
 * Agent 侧只允许固定白名单内的 shell，并清空自己的环境变量后启动，
 * 因此终端里看不到节点身份与主控地址；起始目录同样过路径闸门。
 * 会话的建立与结束都会写入审计。
 */
const props = defineProps<{ nodeId: string; demo: boolean }>();
const element = ref<HTMLElement>();
const workdir = ref("/");
let terminal: Terminal | undefined,
  socket: WebSocket | undefined,
  observer: ResizeObserver | undefined,
  fit: FitAddon | undefined;

const WORKDIRS = [
  { value: "/", label: "起始目录 /" },
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
  socket.onerror = () =>
    terminal?.writeln("\r\n[连接失败；请确认节点在线且会话授权有效]");
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
    fontFamily: "Cascadia Code, Consolas, monospace",
    theme: {
      background: style.getPropertyValue("--terminal-bg").trim(),
      foreground: style.getPropertyValue("--terminal-text").trim(),
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
    // 终端输入是原始按键字节，与其他流一致地按 base64 发送
    const bytes = new TextEncoder().encode(data);
    let text = "";
    for (const byte of bytes) text += String.fromCharCode(byte);
    socket?.send(btoa(text));
  });
  if (props.demo) {
    terminal.writeln("演示模式 · 未连接服务器");
    terminal.writeln("真实环境下这里会打开一个受控的宿主机终端。");
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
  <section class="host-shell">
    <Notice tone="warning">
      宿主机终端以 Agent 的权限运行。Agent 只允许白名单内的 shell（
      <code class="mono">/bin/bash</code>、<code class="mono">/bin/sh</code>），
      并清空自身环境后启动，因此终端里看不到节点身份与主控地址。
      会话的建立与结束都会写入审计（谁、在哪个节点、起止时间与传输量）。
    </Notice>
    <div class="file-toolbar">
      <Select v-model="workdir" aria-label="起始目录" :options="WORKDIRS" />
      <div class="inline">
        <Button :disabled="demo" @click="connect">重新连接</Button>
        <Button variant="danger" :disabled="demo" @click="disconnect"
          >结束会话</Button
        >
      </div>
    </div>
    <div ref="element" class="terminal-host" />
  </section>
</template>
