<script setup lang="ts">
/**
 * LoginPage — 登录页
 * 桌面端双栏：左侧品牌区（深蓝渐变） + 右侧表单区
 * 移动端：单列，表单居中带卡片阴影
 */
import { ref } from 'vue'
import { workspace } from '../state/workspace'
import { ApiError } from '../api/client'

const password = ref('')
const busy = ref(false)
const error = ref<string | null>(null)

async function onSubmit() {
  if (!password.value.trim()) {
    error.value = '请输入密码'
    return
  }

  busy.value = true
  error.value = null

  try {
    await workspace.login(password.value)
  } catch (e) {
    if (e instanceof ApiError) {
      if (e.status === 429) {
        error.value = '登录尝试过于频繁，请稍后再试'
      } else if (e.status === 401 || e.status === 403) {
        error.value = '密码不正确'
      } else {
        error.value = e.message || '登录失败，请检查网络连接'
      }
    } else {
      error.value = '登录失败，请检查网络连接'
    }
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <div class="login-page">
    <!-- 左侧品牌区（桌面端） -->
    <div class="login-brand" aria-hidden="true">
      <div class="brand-content">
        <!-- Shield 图标 -->
        <svg class="brand-icon" width="72" height="72" viewBox="0 0 24 24"
             fill="none" stroke="currentColor" stroke-width="1.5"
             stroke-linecap="round" stroke-linejoin="round">
          <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" />
        </svg>
        <h1 class="brand-name">opsd</h1>
        <p class="brand-tagline">自托管多节点管理控制台</p>
      </div>
    </div>

    <!-- 右侧表单区 -->
    <div class="login-form-side">
      <form class="login-form" @submit.prevent="onSubmit" novalidate>
        <div class="login-form__header">
          <!-- 移动端品牌图标（桌面端隐藏） -->
          <div class="login-form__mobile-brand" aria-hidden="true">
            <svg width="36" height="36" viewBox="0 0 24 24"
                 fill="none" stroke="currentColor" stroke-width="1.75"
                 stroke-linecap="round" stroke-linejoin="round">
              <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" />
            </svg>
          </div>
          <h2 class="login-form__title">管理员登录</h2>
        </div>

        <!-- 错误提示 -->
        <div v-if="error" class="login-error" role="alert">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor"
               stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <circle cx="12" cy="12" r="10"/>
            <line x1="15" y1="9" x2="9" y2="15"/>
            <line x1="9" y1="9" x2="15" y2="15"/>
          </svg>
          <span>{{ error }}</span>
        </div>

        <!-- 密码输入 -->
        <div class="login-field">
          <label class="login-field__label" for="password">密码</label>
          <input
            id="password"
            v-model="password"
            type="password"
            class="login-field__input"
            :class="{ 'login-field__input--error': !!error }"
            placeholder="输入管理员密码"
            autocomplete="current-password"
            :disabled="busy"
            :aria-invalid="!!error"
            autofocus
            @input="error = null"
          />
        </div>

        <!-- 提交按钮 -->
        <button
          type="submit"
          class="login-submit"
          :disabled="busy"
          :aria-busy="busy"
        >
          <span v-if="busy" class="login-submit__spinner" aria-hidden="true"></span>
          <span :style="{ opacity: busy ? 0 : 1 }">
            {{ busy ? '正在登录…' : '登录' }}
          </span>
        </button>
      </form>
    </div>
  </div>
</template>

<style scoped>
/* ---- 整体布局 ---- */
.login-page {
  min-height: 100dvh;
  display: grid;
  grid-template-columns: 1fr 1fr;
}

/* ---- 左侧品牌区 ---- */
.login-brand {
  display: flex;
  align-items: center;
  justify-content: center;
  background: linear-gradient(
    135deg,
    oklch(18% 0.045 235) 0%,
    oklch(26% 0.060 235) 50%,
    oklch(22% 0.050 240) 100%
  );
  padding: var(--sp-12);
}

.brand-content {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--sp-4);
  text-align: center;
}

.brand-icon {
  color: oklch(92% 0.010 235);
  opacity: 0.95;
}

.brand-name {
  font-size: var(--text-3xl);
  font-weight: var(--weight-bold);
  color: oklch(97% 0.005 235);
  letter-spacing: var(--tracking-tight);
  line-height: var(--leading-tight);
}

.brand-tagline {
  font-size: var(--text-sm);
  color: oklch(80% 0.012 235);
  letter-spacing: var(--tracking-wide);
}

/* ---- 右侧表单区 ---- */
.login-form-side {
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--color-surface);
  padding: var(--sp-8) var(--sp-6);
}

.login-form {
  width: 100%;
  max-width: 360px;
  display: flex;
  flex-direction: column;
  gap: var(--sp-5);
}

.login-form__header {
  display: flex;
  flex-direction: column;
  gap: var(--sp-3);
}

/* 移动端品牌图标（桌面端隐藏） */
.login-form__mobile-brand {
  display: none;
  color: var(--color-accent);
}

.login-form__title {
  font-size: var(--text-xl);
  font-weight: var(--weight-semibold);
  color: var(--color-ink);
  line-height: var(--leading-tight);
}

/* ---- 错误提示 ---- */
.login-error {
  display: flex;
  align-items: flex-start;
  gap: var(--sp-2);
  padding: var(--sp-3) var(--sp-3);
  background: var(--color-danger-bg);
  color: var(--color-danger);
  border-radius: var(--radius-md);
  font-size: var(--text-sm);
  line-height: var(--leading-snug);
}

.login-error svg {
  flex-shrink: 0;
  margin-top: 1px;
}

/* ---- 表单字段 ---- */
.login-field {
  display: flex;
  flex-direction: column;
  gap: var(--sp-2);
}

.login-field__label {
  font-size: var(--text-sm);
  font-weight: var(--weight-medium);
  color: var(--color-secondary);
}

.login-field__input {
  width: 100%;
  height: var(--control-lg);
  padding-inline: var(--sp-4);
  background: var(--color-bg);
  color: var(--color-ink);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  font-size: var(--text-md);
  font-family: inherit;
  outline: none;
  transition:
    border-color var(--duration-fast) var(--ease-out),
    box-shadow var(--duration-fast) var(--ease-out);
}

.login-field__input::placeholder {
  color: var(--color-placeholder);
}

.login-field__input:focus-visible {
  border-color: var(--color-accent);
  box-shadow: var(--shadow-focus);
  background: var(--color-surface);
}

.login-field__input--error {
  border-color: var(--color-danger);
}

/* ---- 提交按钮 ---- */
.login-submit {
  position: relative;
  width: 100%;
  height: var(--control-lg);
  background: var(--color-accent);
  color: var(--color-on-accent);
  border: none;
  border-radius: var(--radius-md);
  font-size: var(--text-md);
  font-weight: var(--weight-medium);
  font-family: inherit;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  transition:
    background-color var(--duration-fast) var(--ease-out),
    transform 60ms var(--ease-out);
}

.login-submit:hover:not(:disabled) {
  background: var(--color-accent-hover);
}

.login-submit:active:not(:disabled) {
  transform: scale(0.99);
  filter: brightness(0.95);
}

.login-submit:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.login-submit:focus-visible {
  outline: none;
  box-shadow: var(--shadow-focus);
}

.login-submit__spinner {
  position: absolute;
  width: 18px;
  height: 18px;
  border: 2px solid oklch(80% 0.05 235);
  border-top-color: white;
  border-radius: var(--radius-full);
  animation: login-spin 600ms linear infinite;
}

@keyframes login-spin {
  to { transform: rotate(360deg); }
}

/* ---- 响应式 ---- */

/* 中等屏幕：缩减品牌区占比 */
@media (width < 1100px) {
  .login-page {
    grid-template-columns: 5fr 7fr;
  }
}

/* 小屏幕：隐藏品牌区，表单居中全屏 */
@media (width < 700px) {
  .login-page {
    grid-template-columns: 1fr;
    background: var(--color-bg);
  }

  .login-brand {
    display: none;
  }

  .login-form-side {
    background: transparent;
    align-items: flex-start;
    padding-top: var(--sp-16);
  }

  .login-form {
    background: var(--color-surface);
    padding: var(--sp-8) var(--sp-6);
    border-radius: var(--radius-xl);
    box-shadow: var(--shadow-sm);
    border: 1px solid var(--color-border-subtle);
  }

  .login-form__mobile-brand {
    display: flex;
  }
}
</style>
