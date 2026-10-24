<script setup lang="ts">
import { nextTick, onUnmounted, ref, watch } from 'vue'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import '@xterm/xterm/css/xterm.css'
import Button from '../../components/ui/Button.vue'
import { apiPost, entrancePath } from '../../api/client'
import { workspace } from '../../state/workspace'
import { useTheme } from '../../composables/useTheme'

const props = defineProps<{ nodeId: string; container: string }>()
const display = ref<HTMLElement | null>(null)
const command = ref('/bin/sh')
const message = ref('尚未连接')
const { resolved } = useTheme()
let socket: WebSocket | undefined
let terminal: Terminal | undefined
let observer: ResizeObserver | undefined
let controller: AbortController | undefined
let execId = ''
function colors() {
  const style = getComputedStyle(document.documentElement)
  return { background: style.getPropertyValue('--color-bg').trim(), foreground: style.getPropertyValue('--color-ink').trim(), cursor: style.getPropertyValue('--color-accent').trim() }
}
function disconnect() {
  controller?.abort(); observer?.disconnect()
  if (socket) { socket.onclose = null; socket.onerror = null; socket.onmessage = null; socket.close() }
  terminal?.dispose(); terminal = undefined; socket = undefined; execId = ''; message.value = '终端已断开'
}
async function connect() {
  disconnect(); await nextTick()
  if (!display.value) return
  controller = new AbortController()
  terminal = new Terminal({ cursorBlink: true, fontSize: 13, scrollback: 2000, theme: colors() })
  const fit = new FitAddon()
  terminal.loadAddon(fit); terminal.open(display.value); fit.fit()
  const nodeId = props.nodeId
  const url = new URL(`${entrancePath()}/api/v1/plugins/docker/stream/terminal`, location.origin)
  url.protocol = location.protocol === 'https:' ? 'wss:' : 'ws:'
  url.searchParams.set('csrf', workspace.state.session?.csrf ?? '')
  socket = new WebSocket(url); socket.binaryType = 'arraybuffer'
  const resize = () => {
    if (!terminal || !execId) return
    void apiPost('/api/v1/plugins/docker/api/terminal/resize', { node_id: nodeId, exec_id: execId, rows: terminal.rows, cols: terminal.cols }, { signal: controller?.signal })
      .catch(error => { if (!controller?.signal.aborted) message.value = error.message })
  }
  socket.onopen = () => socket?.send(JSON.stringify({ node_id: nodeId, container: props.container, command: [command.value] }))
  socket.onmessage = event => {
    const text = typeof event.data === 'string' ? event.data : new TextDecoder().decode(event.data)
    try {
      const value = JSON.parse(text)
      if (value.type === 'ready') { execId = value.exec_id; message.value = '已连接'; resize(); terminal?.focus(); return }
      if (value.error) { message.value = value.error; return }
    } catch { /* 普通输出直接交给终端。 */ }
    terminal?.write(text)
  }
  socket.onerror = () => { message.value = '终端连接失败' }
  socket.onclose = () => { message.value = '终端已断开' }
  terminal.onData(data => { if (socket?.readyState === WebSocket.OPEN) socket.send(data) })
  terminal.onResize(resize)
  observer = new ResizeObserver(() => fit.fit()); observer.observe(display.value)
}
watch(resolved, async () => { await nextTick(); if (terminal) terminal.options.theme = colors() })
watch(() => [props.nodeId, props.container], disconnect)
onUnmounted(disconnect)
</script>

<template>
  <div class="module-page">
    <div class="module-row"><label class="module-field"><span>命令</span><select v-model="command"><option>/bin/sh</option><option>/bin/bash</option></select></label><Button @click="connect">连接</Button><Button @click="socket?.send('\u0003')">发送 Ctrl+C</Button><Button @click="disconnect">断开</Button></div>
    <p role="status" class="module-muted">{{ message }}</p>
    <div ref="display" class="terminal-display" aria-label="容器交互终端" />
  </div>
</template>

<style scoped>
.terminal-display { height: 360px; min-width: 0; padding: var(--sp-2); background: var(--color-bg); border: 1px solid var(--color-border); border-radius: var(--radius-md); overflow: hidden; }
</style>
