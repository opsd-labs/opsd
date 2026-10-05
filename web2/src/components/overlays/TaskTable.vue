<script setup lang="ts">
import { computed } from "vue";
import type { Task } from "../../api";
import { statusNames, formatTime } from "../../api";
import { taskNames } from "../../workspace";
import DataTable from "../resources/DataTable.vue";
import Status from "../ui/Status.vue";
import Button from "../ui/Button.vue";
const props = defineProps<{
  tasks: Task[];
  nodeNames: Record<string, string>;
  search?: string;
  loading?: boolean;
}>();
defineEmits<{ detail: [task: Task]; plan: [task: Task] }>();
const rows = computed(() =>
  props.tasks.map((t) => ({
    ...t,
    name: taskNames[t.action.type] || "未知任务",
    target: props.nodeNames[t.node_id] || t.node_id,
    state: statusNames[t.status] || "未知状态",
    time: formatTime(t.updated_at || t.created_at),
    summary:
      t.error ||
      (t.status === "succeeded"
        ? "执行完成"
        : t.status === "pending"
          ? "等待下发"
          : "等待实际执行结果"),
  })),
);
const columns = [
  { key: "name", label: "任务", width: 160 },
  { key: "target", label: "目标节点", width: 140, sortable: true },
  { key: "state", label: "状态", width: 136 },
  { key: "time", label: "更新时间", width: 172, sortable: true },
  { key: "summary", label: "结果摘要", width: 280 },
  { key: "actions", label: "操作", width: 148 },
];
</script>
<template>
  <DataTable
    :rows="rows"
    :columns="columns"
    row-key="id"
    label="任务记录"
    :search="search"
    :loading="loading"
    ><template #cell-name="{ row }"
      ><span class="mono" :title="row.id">{{ row.name }}</span></template
    ><template #cell-state="{ row }"
      ><Status
        :tone="
          row.status === 'succeeded'
            ? 'success'
            : ['failed', 'uncertain', 'blocked', 'rollback_pending'].includes(
                  row.status,
                )
              ? 'danger'
              : 'neutral'
        "
        >{{ row.state }}</Status
      ></template
    ><template #cell-actions="{ row }"
      ><div class="inline">
        <Button variant="text" @click="$emit('detail', row)">详情</Button
        ><Button
          v-if="
            row.result?.plan_id &&
            ['firewall_plan', 'stack_plan', 'stack_create'].includes(
              row.action.type,
            )
          "
          variant="text"
          @click="$emit('plan', row)"
          >查看计划</Button
        >
      </div></template
    ></DataTable
  >
</template>
