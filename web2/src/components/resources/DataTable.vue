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

const page = ref(1);
const sortKey = ref("");
const descending = ref(false);

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
  <section class="dt" :aria-label="label" :aria-busy="loading">
    <div class="dt-scroll" tabindex="0">
      <table class="dt-table">
        <caption class="sr-only">{{ label }}</caption>
        <colgroup>
          <col
            v-for="col in columns"
            :key="col.key"
            :style="col.width ? { width: col.width + 'px' } : {}"
          />
        </colgroup>
        <thead>
          <tr>
            <th
              v-for="col in columns"
              :key="col.key"
              :aria-sort="
                sortKey === col.key
                  ? descending
                    ? 'descending'
                    : 'ascending'
                  : undefined
              "
            >
              <button
                v-if="col.sortable"
                class="dt-sort"
                @click="sort(col.key)"
              >
                {{ col.label }}
                <span class="dt-sort-arrow" aria-hidden="true">
                  {{ sortKey === col.key ? (descending ? "↓" : "↑") : "↕" }}
                </span>
              </button>
              <span v-else class="dt-head">{{ col.label }}</span>
            </th>
          </tr>
        </thead>
        <tbody>
          <tr v-if="loading || error || unknown || !visible.length" class="dt-state">
            <td
              :colspan="columns.length"
              :class="{ 'dt-state--error': error }"
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
          <tr
            v-else
            v-for="row in visible"
            :key="row[rowKey]"
            class="dt-row"
          >
            <td
              v-for="col in columns"
              :key="col.key"
              :class="{ 'dt-mono': col.mono }"
              :title="typeof row[col.key] === 'string' ? row[col.key] : undefined"
            >
              <slot :name="`cell-${col.key}`" :row="row" :value="row[col.key]">
                {{ row[col.key] ?? "—" }}
              </slot>
            </td>
          </tr>
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

<style scoped>
.dt {
  border: none;
}
.dt-scroll {
  overflow-x: auto;
  -webkit-overflow-scrolling: touch;
}
.dt-table {
  width: 100%;
  border-collapse: collapse;
  border-spacing: 0;
}
.dt-table th {
  padding: 0 12px;
  height: 36px;
  text-align: left;
  font-size: 11px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--muted);
  border-bottom: 2px solid var(--ink);
  white-space: nowrap;
}
.dt-table td {
  padding: 0 12px;
  height: 40px;
  font-size: 12px;
  line-height: 1.4;
  color: var(--ink);
  border-bottom: 1px solid var(--line);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 300px;
}
.dt-mono {
  font-family: "JetBrains Mono", "Fira Code", monospace;
  font-size: 11px;
}
.dt-sort {
  background: none;
  border: none;
  padding: 0;
  font: inherit;
  font-size: 11px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--muted);
  cursor: pointer;
  display: inline-flex;
  align-items: center;
  gap: 4px;
}
.dt-sort:hover {
  color: var(--ink);
}
.dt-sort-arrow {
  font-size: 12px;
  line-height: 1;
}
.dt-head {
  display: inline-block;
}
.dt-row:hover {
  background: var(--bg);
}
.dt-state td {
  padding: 40px 12px;
  text-align: left;
  font-size: 13px;
  color: var(--muted);
  border-bottom: none;
}
.dt-state--error td {
  color: var(--accent);
}
</style>
