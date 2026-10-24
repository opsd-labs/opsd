<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { apiDelete, apiFetch, apiGet, apiPost, apiPut } from '../../api/client'
import { workspace } from '../../state/workspace'
import Button from '../../components/ui/Button.vue'
import Badge from '../../components/ui/Badge.vue'
import Drawer from '../../components/layout/Drawer.vue'
import { catalog, loadCatalog, moduleLabels, modulePages, pending, taskLabel, type ModuleId } from './useModule'
import './module.css'

const props = defineProps<{ id: ModuleId }>()
const emit = defineEmits<{ changed: [] }>()
const open = ref(false)
const error = ref('')
const busy = ref(false)
const config = ref<Record<string, string>>({})
const secrets = ref<Record<string, string>>({})
const secretNames = ref<string[]>([])
const extraName = ref('')
const extraValue = ref('')
const expected = computed(() => catalog.value.find(item => item.id === props.id))
const records = computed(() => workspace.state.plugins.filter(item => item.manifest.id === props.id))
const active = computed(() => records.value.find(item => item.enabled))
const current = computed(() => records.value.find(item => item.manifest.version === expected.value?.version))
const available = computed(() => !!expected.value && active.value?.manifest.version === expected.value.version)
const dependencies = computed(() => expected.value?.dependencies.filter(id => !workspace.state.plugins.some(item => item.manifest.id === id && item.enabled && item.manifest.version === catalog.value.find(module => module.id === id)?.version)) ?? [])
const tasks = computed(() => workspace.state.tasks.filter(task => task.node_id === '@hub' && task.action.type === 'plugin' && task.action.plugin === props.id).sort((a, b) => b.created_at - a.created_at).slice(0, 6))
const fields = computed(() => ({
  docker: { username: '管理用户名', server_image: '管理服务镜像（留空使用配套版本）', agent_image: '节点管理程序镜像（留空使用配套版本）' },
  et: { url: 'ET Web 地址', username: '登录用户名' },
  silo: {},
}[props.id]))
const passwordFields = computed(() => ({ docker: { admin_password: '管理密码（至少 12 位）', agent_secret: '节点共享密钥（至少 16 位）', api_key: '已有管理服务 API Key（可选）' }, et: { password: '登录密码', network_secret: '首次组网密钥（至少 16 位）' }, silo: { root_user: '管理用户名', root_password: '管理密码（至少 16 位）' } }[props.id]))

async function perform(action: () => Promise<unknown>, changed = true) {
  busy.value = true; error.value = ''
  const wasAvailable = available.value
  try {
    await action()
    if (changed) {
      await workspace.refreshPlugins()
      if (wasAvailable && available.value) emit('changed')
    }
  }
  catch (reason) { error.value = reason instanceof Error ? reason.message : '模块操作失败' }
  finally { busy.value = false }
}
async function enable() {
  if (dependencies.value.length) throw new Error('请先完成下方依赖功能的模块安装与启用')
  await apiPut(`/api/v1/plugins/${props.id}/selection`, { version: expected.value?.version, enabled: true })
}
async function online() {
  await perform(async () => {
    // 响应中断后先读服务器记录；已经安装的版本不再次下载。
    await workspace.refreshPlugins()
    if (!current.value) await apiPost(`/api/v1/plugins/${props.id}/install`)
    if (!dependencies.value.length) await enable()
  })
}
async function upload(event: Event) {
  const target = event.target as HTMLInputElement
  const file = target.files?.[0]
  if (!file) return
  await perform(async () => {
    await apiFetch(`/api/v1/plugins/${props.id}/package`, { method: 'POST', headers: { 'Content-Type': 'application/zip' }, body: file })
    if (!dependencies.value.length) await enable()
  })
  target.value = ''
}
async function manage() {
  open.value = true
  await perform(async () => {
    if (!active.value) return
    const value = await apiGet<{ config: Record<string, string>; secret_names: string[] }>(`/api/v1/plugins/${props.id}/configuration`)
    config.value = value.config; secretNames.value = value.secret_names; secrets.value = {}; extraName.value = ''; extraValue.value = ''
  }, false)
}
async function save() {
  await perform(async () => {
    const values = Object.fromEntries(Object.entries(secrets.value).filter(([, value]) => value !== ''))
    if (extraName.value.trim() && extraValue.value) values[extraName.value.trim()] = extraValue.value
    const ordinary = Object.fromEntries(Object.entries(config.value).filter(([, value]) => value !== ''))
    const value = await apiPut<{ secret_names: string[] }>(`/api/v1/plugins/${props.id}/configuration`, { config: ordinary, secrets: values })
    secretNames.value = value.secret_names; secrets.value = {}; extraValue.value = ''; open.value = false
  })
}
async function disable(version: string) {
  if (confirm('停止受理该模块的新操作？已有服务和数据会保留，活动任务继续使用原版本。')) await perform(() => apiPut(`/api/v1/plugins/${props.id}/selection`, { version, enabled: false }))
}
async function uninstall(version: string) {
  if (confirm(`卸载 ${version} 的模块程序包？已有服务、数据和配置会保留。`)) await perform(() => apiDelete(`/api/v1/plugins/${props.id}/${version}`))
}
function close() { open.value = false; secrets.value = {}; extraValue.value = '' }
onMounted(() => { void perform(loadCatalog, false) })
</script>

<template>
  <section class="module-page">
    <div class="module-toolbar">
      <p class="module-muted"><slot name="description" /></p>
      <Button size="sm" @click="manage">管理</Button>
    </div>
    <p v-if="error" class="module-error" role="alert">{{ error }}</p>
    <section v-if="!available" class="module-panel">
      <h2>{{ active ? '需要升级功能模块' : current ? '启用功能模块' : '安装功能模块' }}</h2>
      <p class="module-muted">{{ moduleLabels[id] }}页面需要配套模块 {{ expected?.version || '正在读取版本' }}。安装程序包后，在本页继续配置和接入服务。</p>
      <div class="module-row module-actions">
        <Button v-if="!current" variant="primary" :busy="busy" :disabled="!expected" @click="online">在线安装配套版本</Button>
        <Button v-else variant="primary" :busy="busy" :disabled="!!dependencies.length" @click="perform(enable)">{{ active ? '切换至配套版本' : '启用配套版本' }}</Button>
        <label class="module-field">上传模块 ZIP<input type="file" accept=".zip,application/zip" :disabled="busy || !expected" @change="upload" /></label>
      </div>
      <p v-if="dependencies.length" class="module-muted">需要先启用依赖模块：</p>
      <div class="module-row"><Button v-for="dependency in dependencies" :key="dependency" size="sm" @click="workspace.navigate(modulePages[dependency as ModuleId])">前往{{ moduleLabels[dependency as ModuleId] }}</Button></div>
    </section>
    <slot v-if="available" />
    <details v-if="tasks.length" :open="tasks.some(task => pending(task.status))" class="module-panel">
      <summary>任务与恢复记录</summary>
      <div v-for="task in tasks" :key="task.id" class="module-task">
        <div class="module-row"><Badge>{{ taskLabel(task.status) }}</Badge><span>{{ task.action.type === 'plugin' ? task.action.operation : '' }}</span><code>{{ task.id }}</code></div>
        <p v-if="task.error" class="module-error">{{ task.error }}</p>
        <details v-if="task.result"><summary>执行结果</summary><pre>{{ JSON.stringify(task.result, null, 2) }}</pre></details>
      </div>
    </details>
    <Drawer :open="open" :title="`${moduleLabels[id]}管理`" @close="close">
      <div class="module-page">
        <p v-if="error" role="alert" class="module-error">{{ error }}</p>
        <form v-if="active" @submit.prevent="save">
          <label v-for="(label, key) in fields" :key="key" class="module-field"><span>{{ label }}</span><input v-model="config[key]" autocomplete="off" /></label>
          <label v-for="(label, key) in passwordFields" :key="key" class="module-field"><span>{{ label }} · {{ secretNames.includes(key) ? '已配置，留空保留' : '未配置' }}</span><input v-model="secrets[key]" type="password" autocomplete="new-password" /></label>
          <template v-if="id === 'silo'"><label class="module-field"><span>访问用户密钥名称</span><input v-model="extraName" /></label><label class="module-field"><span>访问用户密钥值</span><input v-model="extraValue" type="password" autocomplete="new-password" /></label></template>
          <Button type="submit" variant="primary" :busy="busy">保存配置</Button>
        </form>
        <section v-for="record in records" :key="record.manifest.version" class="module-panel">
          <h2>{{ record.manifest.version }} · {{ record.enabled ? '已启用' : '未启用' }}</h2>
          <div class="module-row"><Button v-if="record.enabled" size="sm" :disabled="busy" @click="disable(record.manifest.version)">禁用模块</Button><Button v-else size="sm" variant="danger" :disabled="busy" @click="uninstall(record.manifest.version)">卸载程序包</Button></div>
        </section>
      </div>
    </Drawer>
  </section>
</template>
