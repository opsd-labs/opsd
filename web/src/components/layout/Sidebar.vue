<script setup lang="ts">
/**
 * Sidebar — 侧边导航栏
 * 包含品牌区、导航项、折叠按钮、登出
 */
import { computed } from 'vue'
import { workspace, PAGES } from '../../state/workspace'
import type { PageName } from '../../state/workspace'

const collapsed = computed(() => workspace.state.sidebarCollapsed)
const page = computed(() => workspace.state.page)

function navigate(name: PageName) {
  workspace.navigate(name)
}

// Lucide SVG path 数据（精简，避免全量导入增加体积）
const navIcons: Record<string, string> = {
  console:  'M3 9l9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z M9 22V12h6v10',
  nodes:    'M20 17a2 2 0 0 0 2-2V4a2 2 0 0 0-2-2H9.5a2 2 0 0 0-2 2v1 M15 22H3a2 2 0 0 1-2-2v-9a2 2 0 0 1 2-2h12a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2z',
  docker:   'M22 12.5c-1.2.6-3.4.6-5-.2-1.6-.8-1.6-2.3.2-3.3 1.7-1 4.3-1 5.8.2 M13 2.3c0 0-3 0.2-5 2 M3 7.5h18 M3 12h18 M9 2v5 M13 2v5 M5 7v10c0 1 .5 2 2 2h10c1.5 0 2-1 2-2V7',
  firewall: 'M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z',
  database: 'M12 2a9 3 0 0 0-9 3v14a9 3 0 0 0 18 0V5a9 3 0 0 0-9-3z M3 5a9 3 0 0 0 18 0 M3 12a9 3 0 0 0 18 0',
  storage:  'M22 12H2 M5.45 5.11L2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z M6 16h.01 M10 16h.01',
  settings: 'M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6z M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z',
}
</script>

<template>
  <aside class="sidebar" :class="{ 'sidebar--collapsed': collapsed }" aria-label="主导航">
    <!-- 品牌区 -->
    <div class="sidebar__brand">
      <div class="brand-mark" aria-hidden="true">
        <!-- Shield icon -->
        <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor"
             stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" />
        </svg>
      </div>
      <span class="brand-name">opsd</span>
    </div>

    <!-- 导航区 -->
    <nav class="sidebar__nav">
      <button
        v-for="p in PAGES"
        :key="p.name"
        class="nav-item"
        :class="{ 'nav-item--active': page === p.name }"
        :aria-current="page === p.name ? 'page' : undefined"
        :title="collapsed ? p.label : undefined"
        @click="navigate(p.name)"
      >
        <svg class="nav-item__icon" width="16" height="16" viewBox="0 0 24 24"
             fill="none" stroke="currentColor" stroke-width="1.75"
             stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <path :d="navIcons[p.name]" />
        </svg>
        <span class="nav-item__label">{{ p.label }}</span>
      </button>
    </nav>

    <!-- 底部操作区 -->
    <div class="sidebar__footer">
      <!-- 折叠按钮 -->
      <button
        class="footer-btn"
        :title="collapsed ? '展开侧栏' : '折叠侧栏'"
        :aria-label="collapsed ? '展开侧栏' : '折叠侧栏'"
        @click="workspace.toggleSidebar()"
      >
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor"
             stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <!-- chevron-left / chevron-right 切换 -->
          <path v-if="!collapsed" d="M15 18l-6-6 6-6" />
          <path v-else d="M9 18l6-6-6-6" />
        </svg>
        <span class="nav-item__label">折叠</span>
      </button>

      <!-- 登出 -->
      <button
        class="footer-btn footer-btn--logout"
        title="退出登录"
        aria-label="退出登录"
        @click="workspace.logout()"
      >
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor"
             stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4 M16 17l5-5-5-5 M21 12H9" />
        </svg>
        <span class="nav-item__label">退出</span>
      </button>
    </div>
  </aside>
</template>

<style scoped>
.sidebar {
  display: flex;
  flex-direction: column;
  width: var(--sidebar-w);
  height: 100dvh;
  position: sticky;
  top: 0;
  background: var(--color-surface);
  border-right: 1px solid var(--color-border-subtle);
  overflow: hidden;
  transition: width var(--duration-base) var(--ease-out);
}

/* 折叠宽度通过父级 grid 控制，sidebar 自身 width: 100% 跟随 */
.sidebar--collapsed {
  /* 折叠时内容宽度对齐图标宽度 */
  width: var(--sidebar-w-collapsed);
}

/* ---- 品牌区 ---- */
.sidebar__brand {
  display: flex;
  align-items: center;
  gap: var(--sp-3);
  height: var(--topbar-h);
  padding-inline: var(--sp-4);
  border-bottom: 1px solid var(--color-border-subtle);
  flex-shrink: 0;
  overflow: hidden;
}

.brand-mark {
  flex-shrink: 0;
  color: var(--color-accent);
  display: flex;
  align-items: center;
}

.brand-name {
  font-size: var(--text-lg);
  font-weight: var(--weight-bold);
  color: var(--color-ink);
  letter-spacing: var(--tracking-tight);
  white-space: nowrap;
  /* 折叠时淡出 */
  transition:
    opacity var(--duration-fast) var(--ease-out),
    max-width var(--duration-base) var(--ease-out);
  max-width: 120px;
  overflow: hidden;
}

/* ---- 导航 ---- */
.sidebar__nav {
  flex: 1;
  overflow-y: auto;
  overflow-x: hidden;
  padding: var(--sp-2) var(--sp-2);
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.nav-item {
  display: flex;
  align-items: center;
  gap: var(--sp-3);
  height: 36px;
  padding-inline: var(--sp-3);
  border: none;
  border-radius: var(--radius-md);
  background: transparent;
  color: var(--color-secondary);
  cursor: pointer;
  font-size: var(--text-sm);
  font-weight: var(--weight-normal);
  text-align: left;
  white-space: nowrap;
  transition:
    background-color var(--duration-fast) var(--ease-out),
    color var(--duration-fast) var(--ease-out);
}

.nav-item:hover {
  background: var(--color-hover-bg);
  color: var(--color-ink);
}

.nav-item--active {
  background: var(--color-accent-subtle);
  color: var(--color-accent);
  font-weight: var(--weight-medium);
}

.nav-item:focus-visible {
  outline: none;
  box-shadow: var(--shadow-focus);
}

.nav-item__icon {
  flex-shrink: 0;
}

.nav-item__label {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  transition:
    opacity var(--duration-fast) var(--ease-out),
    max-width var(--duration-base) var(--ease-out);
  max-width: 140px;
}

/* ---- 折叠时隐藏 label ---- */
.sidebar--collapsed .nav-item__label,
.sidebar--collapsed .brand-name {
  max-width: 0;
  opacity: 0;
  pointer-events: none;
}

.sidebar--collapsed .nav-item {
  justify-content: center;
  padding-inline: 0;
}

.sidebar--collapsed .sidebar__brand {
  justify-content: center;
  padding-inline: 0;
}

/* ---- 底部 ---- */
.sidebar__footer {
  padding: var(--sp-2);
  border-top: 1px solid var(--color-border-subtle);
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.footer-btn {
  display: flex;
  align-items: center;
  gap: var(--sp-3);
  height: 32px;
  padding-inline: var(--sp-3);
  border: none;
  border-radius: var(--radius-md);
  background: transparent;
  color: var(--color-muted);
  cursor: pointer;
  font-size: var(--text-sm);
  transition:
    background-color var(--duration-fast) var(--ease-out),
    color var(--duration-fast) var(--ease-out);
  white-space: nowrap;
  overflow: hidden;
}

.footer-btn:hover {
  background: var(--color-hover-bg);
  color: var(--color-secondary);
}

.footer-btn--logout:hover {
  color: var(--color-danger);
}

.footer-btn:focus-visible {
  outline: none;
  box-shadow: var(--shadow-focus);
}

.sidebar--collapsed .footer-btn {
  justify-content: center;
  padding-inline: 0;
}
</style>
