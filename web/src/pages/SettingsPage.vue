<script setup lang="ts">
/**
 * SettingsPage — 设置页
 * 通用（外观 + 安全入口）/ 主题 / 分享 / 审计日志
 */
import { ref, computed, onMounted } from 'vue'
import { useTheme } from '../composables/useTheme'
import { apiGet, apiPost, apiPut, apiDelete, apiFetch, entrancePath } from '../api/client'
import { workspace } from '../state/workspace'
import type { components } from '../api/generated'
import Button from '../components/ui/Button.vue'
import Input from '../components/ui/Input.vue'
import Badge from '../components/ui/Badge.vue'
import Skeleton from '../components/ui/Skeleton.vue'

type SettingsTab = 'general' | 'themes' | 'share' | 'audit'
type ThemeInstalled = components['schemas']['ThemeInstalled']
type ThemeActive = components['schemas']['ThemeActive']
type ShareToken = components['schemas']['ShareToken']
type ShareSettings = components['schemas']['ShareSettings']
type RepositoryRelease = components['schemas']['ThemeRepositoryRelease']
type AuditLog = components['schemas']['AuditRecord']

const tabs: { key: SettingsTab; label: string }[] = [
  { key: 'general', label: '通用' },
  { key: 'themes',  label: '主题' },
  { key: 'share',   label: '分享' },
  { key: 'audit',   label: '审计日志' },
]

const active = ref<SettingsTab>('general')
const { preference, setTheme } = useTheme()

// ---- 通用：安全入口 ----

const entranceLoading = ref(false)
const entranceRotating = ref(false)
const entranceError = ref('')
const entranceValue = ref('')

async function loadEntrance() {
  entranceLoading.value = true
  try {
    const r = await apiGet<components['schemas']['EntranceValue']>('/api/v1/settings/entrance')
    entranceValue.value = r.value
  } catch (e: unknown) {
    entranceError.value = e instanceof Error ? e.message : '入口加载失败'
  } finally {
    entranceLoading.value = false
  }
}

async function rotateEntrance() {
  if (!confirm('轮换安全入口后将跳转到新入口并重新登录，是否继续？')) return
  entranceRotating.value = true
  entranceError.value = ''
  try {
    const alphabet = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_~'
    const value = Array.from(crypto.getRandomValues(new Uint8Array(16)), b => alphabet[b % 64]).join('')
    const r = await apiPut<components['schemas']['EntranceUpdate']>('/api/v1/settings/entrance', { value })
    window.location.replace(window.location.origin + '/' + r.value + '/')
  } catch (e: unknown) {
    entranceError.value = e instanceof Error ? e.message : '轮换失败'
    entranceRotating.value = false
  }
}

// ---- 主题 ----

const themesLoading = ref(false)
const themes = ref<ThemeInstalled[]>([])
const activeThemes = ref<ThemeActive | null>(null)
const themeError = ref('')

// ZIP 上传
const zipUploading = ref(false)
const zipError = ref('')
async function handleZipUpload(e: Event) {
  const file = (e.target as HTMLInputElement).files?.[0]
  if (!file) return
  if (file.size > 20 * 1024 * 1024) { zipError.value = '文件不得超过 20 MiB'; return }
  zipUploading.value = true
  zipError.value = ''
  try {
    await apiFetch('/api/v1/themes/console/package', {
      method: 'POST',
      headers: { 'Content-Type': 'application/zip' },
      body: file,
    })
    await loadThemes()
  } catch (err: unknown) {
    zipError.value = err instanceof Error ? err.message : '上传失败'
  } finally {
    zipUploading.value = false
    ;(e.target as HTMLInputElement).value = ''
  }
}

// 仓库安装
const repoUrl = ref('')
const repoResolving = ref(false)
const repoRelease = ref<RepositoryRelease | null>(null)
const resolvedUrl = ref('')
const repoError = ref('')
const repoInstalling = ref<number | null>(null)

async function resolveRepo() {
  if (!repoUrl.value.trim()) return
  repoResolving.value = true
  repoError.value = ''
  repoRelease.value = null
  try {
    const url = repoUrl.value.trim()
    const r = await apiPost<RepositoryRelease>(
      '/api/v1/themes/console/repository/resolve',
      { url }
    )
    repoRelease.value = r
    resolvedUrl.value = url
  } catch (err: unknown) {
    repoError.value = err instanceof Error ? err.message : '解析失败'
  } finally {
    repoResolving.value = false
  }
}

async function installRelease(assetId: number) {
  if (!repoRelease.value) return
  repoInstalling.value = assetId
  repoError.value = ''
  try {
    await apiPost('/api/v1/themes/console/repository/install', {
      url: resolvedUrl.value, tag: repoRelease.value.tag, asset_id: assetId,
    })
    repoRelease.value = null
    repoUrl.value = ''
    await loadThemes()
  } catch (err: unknown) {
    repoError.value = err instanceof Error ? err.message : '安装失败'
  } finally {
    repoInstalling.value = null
  }
}

// 切换主题
const switching = ref('')
async function switchFrontend(short: string) {
  const warning = workspace.state.activeStreams > 0 ? '并断开终端连接' : ''
  if (!confirm(`切换全局控制台主题将重新加载页面${warning}，未提交表单会丢失，后台任务继续运行，是否继续？`)) return
  switching.value = short
  themeError.value = ''
  try {
    await apiPut('/api/v1/themes/active', { surface: 'console_frontend', short })
    returnHome()
  } catch (err: unknown) {
    themeError.value = err instanceof Error ? err.message : '切换失败'
    switching.value = ''
  }
}

// 卸载主题
const removing = ref('')
async function removeTheme(short: string) {
  const current = activeFrontendShort.value === short
  if (!confirm(`确定卸载主题 "${short}"？${current ? '当前页面将返回内置前端，未提交表单会丢失，终端连接会断开，后台任务继续运行。' : ''}`)) return
  removing.value = short
  themeError.value = ''
  try {
    await apiDelete(`/api/v1/themes/${encodeURIComponent(short)}`)
    if (current) { returnHome(); return }
    await loadThemes()
  } catch (err: unknown) {
    themeError.value = err instanceof Error ? err.message : '卸载失败'
  } finally {
    removing.value = ''
  }
}

const activeFrontendShort = computed(() =>
  activeThemes.value?.console_frontend?.short ?? 'default'
)

function returnHome() {
  const query = new URLSearchParams(window.location.search)
  window.location.replace(`${entrancePath()}/${query.size ? '?' + query.toString() : ''}`)
}

function themeName(name: ThemeInstalled['name']): string {
  return typeof name === 'string' ? name : name['zh-CN'] ?? Object.values(name)[0] ?? ''
}

async function loadThemes() {
  themesLoading.value = true
  try {
    const [list, act] = await Promise.all([
      apiGet<{ themes: ThemeInstalled[] }>('/api/v1/themes'),
      apiGet<ThemeActive>('/api/v1/themes/active'),
    ])
    themes.value = list.themes
    activeThemes.value = act
  } catch (e: unknown) {
    themeError.value = e instanceof Error ? e.message : '主题加载失败'
  } finally {
    themesLoading.value = false
  }
}

// ---- 分享令牌 ----

const sharesLoading = ref(false)
const shares = ref<ShareToken[]>([])
const shareSettings = ref<ShareSettings | null>(null)
const shareSaving = ref(false)
const createdShareUrl = ref('')
const newShareLabel = ref('')
const sharesError = ref('')
const creatingShare = ref(false)

async function loadShares() {
  sharesLoading.value = true
  try {
    const [list, settings] = await Promise.all([
      apiGet<components['schemas']['ShareTokenList']>('/api/v1/share/tokens'),
      apiGet<ShareSettings>('/api/v1/share/settings'),
    ])
    shares.value = list.tokens
    shareSettings.value = settings
  } catch (e: unknown) {
    sharesError.value = e instanceof Error ? e.message : '分享设置加载失败'
  } finally {
    sharesLoading.value = false
  }
}

async function toggleShare() {
  if (!shareSettings.value) return
  shareSaving.value = true
  sharesError.value = ''
  try {
    shareSettings.value = await apiPut<ShareSettings>('/api/v1/share/settings', {
      ...shareSettings.value, enabled: !shareSettings.value.enabled,
    })
  } catch (e: unknown) {
    sharesError.value = e instanceof Error ? e.message : '分享开关保存失败'
  } finally { shareSaving.value = false }
}

async function createShare() {
  if (!newShareLabel.value.trim()) return
  creatingShare.value = true
  sharesError.value = ''
  try {
    const result = await apiPost<components['schemas']['ShareTokenCreated']>('/api/v1/share/tokens', { label: newShareLabel.value.trim() })
    createdShareUrl.value = `${window.location.origin}/share/${result.token}/`
    newShareLabel.value = ''
    await loadShares()
  } catch (err: unknown) {
    sharesError.value = err instanceof Error ? err.message : '创建失败'
  } finally {
    creatingShare.value = false
  }
}

async function deleteShare(token: string) {
  if (!confirm('确定删除此分享令牌？现有分享链接将立即失效。')) return
  try {
    await apiDelete(`/api/v1/share/tokens/${encodeURIComponent(token)}`)
    await loadShares()
  } catch (err: unknown) {
    sharesError.value = err instanceof Error ? err.message : '删除失败'
  }
}

// ---- 审计日志 ----

const auditLoading = ref(false)
const auditLogs = ref<AuditLog[]>([])
const auditError = ref('')

async function loadAudit() {
  auditLoading.value = true
  try {
    const r = await apiGet<{ records: AuditLog[]; retention_seconds: number }>(
      '/api/v1/audit?limit=50'
    )
    auditLogs.value = r.records
  } catch (e: unknown) {
    auditError.value = e instanceof Error ? e.message : '审计加载失败'
  } finally {
    auditLoading.value = false
  }
}

// ---- 生命周期 ----

onMounted(() => {
  loadEntrance()
})

function onTabChange(tab: SettingsTab) {
  active.value = tab
  if (tab === 'themes' && themes.value.length === 0) loadThemes()
  if (tab === 'share' && shares.value.length === 0) loadShares()
  if (tab === 'audit' && auditLogs.value.length === 0) loadAudit()
}

function formatDate(unixSecs: number): string {
  return new Date(unixSecs * 1000).toLocaleString('zh-CN', {
    month: '2-digit', day: '2-digit',
    hour: '2-digit', minute: '2-digit',
  })
}
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
        @click="onTabChange(t.key)"
      >
        {{ t.label }}
      </button>
    </div>

    <!-- =================== 通用 =================== -->
    <div v-if="active === 'general'" class="settings-section">

      <!-- 外观 -->
      <div class="settings-group">
        <h3 class="settings-group__title">外观</h3>
        <div class="settings-row">
          <span class="settings-row__label">配色模式</span>
          <div class="theme-options">
            <button
              v-for="opt in (['light', 'dark', 'system'] as const)"
              :key="opt"
              class="theme-btn"
              :class="{ 'theme-btn--active': preference === opt }"
              @click="setTheme(opt)"
            >{{ opt === 'light' ? '浅色' : opt === 'dark' ? '深色' : '跟随系统' }}</button>
          </div>
        </div>
      </div>

      <!-- 安全入口 -->
      <div class="settings-group">
        <h3 class="settings-group__title">安全入口</h3>
        <p class="settings-group__desc">所有管理接口的路径前缀，轮换后当前会话将跳转到新入口。</p>
        <div v-if="entranceLoading" class="settings-row">
          <Skeleton height="32px" width="180px" />
        </div>
        <div v-else class="settings-row">
          <code class="entrance-value">{{ entranceValue || '—' }}</code>
          <Button
            variant="danger"
            size="sm"
            :busy="entranceRotating"
            @click="rotateEntrance"
          >轮换入口</Button>
        </div>
        <p v-if="entranceError" class="settings-error">{{ entranceError }}</p>
      </div>
    </div>

    <!-- =================== 主题 =================== -->
    <div v-else-if="active === 'themes'" class="settings-section">

      <!-- 当前激活 -->
      <div class="settings-group">
        <h3 class="settings-group__title">当前控制台前端</h3>
        <div v-if="themesLoading" class="settings-row"><Skeleton height="28px" width="120px" /></div>
        <div v-else class="settings-row">
          <Badge>{{ activeFrontendShort === 'default' ? '内置前端 (default)' : activeFrontendShort }}</Badge>
          <span v-if="activeFrontendShort !== 'default'" class="settings-row__hint">
            在下方切换回内置前端
          </span>
        </div>
      </div>

      <!-- 已安装主题列表 -->
      <div class="settings-group">
        <h3 class="settings-group__title">已安装主题</h3>
        <div v-if="themesLoading" class="theme-list-skeleton">
          <Skeleton v-for="i in 2" :key="i" height="56px" style="border-radius:var(--radius-md)" />
        </div>
        <div v-else class="theme-list">
          <!-- 内置 default -->
          <div class="theme-item">
            <div class="theme-item__info">
              <span class="theme-item__name">内置前端</span>
              <span class="theme-item__short">default</span>
            </div>
            <div class="theme-item__actions">
              <Badge v-if="activeFrontendShort === 'default'" tone="success">当前激活</Badge>
              <Button
                v-else
                variant="default"
                size="sm"
                :busy="switching === 'default'"
                @click="switchFrontend('default')"
              >切换到此</Button>
            </div>
          </div>

          <!-- 外部主题 -->
          <div
            v-for="t in themes.filter(th => th.console_mode === 'frontend')"
            :key="t.short"
            class="theme-item"
          >
            <div class="theme-item__info">
              <span class="theme-item__name">{{ themeName(t.name) }}</span>
              <span class="theme-item__short">{{ t.short }}</span>
              <span v-if="t.version" class="theme-item__ver">v{{ t.version }}</span>
            </div>
            <div class="theme-item__actions">
              <Badge v-if="activeFrontendShort === t.short" tone="success">当前激活</Badge>
              <Button
                v-else
                variant="default"
                size="sm"
                :busy="switching === t.short"
                @click="switchFrontend(t.short)"
              >切换</Button>
              <Button
                variant="ghost"
                size="sm"
                :busy="removing === t.short"
                @click="removeTheme(t.short)"
              >卸载</Button>
            </div>
          </div>

          <p v-if="themes.filter(t => t.console_mode === 'frontend').length === 0" class="theme-empty">
            暂无已安装的外部控制台主题
          </p>
        </div>
        <p v-if="themeError" class="settings-error">{{ themeError }}</p>
      </div>

      <!-- ZIP 上传 -->
      <div class="settings-group">
        <h3 class="settings-group__title">上传主题包</h3>
        <p class="settings-group__desc">主题 ZIP 包根目录须包含 <code>theme.json</code> 和 <code>index.html</code>，最大 20 MiB。</p>
        <div class="settings-row">
          <label class="file-upload-label" :class="{ 'file-upload-label--busy': zipUploading }">
            <input
              type="file"
              accept=".zip"
              class="file-upload-input"
              :disabled="zipUploading"
              @change="handleZipUpload"
            />
            <span>{{ zipUploading ? '上传中…' : '选择 ZIP 文件' }}</span>
          </label>
        </div>
        <p v-if="zipError" class="settings-error">{{ zipError }}</p>
      </div>

      <!-- GitHub 仓库安装 -->
      <div class="settings-group">
        <h3 class="settings-group__title">从 GitHub 安装</h3>
        <p class="settings-group__desc">粘贴公开仓库或发行版 URL，解析后选择版本安装。</p>
        <div class="settings-row settings-row--gap">
          <Input
            v-model="repoUrl"
            placeholder="https://github.com/opsd-labs/opsd-theme-web"
            style="flex:1"
          />
          <Button
            variant="default"
            size="sm"
            :busy="repoResolving"
            @click="resolveRepo"
          >解析</Button>
        </div>
        <p v-if="repoError" class="settings-error">{{ repoError }}</p>

        <!-- 发行版列表 -->
        <div v-if="repoRelease" class="release-list">
          <div v-for="asset in repoRelease.assets" :key="asset.id" class="release-item">
            <div class="release-item__info">
              <span class="release-item__tag">{{ repoRelease.tag }}</span>
              <span class="release-item__asset">{{ asset.name }}</span>
            </div>
            <Button
              variant="primary"
              size="sm"
              :busy="repoInstalling === asset.id"
              :disabled="repoInstalling !== null"
              @click="installRelease(asset.id)"
            >安装</Button>
          </div>
        </div>
      </div>
    </div>

    <!-- =================== 分享令牌 =================== -->
    <div v-else-if="active === 'share'" class="settings-section">
      <div class="settings-group">
        <h3 class="settings-group__title">分享令牌</h3>
        <p class="settings-group__desc">每个令牌生成一个公开只读分享链接，令牌删除后链接立即失效。</p>
        <div v-if="shareSettings" class="settings-row">
          <span>公开分享：{{ shareSettings.enabled ? '已启用' : '已关闭' }}</span>
          <Button variant="default" size="sm" :busy="shareSaving" @click="toggleShare">{{ shareSettings.enabled ? '关闭分享' : '启用分享' }}</Button>
        </div>
        <p v-if="createdShareUrl" class="settings-group__desc">此链接仅显示一次，请保存：<a :href="createdShareUrl" target="_blank" rel="noopener noreferrer">{{ createdShareUrl }}</a></p>

        <!-- 创建 -->
        <div class="settings-row settings-row--gap">
          <Input v-model="newShareLabel" placeholder="标签（如：团队演示）" style="flex:1" />
          <Button
            variant="primary"
            size="sm"
            :busy="creatingShare"
            :disabled="!newShareLabel.trim()"
            @click="createShare"
          >创建</Button>
        </div>
        <p v-if="sharesError" class="settings-error">{{ sharesError }}</p>

        <!-- 令牌列表 -->
        <div v-if="sharesLoading" class="share-list-skeleton">
          <Skeleton v-for="i in 3" :key="i" height="48px" style="border-radius:var(--radius-md)" />
        </div>
        <div v-else class="share-list">
          <div v-for="s in shares" :key="s.id" class="share-item">
            <div class="share-item__info">
              <span class="share-item__name">{{ s.label }}</span>
              <span class="share-item__meta">创建于 {{ formatDate(s.created_at) }}</span>
              <Badge :tone="s.revoked ? 'warning' : 'success'">{{ s.revoked ? '已撤销' : s.expires_at && s.expires_at <= Date.now() / 1000 ? '已过期' : '有效' }}</Badge>
            </div>
            <Button variant="ghost" size="sm" :disabled="s.revoked" @click="deleteShare(s.id)">撤销</Button>
          </div>
          <p v-if="shares.length === 0" class="theme-empty">暂无分享令牌</p>
        </div>
      </div>
    </div>

    <!-- =================== 审计日志 =================== -->
    <div v-else-if="active === 'audit'" class="settings-section">
      <div class="settings-group">
        <h3 class="settings-group__title">审计日志</h3>
        <p class="settings-group__desc">显示最近 50 条记录。</p>
        <p v-if="auditError" class="settings-error">{{ auditError }}</p>
        <div v-if="auditLoading" class="audit-skeleton">
          <Skeleton v-for="i in 8" :key="i" height="36px" />
        </div>
        <table v-else class="audit-table">
          <thead>
            <tr>
              <th>时间</th>
              <th>操作者</th>
              <th>操作</th>
              <th>目标</th>
              <th>结果</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="log in auditLogs" :key="log.id">
              <td class="audit-time">{{ formatDate(log.at) }}</td>
              <td>{{ log.actor }}</td>
              <td>{{ log.category }}</td>
              <td class="audit-target">{{ log.target ?? '—' }}</td>
              <td>
                <Badge :tone="log.result === '失败' ? 'danger' : 'default'">{{ log.result }}</Badge>
              </td>
            </tr>
            <tr v-if="auditLogs.length === 0">
              <td colspan="5" class="audit-empty">暂无审计记录</td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>
  </div>
</template>

<style scoped>
.settings-page { display: flex; flex-direction: column; gap: var(--sp-5); max-width: 760px; }

.settings-tabs {
  display: flex;
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
  justify-content: space-between; gap: var(--sp-3);
}
.settings-row--gap { justify-content: flex-start; }
.settings-row__label { font-size: var(--text-sm); color: var(--color-secondary); }
.settings-row__hint { font-size: var(--text-xs); color: var(--color-muted); }

.settings-error { font-size: var(--text-sm); color: var(--color-danger); }

/* 外观 */
.theme-options { display: flex; gap: var(--sp-1); }
.theme-btn {
  padding: var(--sp-1) var(--sp-3); font-size: var(--text-sm);
  background: var(--color-surface); color: var(--color-secondary);
  border: 1px solid var(--color-border); border-radius: var(--radius-md);
  cursor: pointer;
  transition: background-color var(--duration-fast), color var(--duration-fast), border-color var(--duration-fast);
}
.theme-btn:hover { background: var(--color-hover-bg); color: var(--color-ink); }
.theme-btn--active { background: var(--color-accent-subtle); color: var(--color-accent); border-color: var(--color-accent-muted); }
.theme-btn:focus-visible { outline: none; box-shadow: var(--shadow-focus); }

/* 安全入口 */
.entrance-value {
  font-family: var(--font-mono); font-size: var(--text-sm);
  padding: var(--sp-1) var(--sp-2);
  background: var(--color-surface);
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-sm); color: var(--color-ink);
}

/* 主题列表 */
.theme-list, .theme-list-skeleton { display: flex; flex-direction: column; gap: var(--sp-2); }
.theme-item {
  display: flex; align-items: center; justify-content: space-between; gap: var(--sp-3);
  padding: var(--sp-3) var(--sp-4);
  background: var(--color-surface);
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-md);
}
.theme-item__info { display: flex; align-items: center; gap: var(--sp-2); flex: 1; min-width: 0; }
.theme-item__name { font-size: var(--text-sm); font-weight: var(--weight-medium); color: var(--color-ink); }
.theme-item__short { font-size: var(--text-xs); font-family: var(--font-mono); color: var(--color-muted); }
.theme-item__ver { font-size: var(--text-xs); color: var(--color-muted); }
.theme-item__actions { display: flex; align-items: center; gap: var(--sp-2); flex-shrink: 0; }
.theme-empty { font-size: var(--text-sm); color: var(--color-muted); padding: var(--sp-4); text-align: center; }

/* ZIP 上传 */
.file-upload-label {
  display: inline-flex; align-items: center; gap: var(--sp-2);
  padding: var(--sp-2) var(--sp-4);
  font-size: var(--text-sm); font-weight: var(--weight-medium);
  background: var(--color-surface); color: var(--color-secondary);
  border: 1px dashed var(--color-border);
  border-radius: var(--radius-md); cursor: pointer;
  transition: border-color var(--duration-fast), color var(--duration-fast);
}
.file-upload-label:hover { border-color: var(--color-accent); color: var(--color-accent); }
.file-upload-label--busy { opacity: 0.6; pointer-events: none; }
.file-upload-input { display: none; }

/* 发行版列表 */
.release-list { display: flex; flex-direction: column; gap: var(--sp-2); margin-top: var(--sp-2); }
.release-item {
  display: flex; align-items: center; justify-content: space-between; gap: var(--sp-3);
  padding: var(--sp-2) var(--sp-3);
  background: var(--color-surface);
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-sm);
}
.release-item__info { display: flex; align-items: center; gap: var(--sp-3); }
.release-item__tag { font-family: var(--font-mono); font-size: var(--text-sm); color: var(--color-ink); }
.release-item__asset { font-size: var(--text-xs); color: var(--color-muted); }

/* 分享令牌 */
.share-list, .share-list-skeleton { display: flex; flex-direction: column; gap: var(--sp-2); }
.share-item {
  display: flex; align-items: center; justify-content: space-between; gap: var(--sp-3);
  padding: var(--sp-3) var(--sp-4);
  background: var(--color-surface);
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-md);
}
.share-item__info { display: flex; align-items: center; gap: var(--sp-2); flex: 1; min-width: 0; }
.share-item__name { font-size: var(--text-sm); font-weight: var(--weight-medium); color: var(--color-ink); }
.share-item__meta { font-size: var(--text-xs); color: var(--color-muted); }

/* 审计日志 */
.audit-skeleton { display: flex; flex-direction: column; gap: var(--sp-1); }
.audit-table {
  width: 100%; border-collapse: collapse; font-size: var(--text-sm);
}
.audit-table th, .audit-table td {
  padding: var(--sp-2) var(--sp-3);
  text-align: left;
  border-bottom: 1px solid var(--color-border-subtle);
}
.audit-table th {
  font-weight: var(--weight-medium); color: var(--color-muted);
  background: var(--color-surface); position: sticky; top: 0;
}
.audit-table td { color: var(--color-ink); }
.audit-time { font-family: var(--font-mono); white-space: nowrap; color: var(--color-muted) !important; }
.audit-target { font-family: var(--font-mono); font-size: var(--text-xs); }
.audit-empty { text-align: center; color: var(--color-muted); padding: var(--sp-6) !important; }
.audit-pagination {
  display: flex; align-items: center; gap: var(--sp-3);
  justify-content: flex-end; padding-top: var(--sp-3);
}
.audit-page-info { font-size: var(--text-sm); color: var(--color-muted); }
</style>
