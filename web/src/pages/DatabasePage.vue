<script setup lang="ts">
/**
 * DatabasePage — 跨节点数据库只读巡检总览
 * GET /api/v1/database/overview → Overview
 */
import { ref, onMounted } from 'vue'
import { apiGet } from '../api/client'
import { formatUnixTime } from '../utils/format'
import type { components } from '../api/generated'
import Badge from '../components/ui/Badge.vue'
import Button from '../components/ui/Button.vue'
import Skeleton from '../components/ui/Skeleton.vue'
import type { BadgeTone } from '../components/ui/Badge.vue'

type Overview = components['schemas']['Overview']
type Tone = components['schemas']['Tone']

function toneToBadge(tone: Tone): BadgeTone {
  if (tone === 'ok') return 'success'
  if (tone === 'warning') return 'warning'
  if (tone === 'critical') return 'danger'
  return 'default'
}

function toneLabel(tone: Tone): string {
  if (tone === 'ok') return '正常'
  if (tone === 'warning') return '警告'
  if (tone === 'critical') return '严重'
  return tone
}

const loading = ref(false)
const error = ref('')
const overview = ref<Overview | null>(null)

async function load() {
  loading.value = true
  error.value = ''
  try {
    overview.value = await apiGet<Overview>('/api/v1/database/overview')
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : '加载失败'
  } finally {
    loading.value = false
  }
}

onMounted(load)
</script>

<template>
  <div class="db-page">
    <div class="db-page__header">
      <h2 class="db-page__title">数据库巡检</h2>
      <Button variant="ghost" size="sm" :busy="loading" @click="load">刷新</Button>
    </div>

    <!-- 骨架 -->
    <div v-if="loading && !overview" class="db-skeleton">
      <Skeleton height="60px" style="border-radius:var(--radius-lg)" />
      <Skeleton v-for="i in 3" :key="i" height="120px" style="border-radius:var(--radius-lg)" />
    </div>

    <!-- 错误 -->
    <p v-else-if="error" class="db-error">{{ error }}</p>

    <template v-else-if="overview">
      <!-- 总览横幅 -->
      <div class="db-summary" :class="`db-summary--${overview.tone}`">
        <div class="db-summary__left">
          <Badge :tone="toneToBadge(overview.tone)">{{ toneLabel(overview.tone) }}</Badge>
          <span class="db-summary__text">{{ overview.summary }}</span>
        </div>
        <span class="db-summary__time">{{ formatUnixTime(overview.generated_at) }} 更新</span>
      </div>

      <!-- 跨节点检查项 -->
      <div v-if="overview.checks.length > 0" class="db-section">
        <h3 class="db-section__title">一致性检查</h3>
        <div class="db-findings">
          <div
            v-for="(f, i) in overview.checks"
            :key="i"
            class="db-finding"
            :class="`db-finding--${f.tone}`"
          >
            <Badge :tone="toneToBadge(f.tone)" class="db-finding__badge">{{ toneLabel(f.tone) }}</Badge>
            <div class="db-finding__body">
              <span class="db-finding__title">{{ f.title }}</span>
              <span class="db-finding__detail">{{ f.detail }}</span>
            </div>
          </div>
        </div>
      </div>

      <!-- 逐节点结论 -->
      <div class="db-section">
        <h3 class="db-section__title">节点状态（{{ overview.nodes.length }} 台）</h3>
        <div class="db-nodes">
          <div
            v-for="n in overview.nodes"
            :key="n.node_id"
            class="db-node-card"
          >
            <div class="db-node-card__header">
              <span class="db-node-card__name">{{ n.name }}</span>
              <div class="db-node-card__badges">
                <Badge :tone="n.online ? 'success' : 'danger'">{{ n.online ? '在线' : '离线' }}</Badge>
                <Badge :tone="toneToBadge(n.tone)">{{ toneLabel(n.tone) }}</Badge>
                <Badge v-if="!n.configured" tone="warning">未配置账号</Badge>
              </div>
            </div>

            <div v-if="n.collected_at" class="db-node-card__meta">
              最近巡检：{{ formatUnixTime(n.collected_at) }}
            </div>
            <div v-else class="db-node-card__meta db-node-card__meta--muted">
              从未巡检
            </div>

            <!-- 问题列表 -->
            <div v-if="n.findings.length > 0" class="db-node-card__findings">
              <div
                v-for="(f, i) in n.findings"
                :key="i"
                class="db-finding db-finding--sm"
                :class="`db-finding--${f.tone}`"
              >
                <Badge :tone="toneToBadge(f.tone)" class="db-finding__badge">{{ toneLabel(f.tone) }}</Badge>
                <div class="db-finding__body">
                  <span class="db-finding__title">{{ f.title }}</span>
                  <span class="db-finding__detail">{{ f.detail }}</span>
                </div>
              </div>
            </div>

            <!-- 未知项 -->
            <div v-if="n.unknown.length > 0" class="db-node-card__unknowns">
              <span class="db-node-card__unknowns-label">未知项：</span>
              <span v-for="(u, i) in n.unknown" :key="i" class="db-unknown-tag">{{ u }}</span>
            </div>
          </div>
        </div>
      </div>
    </template>

    <div v-else class="db-empty">暂无巡检数据</div>
  </div>
</template>

<style scoped>
.db-page { display: flex; flex-direction: column; gap: var(--sp-5); }

.db-page__header {
  display: flex; align-items: center; justify-content: space-between;
}
.db-page__title { font-size: var(--text-xl); font-weight: var(--weight-semibold); color: var(--color-ink); }

.db-skeleton { display: flex; flex-direction: column; gap: var(--sp-3); }
.db-error { font-size: var(--text-sm); color: var(--color-danger); }
.db-empty { color: var(--color-muted); font-size: var(--text-sm); padding: var(--sp-8); text-align: center; }

/* ---- 总览横幅 ---- */
.db-summary {
  display: flex; align-items: center; justify-content: space-between;
  gap: var(--sp-4); padding: var(--sp-3) var(--sp-4);
  border-radius: var(--radius-lg);
  border: 1px solid var(--color-border-subtle);
  background: var(--color-surface);
}
.db-summary--ok { border-color: var(--color-success-border, var(--color-border-subtle)); }
.db-summary--warning { border-color: var(--color-warning-border, var(--color-warning)); }
.db-summary--critical { border-color: var(--color-danger-border, var(--color-danger)); }
.db-summary__left { display: flex; align-items: center; gap: var(--sp-3); flex-wrap: wrap; }
.db-summary__text { font-size: var(--text-sm); color: var(--color-secondary); }
.db-summary__time { font-size: var(--text-xs); color: var(--color-muted); white-space: nowrap; }

/* ---- 区块 ---- */
.db-section { display: flex; flex-direction: column; gap: var(--sp-3); }
.db-section__title { font-size: var(--text-md); font-weight: var(--weight-semibold); color: var(--color-ink); }

/* ---- 检查项 / 问题 ---- */
.db-findings { display: flex; flex-direction: column; gap: var(--sp-2); }
.db-finding {
  display: flex; align-items: flex-start; gap: var(--sp-3);
  padding: var(--sp-3) var(--sp-4);
  background: var(--color-surface);
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-md);
}
.db-finding--sm { padding: var(--sp-2) var(--sp-3); }
.db-finding__badge { flex-shrink: 0; margin-top: 1px; }
.db-finding__body { display: flex; flex-direction: column; gap: var(--sp-1); min-width: 0; }
.db-finding__title { font-size: var(--text-sm); font-weight: var(--weight-medium); color: var(--color-ink); }
.db-finding__detail { font-size: var(--text-xs); color: var(--color-muted); }

/* ---- 节点卡片 ---- */
.db-nodes { display: grid; grid-template-columns: repeat(auto-fill, minmax(320px, 1fr)); gap: var(--sp-4); }
.db-node-card {
  display: flex; flex-direction: column; gap: var(--sp-3);
  padding: var(--sp-4);
  background: var(--color-surface);
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-sm);
}
.db-node-card__header {
  display: flex; align-items: flex-start; justify-content: space-between; gap: var(--sp-3); flex-wrap: wrap;
}
.db-node-card__name { font-size: var(--text-md); font-weight: var(--weight-semibold); color: var(--color-ink); }
.db-node-card__badges { display: flex; gap: var(--sp-1); flex-wrap: wrap; }
.db-node-card__meta { font-size: var(--text-xs); color: var(--color-secondary); }
.db-node-card__meta--muted { color: var(--color-muted); font-style: italic; }

.db-node-card__findings { display: flex; flex-direction: column; gap: var(--sp-2); }

.db-node-card__unknowns { display: flex; flex-wrap: wrap; align-items: center; gap: var(--sp-1); }
.db-node-card__unknowns-label { font-size: var(--text-xs); color: var(--color-muted); flex-shrink: 0; }
.db-unknown-tag {
  font-size: var(--text-xs); padding: 1px 6px;
  background: var(--color-hover-bg); border-radius: var(--radius-sm);
  color: var(--color-secondary);
}
</style>
