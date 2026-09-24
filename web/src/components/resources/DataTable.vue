<script setup lang="ts">
import { computed, ref, watch } from "vue";
import Button from "../ui/Button.vue";
import Pagination from "./Pagination.vue";
export type Column = {
  key: string;
  label: string;
  width?: number;
  mono?: boolean;
  sortable?: boolean;
};
const props = withDefaults(
  defineProps<{
    rows: any[];
    columns: Column[];
    rowKey: string;
    label: string;
    search?: string;
    filtered?: boolean;
    loading?: boolean;
    error?: string;
    unknown?: boolean;
    emptyText?: string;
    pageSize?: number;
  }>(),
  { search: "", pageSize: 50, emptyText: "暂无资源" },
);
const page = ref(1),
  sortKey = ref(""),
  descending = ref(false);
const filteredRows = computed(() => {
  const query = props.search.trim().toLowerCase();
  let rows = props.rows.filter(
    (row) =>
      !query ||
      props.columns.some((c) =>
        String(row[c.key] ?? "")
          .toLowerCase()
          .includes(query),
      ),
  );
  if (sortKey.value)
    rows = [...rows].sort(
      (a, b) =>
        String(a[sortKey.value] ?? "").localeCompare(
          String(b[sortKey.value] ?? ""),
          "zh-CN",
          { numeric: true },
        ) * (descending.value ? -1 : 1),
    );
  return rows;
});
const pages = computed(() =>
  Math.max(1, Math.ceil(filteredRows.value.length / props.pageSize)),
);
const visible = computed(() =>
  filteredRows.value.slice(
    (page.value - 1) * props.pageSize,
    page.value * props.pageSize,
  ),
);
watch(
  () => [props.search, props.filtered, sortKey.value, descending.value],
  () => (page.value = 1),
);
watch(pages, (value) => (page.value = Math.min(page.value, value)));
function sort(key: string) {
  if (sortKey.value === key) descending.value = !descending.value;
  else {
    sortKey.value = key;
    descending.value = false;
  }
}
</script>
<template>
  <section class="resource-table" :aria-label="label" :aria-busy="loading">
    <div class="table-scroll" tabindex="0" :aria-label="`${label}滚动区域`">
      <table>
        <caption class="sr-only">
          {{
            label
          }}
        </caption>
        <colgroup>
          <col
            v-for="column in columns"
            :key="column.key"
            :style="column.width ? { width: column.width + 'px' } : {}"
          />
        </colgroup>
        <thead>
          <tr>
            <th
              v-for="column in columns"
              :key="column.key"
              :aria-sort="
                sortKey === column.key
                  ? descending
                    ? 'descending'
                    : 'ascending'
                  : undefined
              "
            >
              <Button
                v-if="column.sortable"
                variant="text"
                @click="sort(column.key)"
                >{{ column.label }}
                <span aria-hidden="true">{{
                  sortKey === column.key ? (descending ? "↓" : "↑") : "↕"
                }}</span></Button
              ><template v-else>{{ column.label }}</template>
            </th>
          </tr>
        </thead>
        <tbody>
          <tr
            v-if="loading || error || unknown || !visible.length"
            class="table-state"
          >
            <td
              :colspan="columns.length"
              :class="{ 'tone-danger': error }"
              :role="error ? 'alert' : 'status'"
            >
              {{
                loading
                  ? "正在读取…"
                  : error
                    ? `读取失败：${error}`
                    : unknown
                      ? "尚未采集，资源数量未知"
                      : search || filtered
                        ? "没有匹配结果"
                        : emptyText
              }}
            </td>
          </tr>
          <template v-else
            ><tr v-for="row in visible" :key="row[rowKey]" class="data-row">
              <td
                v-for="column in columns"
                :key="column.key"
                :class="{ mono: column.mono }"
                :title="
                  typeof row[column.key] === 'string'
                    ? row[column.key]
                    : undefined
                "
              >
                <slot
                  :name="`cell-${column.key}`"
                  :row="row"
                  :value="row[column.key]"
                  >{{ row[column.key] ?? "—" }}</slot
                >
              </td>
            </tr></template
          >
        </tbody>
      </table>
    </div>
    <Pagination
      v-if="!loading && !error && !unknown"
      v-model:page="page"
      :pages="pages"
      :total="filteredRows.length"
      :size="pageSize"
    />
  </section>
</template>
