/**
 * opsd web3 — 主题切换可组合 API
 */

import { ref, watch } from 'vue'

export type Theme = 'light' | 'dark' | 'system'

const STORAGE_KEY = 'opsd3.theme'

function getSystemTheme(): 'light' | 'dark' {
  return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'
}

function applyTheme(resolved: 'light' | 'dark'): void {
  document.documentElement.setAttribute('data-theme', resolved)
}

// 单例状态
const preference = ref<Theme>(
  (localStorage.getItem(STORAGE_KEY) as Theme | null) ?? 'system'
)

const resolved = ref<'light' | 'dark'>(
  preference.value === 'system' ? getSystemTheme() : preference.value
)

// 监听系统主题变化
const mq = window.matchMedia('(prefers-color-scheme: dark)')
mq.addEventListener('change', () => {
  if (preference.value === 'system') {
    resolved.value = getSystemTheme()
    applyTheme(resolved.value)
  }
})

watch(preference, (val) => {
  localStorage.setItem(STORAGE_KEY, val)
  resolved.value = val === 'system' ? getSystemTheme() : val
  applyTheme(resolved.value)
})

export function useTheme() {
  return {
    preference,
    resolved,
    setTheme(t: Theme) {
      preference.value = t
    },
    toggleTheme() {
      preference.value = resolved.value === 'light' ? 'dark' : 'light'
    },
  }
}
