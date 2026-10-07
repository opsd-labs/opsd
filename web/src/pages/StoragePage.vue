<script setup lang="ts">
/**
 * StoragePage — 存储预检报告
 * GET /api/v1/storage/readiness → Report
 */
import { ref, onMounted } from 'vue'
import { apiGet } from '../api/client'
import { formatBytes, formatUnixTime } from '../utils/format'
import type { components } from '../api/generated'
import Badge from '../components/ui/Badge.vue'
import Button from '../components/ui/Button.vue'
import Skeleton from '../components/ui/Skeleton.vue'
import type { BadgeTone } from '../components/ui/Badge.vue'

type Report = components['schemas']['Report']
type Suitability = components['schemas']['Suitability']

function suitTone(s: Suitability): BadgeTone {
  if (s === 'suitable') return 'success'
  if (s === 'needs_work') return 'warning'
  if (s === 'unsuitable') return 'danger'
  return 'default' // pending
}

function suitLabel(s: Suitability): string {
  if (s === 'suitable') return '适合'
  if (s === 'needs_work') return '需调整'
  if (s === 'unsuitable') return '不适合'
  return '待采集'
}

const loading = ref(false)
const error = ref('')
const report = ref<Report | null>(null)

async function load() {
  loading.value = true
  error.value = ''
  try {
    report.value = await apiGet<Report>('/api/v1/storage/readiness')
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : '加载失败'
  } finally {
    loading.value = false
  }
}

onMounted(load)
</script>

<template>
  <div class="storage-page">
    <div class="storage-page__header">
      <h2 class="storage-page__title">存储预检</h2>
      <Button variant="ghost" size="sm" :busy="loading" @click="load">刷新</Button>
    </div>

    <!-- 骨架 -->
    <div v-if="loading && !report" class="storage-skeleton">
      <Skeleton height="60px" style="border-radius:var(--radius-lg)" />
      <Skeleton v-for="i in 3" :key="i" height="140px" style="border-radius:var(--radius-lg)" />
    </div>

    <!-- 错误 -->
    <p v-else-if="error" class="storage-error">{{ error }}</p>

    <template v-else-if="report">
      <!-- 总览横幅 -->
      <div class="storage-summary" :class="report.ready ? 'storage-summary--ready' : 'storage-summary--notready'">
        <div class="storage-summary__left">
          <Badge :tone="report.ready ? 'success' : 'warning'">{{ report.ready ? '可部署' : '未就绪' }}</Badge>
          <span class="storage-summary__text">{{ report.summary }}</span>
        </div>
        <span class="storage-summary__time">{{ formatUnixTime(report.generated_at) }} 更新</span>
      </div>

      <!-- 一致性检查 -->
      <div v-if="report.checks.length > 0" class="storage-section">
        <h3 class="storage-section__title">一致性检查</h3>
        <div class="storage-checks">
          <div
            v-for="(c, i) in report.checks"
            :key="i"
            class="storage-check"
            :class="c.passed ? 'storage-check--pass' : 'storage-check--fail'"
          >
            <Badge :tone="c.passed ? 'success' : 'danger'" class="storage-check__badge">
              {{ c.passed ? '通过' : '未通过' }}
            </Badge>
            <div class="storage-check__body">
              <span class="storage-check__name">{{ c.name }}</span>
              <span v-if="!c.passed" class="storage-check__detail">{{ c.detail }}</span>
            </div>
          </div>
        </div>
      </div>

      <!-- 推荐拓扑 -->
      <div v-if="report.topology" class="storage-section">
        <h3 class="storage-section__title">推荐拓扑</h3>
        <div class="storage-topo">
          <div class="storage-topo__item">
            <span class="storage-topo__label">节点数</span>
            <span class="storage-topo__value">{{ report.topology.nodes }}</span>
          </div>
          <div class="storage-topo__item">
            <span class="storage-topo__label">每节点盘数</span>
            <span class="storage-topo__value">{{ report.topology.drives_per_node }}</span>
          </div>
          <div class="storage-topo__item">
            <span class="storage-topo__label">最小盘容量</span>
            <span class="storage-topo__value">{{ formatBytes(report.topology.drive_size) }}</span>
          </div>
          <div class="storage-topo__item">
            <span class="storage-topo__label">原始容量</span>
            <span class="storage-topo__value">{{ formatBytes(report.topology.raw_capacity) }}</span>
          </div>
          <div class="storage-topo__item">
            <span class="storage-topo__label">可用容量</span>
            <span class="storage-topo__value">{{ formatBytes(report.topology.usable_capacity) }}</span>
          </div>
          <div class="storage-topo__item">
            <span class="storage-topo__label">冗余策略</span>
            <span class="storage-topo__value">{{ report.topology.parity_note }}</span>
          </div>
        </div>
      </div>

      <!-- 逐节点结论 -->
      <div class="storage-section">
        <h3 class="storage-section__title">节点状态（{{ report.nodes.length }} 台）</h3>
        <div class="storage-nodes">
          <div
            v-for="n in report.nodes"
            :key="n.node_id"
            class="storage-node-card"
          >
            <div class="storage-node-card__header">
              <span class="storage-node-card__name">{{ n.name }}</span>
              <div class="storage-node-card__badges">
                <Badge :tone="n.online ? 'success' : 'danger'">{{ n.online ? '在线' : '离线' }}</Badge>
                <Badge :tone="suitTone(n.suitability)">{{ suitLabel(n.suitability) }}</Badge>
              </div>
            </div>

            <div v-if="n.collected_at" class="storage-node-card__meta">
              最近采集：{{ formatUnixTime(n.collected_at) }}
            </div>
            <div v-else class="storage-node-card__meta storage-node-card__meta--muted">
              尚未采集
            </div>

            <!-- 结论原因 -->
            <ul v-if="n.reasons.length > 0" class="storage-node-card__reasons">
              <li v-for="(r, i) in n.reasons" :key="i">{{ r }}</li>
            </ul>

            <!-- 候选盘 -->
            <div v-if="n.candidates.length > 0" class="storage-node-card__sub">
              <span class="storage-node-card__sub-label">候选盘 ({{ n.candidates.length }})</span>
              <div class="storage-disks">
                <div v-for="(d, i) in n.candidates" :key="i" class="storage-disk">
                  <span class="storage-disk__name">{{ d.device }}</span>
                  <span class="storage-disk__size">{{ formatBytes(d.size) }}</span>
                  <Badge v-if="d.rotational" tone="default">HDD</Badge>
                  <Badge v-else tone="accent">SSD</Badge>
                </div>
              </div>
            </div>

            <!-- 被拒盘 -->
            <div v-if="n.rejected.length > 0" class="storage-node-card__sub">
              <span class="storage-node-card__sub-label">被拒盘 ({{ n.rejected.length }})</span>
              <div class="storage-disks">
                <div v-for="(d, i) in n.rejected" :key="i" class="storage-disk storage-disk--rejected">
                  <span class="storage-disk__name">{{ d.device }}</span>
                  <span class="storage-disk__reason">{{ d.reason }}</span>
                </div>
              </div>
            </div>

            <!-- 采集缺口 -->
            <div v-if="n.gaps.length > 0" class="storage-node-card__gaps">
              <span class="storage-node-card__sub-label">采集缺口：</span>
              <span v-for="(g, i) in n.gaps" :key="i" class="storage-gap-tag">{{ g }}</span>
            </div>
          </div>
        </div>
      </div>
    </template>

    <div v-else class="storage-empty">暂无预检数据</div>
  </div>
</template>

<style scoped>
.storage-page { display: flex; flex-direction: column; gap: var(--sp-5); }

.storage-page__header {
  display: flex; align-items: center; justify-content: space-between;
}
.storage-page__title { font-size: var(--text-xl); font-weight: var(--weight-semibold); color: var(--color-ink); }

.storage-skeleton { display: flex; flex-direction: column; gap: var(--sp-3); }
.storage-error { font-size: var(--text-sm); color: var(--color-danger); }
.storage-empty { color: var(--color-muted); font-size: var(--text-sm); padding: var(--sp-8); text-align: center; }

/* ---- 总览横幅 ---- */
.storage-summary {
  display: flex; align-items: center; justify-content: space-between;
  gap: var(--sp-4); padding: var(--sp-3) var(--sp-4);
  border-radius: var(--radius-lg);
  border: 1px solid var(--color-border-subtle);
  background: var(--color-surface);
}
.storage-summary--ready { border-color: var(--color-success-border, var(--color-border-subtle)); }
.storage-summary--notready { border-color: var(--color-warning-border, var(--color-warning)); }
.storage-summary__left { display: flex; align-items: center; gap: var(--sp-3); flex-wrap: wrap; }
.storage-summary__text { font-size: var(--text-sm); color: var(--color-secondary); }
.storage-summary__time { font-size: var(--text-xs); color: var(--color-muted); white-space: nowrap; }

/* ---- 区块 ---- */
.storage-section { display: flex; flex-direction: column; gap: var(--sp-3); }
.storage-section__title { font-size: var(--text-md); font-weight: var(--weight-semibold); color: var(--color-ink); }

/* ---- 检查项 ---- */
.storage-checks { display: flex; flex-direction: column; gap: var(--sp-2); }
.storage-check {
  display: flex; align-items: flex-start; gap: var(--sp-3);
  padding: var(--sp-2) var(--sp-3);
  background: var(--color-surface);
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-md);
}
.storage-check__badge { flex-shrink: 0; margin-top: 1px; }
.storage-check__body { display: flex; flex-direction: column; gap: 2px; }
.storage-check__name { font-size: var(--text-sm); font-weight: var(--weight-medium); color: var(--color-ink); }
.storage-check__detail { font-size: var(--text-xs); color: var(--color-muted); }

/* ---- 推荐拓扑 ---- */
.storage-topo {
  display: grid; grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
  gap: var(--sp-3);
}
.storage-topo__item {
  display: flex; flex-direction: column; gap: var(--sp-1);
  padding: var(--sp-3) var(--sp-4);
  background: var(--color-surface);
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-md);
}
.storage-topo__label { font-size: var(--text-xs); color: var(--color-muted); }
.storage-topo__value { font-size: var(--text-lg); font-weight: var(--weight-semibold); color: var(--color-ink); font-family: var(--font-mono); }

/* ---- 节点卡片 ---- */
.storage-nodes { display: grid; grid-template-columns: repeat(auto-fill, minmax(320px, 1fr)); gap: var(--sp-4); }
.storage-node-card {
  display: flex; flex-direction: column; gap: var(--sp-3);
  padding: var(--sp-4);
  background: var(--color-surface);
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-sm);
}
.storage-node-card__header {
  display: flex; align-items: flex-start; justify-content: space-between; gap: var(--sp-3); flex-wrap: wrap;
}
.storage-node-card__name { font-size: var(--text-md); font-weight: var(--weight-semibold); color: var(--color-ink); }
.storage-node-card__badges { display: flex; gap: var(--sp-1); flex-wrap: wrap; }
.storage-node-card__meta { font-size: var(--text-xs); color: var(--color-secondary); }
.storage-node-card__meta--muted { color: var(--color-muted); font-style: italic; }

.storage-node-card__reasons {
  margin: 0; padding: 0 0 0 var(--sp-4);
  list-style: disc; font-size: var(--text-sm); color: var(--color-secondary);
  display: flex; flex-direction: column; gap: var(--sp-1);
}

.storage-node-card__sub { display: flex; flex-direction: column; gap: var(--sp-2); }
.storage-node-card__sub-label { font-size: var(--text-xs); color: var(--color-muted); font-weight: var(--weight-medium); }

/* ---- 盘 ---- */
.storage-disks { display: flex; flex-direction: column; gap: var(--sp-1); }
.storage-disk {
  display: flex; align-items: center; gap: var(--sp-2);
  font-size: var(--text-xs); font-family: var(--font-mono);
}
.storage-disk--rejected { color: var(--color-muted); }
.storage-disk__name { color: var(--color-ink); }
.storage-disk__size { color: var(--color-secondary); }
.storage-disk__reason { color: var(--color-muted); font-style: italic; font-family: var(--font-base, sans-serif); }

/* ---- 缺口 ---- */
.storage-node-card__gaps { display: flex; flex-wrap: wrap; align-items: center; gap: var(--sp-1); }
.storage-gap-tag {
  font-size: var(--text-xs); padding: 1px 6px;
  background: var(--color-hover-bg); border-radius: var(--radius-sm);
  color: var(--color-secondary);
}
</style>
