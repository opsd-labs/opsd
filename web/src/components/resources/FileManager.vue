<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useWorkspace } from "../../workspace";
import { api, apiPath, getCsrf } from "../../api";
import Button from "../ui/Button.vue";
import Input from "../ui/Input.vue";
import Checkbox from "../ui/Checkbox.vue";
import Notice from "../ui/Notice.vue";
import Status from "../ui/Status.vue";
import { FolderUp, RefreshCw, Trash2, Upload } from "lucide-vue-next";

/**
 * 结构化文件管理。
 *
 * 列表与下载是**只读操作**，走流式请求-响应，不进任务表；
 * 上传、删除、重命名、改权限、建目录是**变更操作**，走任务机制，
 * 因此天然带幂等键与审计，也与防火墙、存储等变更串行。
 *
 * 路径校验发生在 Agent 侧：所有请求的路径都要过同一道闸门。
 */
type Entry = {
  name: string;
  kind: "file" | "dir" | "symlink";
  size: number;
  modified: number;
  readonly: boolean;
};
const props = defineProps<{ nodeId: string }>();
const w = useWorkspace();
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

/** 只读：列目录走流式请求-响应，一次性返回结果。 */
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
    error.value = "无法连接节点；请确认节点在线且仍持有有效证书";
  };
}

/** 变更：统一走任务接口，服务端会写审计并参与节点变更锁。 */
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
    notice.value = `已提交任务 ${result.task_id.slice(0, 8)}；结果可在任务面板查看`;
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
    error.value = "删除目录需要先勾选「递归删除」，并在确认后提交";
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
    // 第一步：把内容分块写进暂存文件
    const digest = await sha256(file);
    const url = apiPath(
      `/nodes/${props.nodeId}/stream?kind=file_write&path=${encodeURIComponent(target)}&csrf=${encodeURIComponent(getCsrf())}`,
    );
    await stream(socketUrl(url), file);
    // 第二步：提交任务，由 Agent 核对摘要后原子就位
    await mutate({ type: "put", path: target, digest, size: file.size });
    list(path.value);
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e);
  }
}

function socketUrl(url: string) {
  return `${location.protocol === "https:" ? "wss" : "ws"}://${location.host}${url}`;
}

/** 分块发送，避免一次性塞进一个 WebSocket 帧。 */
function stream(url: string, file: File) {
  return new Promise<void>((resolve, reject) => {
    const socket = new WebSocket(url);
    const chunk = 256 * 1024;
    socket.onopen = async () => {
      for (let offset = 0; offset < file.size; offset += chunk) {
        const slice = await file.slice(offset, offset + chunk).arrayBuffer();
        socket.send(base64(new Uint8Array(slice)));
      }
      // 关闭写端，Agent 会把暂存内容落盘并回执摘要
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
  <section class="file-manager">
    <header class="file-toolbar">
      <nav aria-label="路径" class="file-crumbs">
        <template v-for="(crumb, index) in crumbs" :key="crumb.value">
          <span v-if="index" class="muted">/</span>
          <button type="button" class="ui-button is-text" @click="list(crumb.value)">
            {{ crumb.label }}
          </button>
        </template>
      </nav>
      <div class="inline">
        <label class="ui-button" role="button">
          <Upload />
          <span>上传</span>
          <input
            type="file"
            class="sr-only"
            aria-label="上传文件"
            @change="pick"
          />
        </label>
        <Button :disabled="loading" @click="list(path)">
          <RefreshCw />刷新
        </Button>
      </div>
    </header>

    <Notice v-if="error" tone="danger">{{ error }}</Notice>
    <Notice v-if="notice">{{ notice }}</Notice>
    <Notice tone="warning">
      文件操作在节点侧强制校验路径：拒绝符号链接、拒绝
      <code class="mono">/proc</code>、<code class="mono">/sys</code>、
      <code class="mono">/dev</code>、Docker socket 与 Agent 自身数据目录。
      上传先写暂存文件，摘要核对通过才原子覆盖目标；<strong>失败不会破坏原文件</strong>。
    </Notice>

    <div class="file-actions">
      <Input
        v-model="renameText"
        aria-label="新名称"
        placeholder="新名称"
        class="search-input"
      />
      <Button
        :disabled="busy || !selected || !renameText"
        @click="
          mutate({ type: 'rename', from: join(selected), to: join(renameText) });
          renameText = '';
        "
        >重命名</Button
      >
      <Input
        v-model="mkdirText"
        aria-label="新目录名称"
        placeholder="新目录名称"
        class="search-input"
      />
      <Button
        :disabled="busy || !mkdirText"
        @click="
          mutate({ type: 'mkdir', path: join(mkdirText) });
          mkdirText = '';
        "
        ><FolderUp />新建目录</Button
      >
      <Checkbox v-model="recursive">递归删除</Checkbox>
      <Button
        variant="danger"
        :disabled="busy || !selected"
        @click="selectedEntry && remove(selectedEntry)"
        ><Trash2 />删除</Button
      >
      <Button
        :disabled="!selectedEntry || selectedEntry.kind !== 'file'"
        @click="selectedEntry && download(selectedEntry)"
        >下载</Button
      >
      <span v-if="selected" class="muted">已选：{{ selected }}</span>
    </div>

    <table class="file-table">
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
          <td colspan="4">正在读取目录…</td>
        </tr>
        <tr v-else-if="!entries.length">
          <td colspan="4" class="muted">该目录为空</td>
        </tr>
        <tr
          v-for="entry in entries"
          v-else
          :key="entry.name"
          :class="{ 'is-selected': selected === entry.name }"
          @click="selected = entry.name"
        >
          <td>
            <button
              type="button"
              class="ui-button is-text"
              :title="entry.kind === 'dir' ? '进入目录' : '选中'"
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
              >{{
                entry.kind === "dir" ? "目录" : entry.kind === "symlink" ? "符号链接" : "文件"
              }}</Status
            >
          </td>
          <td class="mono">{{ entry.kind === "file" ? bytes(entry.size) : "—" }}</td>
          <td class="mono">
            {{
              entry.modified
                ? new Date(entry.modified * 1000).toLocaleString("zh-CN", { hour12: false })
                : "—"
            }}
          </td>
        </tr>
      </tbody>
    </table>
    <p class="muted">
      符号链接单列为「符号链接」，不会被当成目录或文件；对它的读写一律被节点拒绝。
      只读操作（列目录、下载）不进任务表，变更操作会记入审计。
    </p>
  </section>
</template>
