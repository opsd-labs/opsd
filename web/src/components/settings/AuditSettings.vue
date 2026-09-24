<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useWorkspace } from "../../workspace";
import { api } from "../../api";
import Field from "../resources/Field.vue";
import Button from "../ui/Button.vue";
import Input from "../ui/Input.vue";
import Select from "../ui/Select.vue";
import Notice from "../ui/Notice.vue";
import Status from "../ui/Status.vue";

/**
 * 审计查询。记录的是「谁在什么时候对哪个节点做了什么」，**不含操作内容**：
 * 凭据、环境变量与文件正文都不进审计，否则审计本身就成了新的泄漏面。
 */
const w = useWorkspace();
type Record_ = {
  id: string;
  at: number;
  actor: string;
  node_id: string;
  category: string;
  target: string;
  result: string;
  detail: string;
  source: string;
  bytes: number;
  duration: number;
};
const records = ref<Record_[]>([]);
const error = ref("");
const loading = ref(false);
const hours = ref("24");
const category = ref("");
const result = ref("");

const CATEGORIES = [
  { value: "", label: "全部类别" },
  { value: "会话", label: "会话" },
  { value: "文件", label: "文件" },
  { value: "防火墙", label: "防火墙" },
  { value: "容器", label: "容器" },
  { value: "分享页", label: "分享页" },
  { value: "主题", label: "主题" },
];
const RESULTS = [
  { value: "", label: "全部结果" },
  { value: "已开始", label: "已开始" },
  { value: "已结束", label: "已结束" },
  { value: "已提交", label: "已提交" },
  { value: "失败", label: "失败" },
];
const nodeNames = computed(() =>
  Object.fromEntries(w.nodes.map((e) => [e.node.id, e.node.name])),
);
const trimmed = computed(() =>
  records.value.slice(0, 500),
);

async function load() {
  loading.value = true;
  error.value = "";
  try {
    if (w.demo) {
      error.value = "演示模式：未读取审计记录";
      records.value = [];
      return;
    }
    const query = new URLSearchParams();
    if (hours.value) {
      query.set("from", String(Math.floor(Date.now() / 1000) - Number(hours.value) * 3600));
    }
    if (category.value) query.set("category", category.value);
    if (result.value) query.set("result", result.value);
    const data = await api(`/audit?${query}`);
    records.value = data.records || [];
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e);
  } finally {
    loading.value = false;
  }
}
watch([hours, category, result], load);
onMounted(load);

function tone(value: string) {
  if (value === "失败") return "danger" as const;
  if (value === "已开始") return "warning" as const;
  return "success" as const;
}
function mark(value: string) {
  if (value === "失败") return "failed" as const;
  if (value === "已开始") return "pending" as const;
  return "active" as const;
}
</script>
<template>
  <section class="settings-section">
    <h2>审计</h2>
    <Field label="时间范围">
      <Select
        v-model="hours"
        aria-label="审计时间范围"
        :options="[
          { value: '1', label: '近 1 小时' },
          { value: '24', label: '近 24 小时' },
          { value: '168', label: '近 7 天' },
          { value: '720', label: '近 30 天' },
        ]"
      />
      <Select v-model="category" aria-label="审计类别" :options="CATEGORIES" />
      <Select v-model="result" aria-label="审计结果" :options="RESULTS" />
      <Button :disabled="loading" @click="load">刷新</Button>
    </Field>
    <Notice v-if="error" tone="danger">{{ error }}</Notice>
    <p class="muted">
      审计只记录对象与结果：<strong>不记录文件内容、命令全文、凭据或 SQL 正文</strong>。
      记录保留一年后自动清理。
    </p>
    <table v-if="trimmed.length" class="settings-table">
      <thead>
        <tr>
          <th>时间</th>
          <th>节点</th>
          <th>类别</th>
          <th>对象</th>
          <th>结果</th>
          <th>来源</th>
          <th>传输</th>
          <th>时长</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="record in trimmed" :key="record.id">
          <td class="mono">
            {{ new Date(record.at * 1000).toLocaleString("zh-CN", { hour12: false }) }}
          </td>
          <td>{{ nodeNames[record.node_id] || record.node_id }}</td>
          <td>{{ record.category }}</td>
          <td class="mono">{{ record.target }}</td>
          <td>
            <Status :tone="tone(record.result)" :mark="mark(record.result)">{{
              record.result
            }}</Status>
          </td>
          <td class="mono">{{ record.source || "—" }}</td>
          <td class="mono">
            {{ record.bytes ? `${record.bytes} B` : "—" }}
          </td>
          <td class="mono">{{ record.duration ? `${record.duration} 秒` : "—" }}</td>
        </tr>
      </tbody>
    </table>
    <p v-else-if="!loading && !error" class="muted">
      该范围内没有审计记录。会话与文件操作从建立/提交时开始记录。
    </p>
  </section>
</template>
