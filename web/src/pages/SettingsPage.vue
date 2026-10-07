<script setup lang="ts">
/**
 * SettingsPage — 设置页
 * 安全入口 / 主题 / 分享令牌 / API Key / 审计日志
 */
import { ref } from 'vue'
import { useTheme } from '../composables/useTheme'

type SettingsTab = 'general' | 'share' | 'audit'

const tabs: { key: SettingsTab; label: string }[] = [
  { key: 'general', label: '通用' },
  { key: 'share',   label: '分享' },
  { key: 'audit',   label: '审计日志' },
]

const active = ref<SettingsTab>('general')
const { preference, setTheme } = useTheme()
</script>

<template>
  <div class="settings-page">
    <!-- Tab 导航 -->
    <div class="settings-tabs" role="tablist">
      <button
        v-for="t in tabs"
        :key="t.key"
        class="settings-tab"
        :class="{ 'settings-tab--active': active === t.key }"
        role="tab"
        :aria-selected="active === t.key"
        @click="active = t.key"
      >
        {{ t.label }}
      </button>
    </div>

    <!-- 通用设置 -->
    <div v-if="active === 'general'" class="settings-section">
      <div class="settings-group">
        <h3 class="settings-group__title">外观</h3>
        <div class="settings-row">
          <span class="settings-row__label">主题</span>
          <div class="theme-options">
            <button
              v-for="opt in (['light', 'dark', 'system'] as const)"
              :key="opt"
              class="theme-btn"
              :class="{ 'theme-btn--active': preference === opt }"
              @click="setTheme(opt)"
            >
              {{ opt === 'light' ? '浅色' : opt === 'dark' ? '深色' : '跟随系统' }}
            </button>
          </div>
        </div>
      </div>

      <div class="settings-group">
        <h3 class="settings-group__title">安全入口</h3>
        <p class="settings-group__desc">安全入口是所有管理接口的路径前缀，变更后当前会话失效。</p>
        <div class="settings-placeholder">安全入口管理（实现中）</div>
      </div>
    </div>

    <!-- 分享 -->
    <div v-else-if="active === 'share'" class="settings-section">
      <div class="settings-placeholder">分享令牌 / API Key 管理（实现中）</div>
    </div>

    <!-- 审计日志 -->
    <div v-else-if="active === 'audit'" class="settings-section">
      <div class="settings-placeholder">审计日志（实现中）</div>
    </div>
  </div>
</template>

<style scoped>
.settings-page { display: flex; flex-direction: column; gap: var(--sp-5); max-width: var(--narrow-max); }

.settings-tabs {
  display: flex; gap: 0;
  border-bottom: 1px solid var(--color-border-subtle);
}
.settings-tab {
  padding: var(--sp-2) var(--sp-4);
  font-size: var(--text-sm); font-weight: var(--weight-medium);
  color: var(--color-muted); background: transparent;
  border: none; border-bottom: 2px solid transparent;
  cursor: pointer; margin-bottom: -1px;
  transition: color var(--duration-fast), border-color var(--duration-fast);
}
.settings-tab:hover { color: var(--color-ink); }
.settings-tab--active { color: var(--color-accent); border-bottom-color: var(--color-accent); }
.settings-tab:focus-visible { outline: none; box-shadow: var(--shadow-focus); border-radius: var(--radius-sm); }

.settings-section { display: flex; flex-direction: column; gap: var(--sp-6); }

.settings-group { display: flex; flex-direction: column; gap: var(--sp-3); }
.settings-group__title { font-size: var(--text-md); font-weight: var(--weight-semibold); color: var(--color-ink); }
.settings-group__desc { font-size: var(--text-sm); color: var(--color-muted); }

.settings-row {
  display: flex; align-items: center;
  justify-content: space-between; gap: var(--sp-4);
}
.settings-row__label { font-size: var(--text-sm); color: var(--color-secondary); }

.theme-options { display: flex; gap: var(--sp-1); }
.theme-btn {
  padding: var(--sp-1) var(--sp-3); font-size: var(--text-sm);
  background: var(--color-surface); color: var(--color-secondary);
  border: 1px solid var(--color-border); border-radius: var(--radius-md);
  cursor: pointer;
  transition: background-color var(--duration-fast), color var(--duration-fast), border-color var(--duration-fast);
}
.theme-btn:hover { background: var(--color-hover-bg); color: var(--color-ink); }
.theme-btn--active {
  background: var(--color-accent-subtle); color: var(--color-accent);
  border-color: var(--color-accent-muted);
}
.theme-btn:focus-visible { outline: none; box-shadow: var(--shadow-focus); }

.settings-placeholder { color: var(--color-muted); font-size: var(--text-sm); padding: var(--sp-6); text-align: center; }
</style>
