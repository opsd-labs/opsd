/**
 * opsd web3 — Toast 通知可组合 API
 * 用法：const { toast } = useToast()
 *       toast.success('操作成功')
 */

import { inject, type InjectionKey } from 'vue'

export interface ToastItem {
  id: string
  type: 'success' | 'error' | 'warning' | 'info'
  message: string
  /** 自动消失延迟（ms），0 = 不自动消失 */
  duration: number
}

export interface ToastApi {
  success(msg: string, duration?: number): void
  error(msg: string, duration?: number): void
  warning(msg: string, duration?: number): void
  info(msg: string, duration?: number): void
  dismiss(id: string): void
}

export const TOAST_KEY: InjectionKey<ToastApi> = Symbol('toast')

export function useToast(): { toast: ToastApi } {
  const toast = inject(TOAST_KEY)
  if (!toast) {
    // 开发环境回退：直接 console，不会崩溃
    const noop = (msg: string) => console.info('[toast]', msg)
    return {
      toast: {
        success: noop,
        error: (msg) => console.error('[toast error]', msg),
        warning: (msg) => console.warn('[toast warn]', msg),
        info: noop,
        dismiss: () => {},
      },
    }
  }
  return { toast }
}
