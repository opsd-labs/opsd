import { api, core, nodes, el, page, panel, button, table, field, select, editor, report, json, job, nativeLink } from './ui.js'

const main = page('EasyTier 网络', '连接现有 ET Web，查看组网状态并将实例关联到 OPSD 节点。')
const machines = el('div')
const detail = el('div')
main.append(el('div', { class: 'row' }, button('刷新机器', refresh)), panel('网络机器', machines), detail)
let selectedMachine = ''
let selectedInstance = ''

function uuid(value) {
  if (typeof value === 'string') return value
  const text = ['part1', 'part2', 'part3', 'part4'].map(key => Number(value[key]).toString(16).padStart(8, '0')).join('')
  return `${text.slice(0, 8)}-${text.slice(8, 12)}-${text.slice(12, 16)}-${text.slice(16, 20)}-${text.slice(20)}`
}
async function refresh() {
  const response = await api('/machines')
  const list = response.machines || []
  machines.replaceChildren(table(['机器', '版本', '上报时间', '操作'], list.map(machine => [
    machine.info?.hostname || machine.machine_id, machine.info?.easytier_version || '—', machine.info?.report_time || '—',
    machine.machine_id ? button('查看实例', () => showInstances(machine)) : '尚未上报机器身份',
  ])))
  try {
    const console = await api('/console')
    if (!main.querySelector('header a')) main.querySelector('header').append(nativeLink(console.url, '打开 ET 管理台 ↗'))
  } catch { /* 查询状态不依赖管理台按钮。 */ }
  report(`已读取 ${list.length} 台机器；只有明确关联的实例参与 OPSD 网络目录。`)
}
async function showInstances(machine) {
  selectedMachine = machine.machine_id
  const state = await api('/instances', 'POST', { machine_id: selectedMachine })
  const running = (state.running_inst_ids || []).map(uuid)
  const disabled = (state.disabled_inst_ids || []).map(uuid)
  const ids = [...new Set([...running, ...disabled, ...(machine.info?.running_network_instances || []).map(uuid)])]
  detail.replaceChildren(panel(`${machine.info?.hostname || selectedMachine} / 网络实例`, table(['实例编号', '状态', '操作'], ids.map(id => [
    id, running.includes(id) ? '运行中' : '已停止', el('div', { class: 'row' },
      button('配置与链路', () => showInstance(id)),
      button(running.includes(id) ? '停止实例' : '启动实例', async () => {
        if (!confirm(`${running.includes(id) ? '停止' : '启动'}该 ET 实例？停止可能中断当前 ET 管理线路。`)) return
        await job('/instance/state', { machine_id: selectedMachine, instance_id: id, disabled: running.includes(id) })
        await showInstances(machine)
      })),
  ])), el('details', {}, el('summary', {}, '实例目录原始状态'), el('pre', {}, json(state)))))
}
async function showInstance(id) {
  selectedInstance = id
  const identity = { machine_id: selectedMachine, instance_id: id }
  const [config, info, managed] = await Promise.all([api('/config', 'POST', identity), api('/info', 'POST', identity), nodes()])
  const existing = main.querySelector('#instance-detail')
  existing?.remove()
  const editable = Object.fromEntries(Object.entries(config).filter(([key]) => !['network_secret', 'credential_file', 'instance_id', 'secure_mode'].includes(key)))
  const text = editor(editable, 16)
  const node = select(managed.map(entry => [entry.node.id, `${entry.node.name} · ${entry.connected ? '在线' : '离线'}`]), '选择实际对应的 OPSD 节点')
  const content = panel('实例配置与关联', el('p', { class: 'muted' }, 'ET Web 是配置来源。保存只更新改动字段；网络凭据保持原值。'),
    el('div', { class: 'grid' }, el('div', {}, field('配置（JSON）', text), button('保存配置', async () => {
      const next = JSON.parse(text.value)
      const patch = Object.fromEntries(Object.entries(next).filter(([key, value]) => json(value) !== json(editable[key])))
      if (!Object.keys(patch).length) { report('配置没有变化'); return }
      await job('/config/update', { ...identity, patch })
      report('配置已保存并读回核对；运行实例的应用时机由现有 ET Web 管理。')
    }, 'primary')), el('div', {}, field('关联节点', node), button('核对接口并关联', async () => {
      if (!node.value) throw new Error('请选择对应的 OPSD 节点')
      await job('/bind', { ...identity, node_id: node.value })
      report('已核对节点实际接口，关联成功')
    }, 'primary'), el('p', { class: 'muted' }, '名称仅用于显示。关联需要 ET 地址、网段和本机接口一致。'),
    el('details', { open: true }, el('summary', {}, '直连、中继与运行信息'), el('pre', {}, json(info))))))
  content.id = 'instance-detail'
  detail.append(content)
}
refresh().catch(error => report(`ET Web 暂不可用：${error.message}。已知网络关联保持不变。`, true))
