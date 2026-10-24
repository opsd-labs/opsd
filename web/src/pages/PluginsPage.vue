<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { apiFetch, apiGet, apiPut, apiDelete, entrancePath } from '../api/client'
import { workspace } from '../state/workspace'
import Button from '../components/ui/Button.vue'

const busy = ref(false)
const error = ref('')
const configured = ref('')
const config = ref('{}')
const secretNames = ref<string[]>([])
const secretName = ref('')
const secretValue = ref('')
const selected = computed(() => workspace.state.plugins.find(record => record.enabled && record.manifest.page && record.manifest.id === workspace.state.selectedPluginId))
const source = computed(() => selected.value ? `${entrancePath()}/plugins/${encodeURIComponent(selected.value.manifest.id)}/${selected.value.manifest.page}?version=${encodeURIComponent(selected.value.manifest.version)}` : '')

async function run(action: () => Promise<unknown>) {
  busy.value = true
  error.value = ''
  try { await action() } catch (reason) { error.value = reason instanceof Error ? reason.message : '插件操作失败' } finally { busy.value = false }
}
async function upload(event: Event) {
  const file = (event.target as HTMLInputElement).files?.[0]
  if (!file) return
  await run(async () => {
    await apiFetch('/api/v1/plugins/package', { method: 'POST', headers: { 'Content-Type': 'application/zip' }, body: file })
    await workspace.refreshPlugins()
  })
  ;(event.target as HTMLInputElement).value = ''
}
async function selection(id: string, version: string, enabled: boolean) {
  await run(async () => {
    await apiPut(`/api/v1/plugins/${id}/selection`, { version, enabled })
    await workspace.refreshPlugins()
  })
}
async function configure(id: string) {
  await run(async () => {
    const result = await apiGet<{ config: unknown; secret_names: string[] }>(`/api/v1/plugins/${id}/configuration`)
    configured.value = id
    config.value = JSON.stringify(result.config, null, 2)
    secretNames.value = result.secret_names
    secretName.value = ''
    secretValue.value = ''
  })
}
async function save() {
  await run(async () => {
    const secrets: Record<string, string> = {}
    if (secretName.value.trim()) secrets[secretName.value.trim()] = secretValue.value
    const result = await apiPut<{ secret_names: string[] }>(`/api/v1/plugins/${configured.value}/configuration`, { config: JSON.parse(config.value), secrets })
    secretNames.value = result.secret_names
    secretName.value = ''
    secretValue.value = ''
  })
}
async function uninstall(id: string, version: string) {
  if (!confirm(`卸载 ${id} ${version} 的程序包？管理的数据和秘密会保留。`)) return
  await run(async () => { await apiDelete(`/api/v1/plugins/${id}/${version}`); await workspace.refreshPlugins() })
}
watch(() => workspace.state.selectedPluginId, () => { error.value = ''; configured.value = '' })
</script>

<template>
  <section class="plugins-page">
    <p v-if="error" role="alert" class="error">{{ error }}</p>
    <template v-if="source">
      <div class="plugin-heading">
        <span>{{ selected?.manifest.name }} · {{ selected?.manifest.version }}</span>
        <Button size="sm" @click="workspace.openPlugin(null)">插件管理</Button>
      </div>
      <iframe :key="source" :src="source" :title="selected?.manifest.name" class="plugin-frame" />
    </template>
    <template v-else>
      <div class="plugin-heading">
        <p>独立安装、切换版本和配置插件。升级后，已受理任务继续使用原版本。</p>
        <label class="upload">安装插件 ZIP<input type="file" accept=".zip,application/zip" :disabled="busy" @change="upload" /></label>
      </div>
      <div class="plugin-grid">
        <article v-for="record in workspace.state.plugins" :key="`${record.manifest.id}@${record.manifest.version}`" class="plugin-card">
          <h2>{{ record.manifest.name }}</h2>
          <p>{{ record.manifest.id }} · {{ record.manifest.version }} · {{ record.enabled ? '已启用' : '未启用' }}</p>
          <p>依赖：{{ record.manifest.dependencies.join('、') || '无' }}</p>
          <div class="actions">
            <Button v-if="record.enabled && record.manifest.page" variant="primary" size="sm" @click="workspace.openPlugin(record.manifest.id)">打开</Button>
            <Button size="sm" :disabled="busy" @click="selection(record.manifest.id, record.manifest.version, !record.enabled)">{{ record.enabled ? '禁用' : '启用此版本' }}</Button>
            <Button v-if="record.enabled" size="sm" @click="configure(record.manifest.id)">配置</Button>
            <Button v-else size="sm" :disabled="busy" @click="uninstall(record.manifest.id, record.manifest.version)">卸载程序包</Button>
          </div>
        </article>
        <p v-if="!workspace.state.plugins.length">暂无插件。安装独立发行的插件 ZIP 后，按依赖顺序启用。</p>
      </div>
      <form v-if="configured" class="plugin-card configuration" @submit.prevent="save">
        <h2>{{ configured }} / 配置</h2>
        <label>普通配置（JSON；可用字段：{{ workspace.state.plugins.find(record => record.enabled && record.manifest.id === configured)?.manifest.config_keys.join('、') || '无' }}）<textarea v-model="config" rows="7" spellcheck="false" /></label>
        <p>已保存的秘密名称：{{ secretNames.join('、') || '无' }}。原文不会返回；留空值并填写名称可删除该秘密。</p>
        <div class="secret-fields"><label>秘密名称<input v-model="secretName" autocomplete="off" placeholder="例如 password" /></label><label>秘密值<input v-model="secretValue" type="password" autocomplete="new-password" /></label></div>
        <p>{{ workspace.state.plugins.find(record => record.enabled && record.manifest.id === configured)?.manifest.configuration_help }}</p>
        <Button type="submit" variant="primary" :busy="busy">保存配置</Button>
      </form>
    </template>
  </section>
</template>

<style scoped>
.plugin-heading,.actions{display:flex;gap:12px;justify-content:space-between;align-items:center;flex-wrap:wrap;margin-bottom:18px}.plugin-heading p{color:var(--color-secondary);margin:0}.plugin-grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(270px,1fr));gap:18px}.plugin-card{background:var(--color-surface);border:1px solid var(--color-border);border-radius:var(--radius-lg);padding:22px}.plugin-card h2{font-size:16px;margin:0 0 12px}.plugin-card p{color:var(--color-secondary);font-size:13px}.actions{justify-content:flex-start;margin:18px 0 0}.upload{cursor:pointer;color:var(--color-accent);font-size:13px}.upload input{display:block;max-width:260px;margin-top:5px}.configuration{margin-top:20px}.configuration label{display:flex;flex-direction:column;gap:8px;font-size:13px}.configuration textarea,.configuration input{background:var(--color-bg);color:var(--color-ink);border:1px solid var(--color-border);border-radius:var(--radius-md);padding:10px}.configuration textarea{font:13px/1.6 monospace;resize:vertical}.secret-fields{display:grid;grid-template-columns:1fr 1fr;gap:14px}.error{color:var(--color-danger)}.plugin-frame{width:100%;min-height:950px;height:calc(100dvh - 170px);border:0;border-radius:var(--radius-lg)}@media(width < 700px){.secret-fields{grid-template-columns:1fr}.plugin-card{padding:16px}}
</style>
