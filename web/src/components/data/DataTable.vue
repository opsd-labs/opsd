<script setup lang="ts">
/**
 * DataTable — 通用数据表格
 * 含：sticky 表头、排序、skeleton loading、空/错误状态
 */
import { ref, computed } from 'vue'
import Skeleton from '../ui/Skeleton.vue'

export interface Column<T = Record<string, unknown>> {
  key: string
  label: string
  width?: string
  align?: 'left' | 'right' | 'center'
  sortable?: boolean
  render?: (row: T) => unknown  // slot 覆盖更常用
}

const props = withDefaults(defineProps<{
  columns: Column[]
  rows: Record<string, unknown>[]
  loading?: boolean
  error?: string | null
  emptyText?: string
  skeletonRows?: number
  rowKey?: string
}>(), {
  emptyText: '暂无数据',
  skeletonRows: 6,
  rowKey: 'id',
})

// 排序状态
const sortKey = ref<string | null>(null)
const sortDir = ref<'asc' | 'desc'>('asc')

function toggleSort(col: Column) {
  if (!col.sortable) return
  if (sortKey.value === col.key) {
    sortDir.value = sortDir.value === 'asc' ? 'desc' : 'asc'
  } else {
    sortKey.value = col.key
    sortDir.value = 'asc'
  }
}

const sortedRows = computed(() => {
  if (!sortKey.value) return props.rows
  const key = sortKey.value
  const dir = sortDir.value === 'asc' ? 1 : -1
  return [...props.rows].sort((a, b) => {
    const av = a[key] ?? ''
    const bv = b[key] ?? ''
    if (av < bv) return -dir
    if (av > bv) return dir
    return 0
  })
})

const showSkeleton = computed(() => props.loading && props.rows.length === 0)
</script>

<template>
  <div class="dt-wrap" role="region" aria-label="数据表格">
    <div class="dt-scroll">
      <table class="dt">
        <!-- 表头 -->
        <thead class="dt__head">
          <tr>
            <th
              v-for="col in columns"
              :key="col.key"
              class="dt__th"
              :class="[
                `dt__th--${col.align ?? 'left'}`,
                { 'dt__th--sortable': col.sortable },
                { 'dt__th--sorted': sortKey === col.key },
              ]"
              :style="col.width ? { width: col.width } : undefined"
              :aria-sort="col.sortable
                ? (sortKey === col.key ? (sortDir === 'asc' ? 'ascending' : 'descending') : 'none')
                : undefined"
              @click="toggleSort(col)"
            >
              <span class="dt__th-inner">
                {{ col.label }}
                <template v-if="col.sortable">
                  <!-- 未排序 -->
                  <svg v-if="sortKey !== col.key" class="dt__sort-icon dt__sort-icon--idle"
                       width="12" height="12" viewBox="0 0 24 24" fill="none"
                       stroke="currentColor" stroke-width="2" stroke-linecap="round"
                       stroke-linejoin="round" aria-hidden="true">
                    <path d="M7 15l5 5 5-5M7 9l5-5 5 5"/>
                  </svg>
                  <!-- 升序 -->
                  <svg v-else-if="sortDir === 'asc'" class="dt__sort-icon dt__sort-icon--active"
                       width="12" height="12" viewBox="0 0 24 24" fill="none"
                       stroke="currentColor" stroke-width="2" stroke-linecap="round"
                       stroke-linejoin="round" aria-hidden="true">
                    <path d="M12 19V5M5 12l7-7 7 7"/>
                  </svg>
                  <!-- 降序 -->
                  <svg v-else class="dt__sort-icon dt__sort-icon--active"
                       width="12" height="12" viewBox="0 0 24 24" fill="none"
                       stroke="currentColor" stroke-width="2" stroke-linecap="round"
                       stroke-linejoin="round" aria-hidden="true">
                    <path d="M12 5v14M5 12l7 7 7-7"/>
                  </svg>
                </template>
              </span>
            </th>
          </tr>
        </thead>

        <!-- skeleton loading -->
        <tbody v-if="showSkeleton" class="dt__body" aria-label="正在加载数据">
          <tr v-for="i in skeletonRows" :key="i" class="dt__row dt__row--skeleton">
            <td v-for="col in columns" :key="col.key" class="dt__td">
              <Skeleton :height="'14px'" :width="col.align === 'right' ? '70%' : '80%'" />
            </td>
          </tr>
        </tbody>

        <!-- 错误状态 -->
        <tbody v-else-if="error" class="dt__body">
          <tr>
            <td :colspan="columns.length" class="dt__empty dt__empty--error">
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor"
                   stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                <circle cx="12" cy="12" r="10"/>
                <line x1="15" y1="9" x2="9" y2="15"/>
                <line x1="9" y1="9" x2="15" y2="15"/>
              </svg>
              <span>{{ error }}</span>
            </td>
          </tr>
        </tbody>

        <!-- 空状态 -->
        <tbody v-else-if="sortedRows.length === 0" class="dt__body">
          <tr>
            <td :colspan="columns.length" class="dt__empty">
              {{ emptyText }}
            </td>
          </tr>
        </tbody>

        <!-- 数据行 -->
        <tbody v-else class="dt__body">
          <tr
            v-for="row in sortedRows"
            :key="String(row[rowKey] ?? '')"
            class="dt__row"
          >
            <td
              v-for="col in columns"
              :key="col.key"
              class="dt__td"
              :class="`dt__td--${col.align ?? 'left'}`"
            >
              <slot :name="col.key" :row="row" :value="row[col.key]">
                {{ row[col.key] ?? '—' }}
              </slot>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>
</template>

<style scoped>
.dt-wrap {
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-lg);
  background: var(--color-surface);
  box-shadow: var(--shadow-sm);
  overflow: hidden;
}

.dt-scroll {
  overflow-x: auto;
}

.dt {
  width: 100%;
  border-collapse: collapse;
  font-size: var(--text-sm);
}

/* ---- 表头 ---- */
.dt__head {
  position: sticky;
  top: 0;
  z-index: var(--z-sticky);
  background: var(--color-surface);
}

.dt__th {
  height: 36px;
  padding-inline: var(--sp-3);
  font-size: var(--text-xs);
  font-weight: var(--weight-medium);
  color: var(--color-muted);
  text-align: left;
  white-space: nowrap;
  border-bottom: 1px solid var(--color-border-subtle);
  -webkit-user-select: none;
  user-select: none;
}

.dt__th--right  { text-align: right; }
.dt__th--center { text-align: center; }

.dt__th--sortable {
  cursor: pointer;
}

.dt__th--sortable:hover {
  color: var(--color-secondary);
}

.dt__th--sorted {
  color: var(--color-ink);
}

.dt__th-inner {
  display: inline-flex;
  align-items: center;
  gap: var(--sp-1);
}

.dt__sort-icon--idle {
  opacity: 0.4;
}

.dt__sort-icon--active {
  color: var(--color-accent);
}

/* ---- 数据行 ---- */
.dt__row {
  transition: background-color 80ms var(--ease-out);
}

.dt__row:hover {
  background: var(--color-hover-bg);
}

.dt__row:not(:last-child) .dt__td {
  border-bottom: 1px solid var(--color-border-subtle);
}

.dt__td {
  height: var(--row-h);
  padding-inline: var(--sp-3);
  color: var(--color-ink);
  vertical-align: middle;
}

.dt__td--right  { text-align: right; }
.dt__td--center { text-align: center; }

/* ---- skeleton 行 ---- */
.dt__row--skeleton .dt__td {
  vertical-align: middle;
}

/* ---- 空/错误状态 ---- */
.dt__empty {
  height: 80px;
  text-align: center;
  color: var(--color-muted);
  font-size: var(--text-sm);
}

.dt__empty--error {
  display: table-cell;
  color: var(--color-danger);
}

.dt__empty--error {
  vertical-align: middle;
}
</style>
