<script setup lang="ts">
import { computed, onMounted, ref, watch, inject } from "vue";
import { workspaceKey } from "../../workspace";
import { api, apiPath, getCsrf } from "../../api";
import Button from "../ui/Button.vue";
import Input from "../ui/Input.vue";
import Checkbox from "../ui/Checkbox.vue";
import Notice from "../ui/Notice.vue";
import Status from "../ui/Status.vue";

type Entry = {
  name: string;
  kind: "file" | "dir" | "symlink";
  size: number;
  modified: number;
  readonly: boolean;
};

const props = defineProps<{ nodeId: string }>();
const w = inject(workspaceKey)!;
const path = ref("/");
const entries = ref<Entry[]>([]);
const loading = ref(false);
const error = ref("");
const notice = ref("");
const busy = ref(false);
const recursive = ref(false);
const selected = ref("");
const renameText = ref("");
const mkdirText = ref("");

const crumbs = computed(() => {
  const parts = path.value.split("/").filter(Boolean);
  const result = [{ label: "/", value: "/" }];
  let current = "";
  for (const part of parts) {
    current += `/${part}`;
    result.push({ label: part, value: current });
  }
  return result;
});
const selectedEntry = computed(() =>
  entries.value.find((e) => e.name === selected.value),
);
const join = (name: string) =>
  path.value === "/" ? `/${name}` : `${path.value}/${name}`;

const bytes = (value: number) => {
  if (value < 1024) return `${value} B`;
  const units = ["KiB", "MiB", "GiB", "TiB"];
  let size = value / 1024;
  let unit = 0;
  while (size >= 1024 && unit < units.length - 1) {
    size /= 1024;
    unit += 1;
  }
  return `${size.toFixed(1)} ${units[unit]}`;
};

function list(target: string) {
  if (w.demo) {
    entries.value = [
      { name: "etc", kind: "dir", size: 4096, modified: 0, readonly: false },
      { name: "opt", kind: "dir", size: 4096, modified: 0, readonly: false },
      { name: "app.log", kind: "file", size: 20480, modified: 0, readonly: false },
      { name: "link", kind: "symlink", size: 12, modified: 0, readonly: false },
    ];
    path.value = target;
    error.value = "";
    return;
  }
  loading.value = true;
  error.value = "";
  const url = apiPath(
    `/nodes/${props.nodeId}/stream?kind=file_list&path=${encodeURIComponent(target)}&csrf=${encodeURIComponent(getCsrf())}`,
  );
  const socket = new WebSocket(
    `${location.protocol === "https:" ? "wss" : "ws"}://${location.host}${url}`,
  );
  socket.onmessage = (event) => {
    try {
      const payload = JSON.parse(
        new TextDecoder().decode(
          Uint8Array.from(atob(event.data), (c) => c.charCodeAt(0)),
        ),
      );
      if (payload.error) {
        error.value = payload.error;
        socket.close();
        return;
      }
      entries.value = payload.entries || [];
      path.value = payload.path || target;
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e);
    }
  };
  socket.onclose = () => (loading.value = false);
  socket.onerror = () => {
    loading.value = false;
    error.value = "无法连接节点";
  };
}

async function mutate(action: Record<string, unknown>) {
  busy.value = true;
  error.value = "";
  notice.value = "";
  try {
    if (w.demo) {
      notice.value = "演示模式：未提交文件变更";
      return;
    }
    const result = await api(`/nodes/${props.nodeId}/files`, "POST", {
      idempotency_key: crypto.randomUUID(),
      action,
    });
    notice.value = `已提交任务 ${result.task_id.slice(0, 8)}`;
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e);
  } finally {
    busy.value = false;
  }
}

function enter(entry: Entry) {
  if (entry.kind !== "dir") {
    selected.value = entry.name;
    return;
  }
  list(join(entry.name));
}

function remove(entry: Entry) {
  const target = join(entry.name);
  if (entry.kind === "dir" && !recursive.value) {
    error.value = "删除目录需要勾选「递归删除」";
    return;
  }
  mutate({
    type: "remove",
    path: target,
    recursive: entry.kind === "dir" && recursive.value,
    confirmed: true,
  });
}

async function upload(file: File) {
  error.value = "";
  notice.value = "";
  const target = path.value === "/" ? `/${file.name}` : `${path.value}/${file.name}`;
  try {
    const digest = await sha256(file);
    const url = apiPath(
      `/nodes/${props.nodeId}/stream?kind=file_write&path=${encodeURIComponent(target)}&csrf=${encodeURIComponent(getCsrf())}`,
    );
    await stream(socketUrl(url), file);
    await mutate({ type: "put", path: target, digest, size: file.size });
    list(path.value);
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e);
  }
}

function socketUrl(url: string) {
  return `${location.protocol === "https:" ? "wss" : "ws"}://${location.host}${url}`;
}

function stream(url: string, file: File) {
  return new Promise<void>((resolve, reject) => {
    const socket = new WebSocket(url);
    const chunk = 256 * 1024;
    socket.onopen = async () => {
      for (let offset = 0; offset < file.size; offset += chunk) {
        const slice = await file.slice(offset, offset + chunk).arrayBuffer();
        socket.send(base64(new Uint8Array(slice)));
      }
      setTimeout(() => socket.close(), 100);
    };
    socket.onerror = () => reject(new Error("上传连接失败"));
    socket.onclose = () => resolve();
  });
}

function base64(data: Uint8Array) {
  let text = "";
  for (const byte of data) text += String.fromCharCode(byte);
  return btoa(text);
}

async function sha256(file: File) {
  const buffer = await file.arrayBuffer();
  const hash = await crypto.subtle.digest("SHA-256", buffer);
  return Array.from(new Uint8Array(hash))
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

function download(entry: Entry) {
  const target = join(entry.name);
  const url = apiPath(
    `/nodes/${props.nodeId}/stream?kind=file_read&path=${encodeURIComponent(target)}&csrf=${encodeURIComponent(getCsrf())}`,
  );
  const socket = new WebSocket(socketUrl(url));
  socket.binaryType = "arraybuffer";
  const parts: Uint8Array[] = [];
  socket.onmessage = (event) => {
    const raw = Uint8Array.from(atob(event.data), (c) => c.charCodeAt(0));
    try {
      const payload = JSON.parse(new TextDecoder().decode(raw));
      if (payload.error) {
        error.value = payload.error;
        socket.close();
        return;
      }
    } catch {
      parts.push(raw);
    }
  };
  socket.onclose = () => {
    if (!parts.length) return;
    const blob = new Blob(parts as BlobPart[]);
    const link = document.createElement("a");
    link.href = URL.createObjectURL(blob);
    link.download = entry.name;
    link.click();
    URL.revokeObjectURL(link.href);
  };
}

function pick(event: Event) {
  const input = event.target as HTMLInputElement;
  const file = input.files?.[0];
  if (file) upload(file);
  input.value = "";
}

watch(() => props.nodeId, () => {
  path.value = "/";
  selected.value = "";
  list("/");
});
onMounted(() => list("/"));
</script>

<template>
  <section class="fm">
    <header class="fm-toolbar">
      <nav class="fm-crumbs">
        <template v-for="(crumb, index) in crumbs" :key="crumb.value">
          <span v-if="index" class="fm-sep">/</span>
          <button class="fm-crumb" @click="list(crumb.value)">{{ crumb.label }}</button>
        </template>
      </nav>
      <div class="fm-actions">
        <label class="fm-upload">
          <span>上传</span>
          <input type="file" class="sr-only" @change="pick" />
        </label>
        <Button :disabled="loading" @click="list(path)">刷新</Button>
      </div>
    </header>

    <Notice v-if="error" tone="danger">{{ error }}</Notice>
    <Notice v-if="notice">{{ notice }}</Notice>

    <div class="fm-controls">
      <Input v-model="renameText" placeholder="新名称" />
      <Button :disabled="busy || !selected || !renameText" @click="mutate({ type: 'rename', from: join(selected), to: join(renameText) }); renameText = ''">重命名</Button>
      <Input v-model="mkdirText" placeholder="新目录名称" />
      <Button :disabled="busy || !mkdirText" @click="mutate({ type: 'mkdir', path: join(mkdirText) }); mkdirText = ''">新建目录</Button>
      <Checkbox v-model="recursive">递归删除</Checkbox>
      <Button variant="danger" :disabled="busy || !selected" @click="selectedEntry && remove(selectedEntry)">删除</Button>
      <Button :disabled="!selectedEntry || selectedEntry.kind !== 'file'" @click="selectedEntry && download(selectedEntry)">下载</Button>
    </div>

    <table class="fm-table">
      <thead>
        <tr>
          <th>名称</th>
          <th>类型</th>
          <th>大小</th>
          <th>修改时间</th>
        </tr>
      </thead>
      <tbody>
        <tr v-if="loading">
          <td colspan="4" class="fm-state">正在读取目录…</td>
        </tr>
        <tr v-else-if="!entries.length">
          <td colspan="4" class="fm-state">该目录为空</td>
        </tr>
        <tr
          v-else
          v-for="entry in entries"
          :key="entry.name"
          :class="{ 'fm-selected': selected === entry.name }"
          @click="selected = entry.name"
        >
          <td>
            <button
              class="fm-name"
              @dblclick="enter(entry)"
              @click="entry.kind === 'dir' ? enter(entry) : (selected = entry.name)"
            >
              {{ entry.name }}{{ entry.kind === "dir" ? "/" : "" }}
            </button>
          </td>
          <td>
            <Status
              :tone="entry.kind === 'symlink' ? 'warning' : 'neutral'"
              :mark="entry.kind === 'symlink' ? 'pending' : 'neutral'"
            >{{ entry.kind === "dir" ? "目录" : entry.kind === "symlink" ? "符号链接" : "文件" }}</Status>
          </td>
          <td class="fm-mono">{{ entry.kind === "file" ? bytes(entry.size) : "—" }}</td>
          <td class="fm-mono">
            {{ entry.modified ? new Date(entry.modified * 1000).toLocaleString("zh-CN", { hour12: false }) : "—" }}
          </td>
        </tr>
      </tbody>
    </table>
  </section>
</template>

<style scoped>
.fm {
  display: flex;
  flex-direction: column;
  gap: 16px;
}
.fm-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 8px 0;
  border-bottom: 1px solid var(--line);
}
.fm-crumbs {
  display: flex;
  align-items: center;
  gap: 4px;
  font-size: 12px;
}
.fm-sep {
  color: var(--muted);
}
.fm-crumb {
  background: none;
  border: none;
  padding: 2px 4px;
  font: inherit;
  font-size: 12px;
  color: var(--ink);
  cursor: pointer;
}
.fm-crumb:hover {
  text-decoration: underline;
}
.fm-actions {
  display: flex;
  gap: 8px;
}
.fm-upload {
  display: inline-flex;
  align-items: center;
  padding: 6px 12px;
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: var(--r);
  font-size: 12px;
  cursor: pointer;
}
.fm-upload:hover {
  border-color: var(--ink);
}
.fm-controls {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}
.fm-table {
  width: 100%;
  border-collapse: collapse;
}
.fm-table th {
  padding: 8px 12px;
  text-align: left;
  font-size: 11px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--muted);
  border-bottom: 2px solid var(--ink);
}
.fm-table td {
  padding: 8px 12px;
  font-size: 12px;
  border-bottom: 1px solid var(--line);
}
.fm-state {
  text-align: left;
  color: var(--muted);
}
.fm-selected {
  background: var(--bg);
}
.fm-name {
  background: none;
  border: none;
  padding: 0;
  font: inherit;
  font-size: 12px;
  color: var(--ink);
  cursor: pointer;
}
.fm-name:hover {
  text-decoration: underline;
}
.fm-mono {
  font-family: "JetBrains Mono", "Fira Code", monospace;
  font-size: 11px;
}
</style>
