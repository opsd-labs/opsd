import { api, core, nodes, el, page, panel, button, table, field, select, input, editor, report, json, job, nativeLink, terminal } from './ui.js'

const main = page('Docker / Portainer', '通过 Portainer 管理多台 Docker Engine，已有容器保持原有身份和挂载。')
const node = select([])
const content = el('div')
const output = el('div')
const wizard = el('details', {}, el('summary', {}, '安装或接入 Portainer'))
const tabs = el('div', { class: 'row' })
main.append(panel('环境', el('div', { class: 'row' }, field('节点环境', node), button('刷新', refresh))), wizard, tabs, content, output)
let inventory = {}
let section = 'containers'
let entries = []
let socket
let closeTerminal = () => {}
for (const [key, label] of [['containers', '容器'], ['stacks', 'Stack'], ['images', '镜像'], ['networks', '网络'], ['volumes', '卷']]) tabs.append(button(label, () => { section = key; render() }))
node.onchange = refresh

function requireNode() { if (!node.value) throw new Error('请先选择已接入的节点环境'); return node.value }
async function mutate(body) { await job('/mutate', { node_id: requireNode(), ...body }); await refresh() }
function show(title, body) { closeTerminal(); output.replaceChildren(panel(title, body, button('收起', () => { output.replaceChildren(); closeTerminal() }))) }
async function refresh() {
  if (!node.value) { report('安装向导可接入新的节点环境'); return }
  inventory = await api('/inventory', 'POST', { node_id: requireNode() })
  render()
  report('已读取当前 Portainer 环境')
}
function confirmRemove(message) { return confirm(message) }
function render() {
  content.replaceChildren()
  if (section === 'containers') {
    const rows = (inventory.containers || []).map(item => {
      const owned = item.Labels?.['io.opsd.plugin']
      const terminalAction = button('终端', () => openTerminal(item.Id))
      terminalAction.disabled = !!owned
      return [item.Names?.join(', ') || item.Id.slice(0, 12), item.Image, item.Status,
        el('div', { class: 'row' }, ...['start', 'stop', 'restart'].map((operation, index) => {
          const action = button(['启动', '停止', '重启'][index], () => mutate({ resource: 'container', operation, id: item.Id }))
          action.disabled = !!owned
          return action
        }), button('日志', async () => { const logs = await api('/logs', 'POST', { node_id: requireNode(), container: item.Id }); show('最近 300 行日志', el('pre', {}, logs.text)) }),
        terminalAction, button('详情', async () => show('容器详情', el('pre', {}, json(await api('/detail', 'POST', { node_id: requireNode(), resource: 'container', id: item.Id }))))),
        owned ? el('span', { class: 'tag' }, `${owned} 插件服务`) : button('删除', async () => {
          if (confirmRemove('删除已停止的容器？数据卷会保留。')) await mutate({ resource: 'container', operation: 'remove', id: item.Id, confirmed: true })
        }, 'danger'))]
    })
    const name = input('容器名称')
    const spec = editor({ Image: 'nginx:stable', HostConfig: { RestartPolicy: { Name: 'unless-stopped' } } })
    content.append(panel('容器', table(['名称', '镜像', '状态', '操作'], rows)), el('details', {}, el('summary', {}, '创建容器'), panel('创建容器', field('名称', name), field('Docker 容器参数（JSON）', spec), button('创建', () => mutate({ resource: 'container', operation: 'create', name: name.value, spec: JSON.parse(spec.value) }), 'primary'))))
  } else if (section === 'stacks') {
    const rows = (inventory.stacks || []).map(stack => [stack.Name, stack.Status === 1 ? '运行中' : '已停止', el('div', { class: 'row' },
      button('编辑', async () => { const file = await api('/detail', 'POST', { node_id: requireNode(), resource: 'stack', id: String(stack.Id) }); stackEditor(stack, file.StackFileContent || '') }),
      ...['start', 'stop'].map((operation, i) => button(['启动', '停止'][i], () => mutate({ resource: 'stack', operation, stack_id: stack.Id }))),
      button('删除', async () => { if (confirmRemove('删除此 Stack 及其容器？持久卷由 Portainer 按项目配置处理。')) await mutate({ resource: 'stack', operation: 'remove', stack_id: stack.Id }) }, 'danger'))])
    content.append(panel('Stack', el('p', { class: 'muted' }, '此处列出 Portainer 已登记的 Stack。已有 Compose 项目不会从容器列表反推后自动接管。'), table(['项目', '状态', '操作'], rows), button('新建 Stack', () => stackEditor(null, 'services:\n  app:\n    image: nginx:stable\n'))))
  } else if (section === 'images') {
    const reference = input('例如 nginx:stable')
    content.append(panel('镜像', el('div', { class: 'row' }, reference, button('拉取镜像', () => mutate({ resource: 'image', operation: 'pull', reference: reference.value }), 'primary')), table(['引用', '大小', '操作'], (inventory.images || []).map(image => [image.RepoTags?.join(', ') || image.Id.slice(0, 20), `${Math.round(image.Size / 1048576)} MiB`, button('删除', async () => { if (confirmRemove('删除未被容器使用的镜像？')) await mutate({ resource: 'image', operation: 'remove', id: image.Id.replace(/^sha256:/, '') }) }, 'danger')]))))
  } else {
    const volume = section === 'volumes'
    const list = volume ? inventory.volumes?.Volumes || [] : inventory.networks || []
    const spec = editor(volume ? { Name: 'app-data' } : { Name: 'app-network', Driver: 'bridge' }, 5)
    content.append(panel(volume ? '数据卷' : '网络', table(['名称', '驱动', '操作'], list.map(item => [item.Name, item.Driver, button('删除', async () => { if (confirmRemove(`删除${volume ? '数据卷会永久移除卷中数据' : '网络'}，确认继续？`)) await mutate({ resource: volume ? 'volume' : 'network', operation: 'remove', id: item.Name }) }, 'danger')])), field('创建参数（JSON）', spec), button('创建', () => mutate({ resource: volume ? 'volume' : 'network', operation: 'create', spec: JSON.parse(spec.value) }), 'primary')))
  }
}
function stackEditor(stack, initial) {
  const name = input('项目名称', 'text', stack?.Name || '')
  name.disabled = !!stack
  const text = editor(initial, 18)
  show(stack ? '编辑 Stack' : '创建 Stack', el('div', {}, field('名称', name), field('Compose 配置', text), button('提交并等待执行', async () => {
    await mutate({ resource: 'stack', operation: stack ? 'update' : 'create', stack_id: stack?.Id, name: name.value, content: text.value })
    output.replaceChildren()
  }, 'primary')))
}
async function openTerminal(container) {
  closeTerminal()
  const display = el('div', { class: 'terminal', 'aria-label': '容器交互终端', style: 'height:360px;padding:12px' })
  const command = select([['/bin/sh', '/bin/sh'], ['/bin/bash', '/bin/bash']])
  command.value = '/bin/sh'
  let session
  let fit
  let execId = ''
  const resize = () => { if (session && execId) api('/terminal/resize', 'POST', { node_id: requireNode(), exec_id: execId, rows: session.rows, cols: session.cols }).catch(error => report(error.message, true)) }
  show('容器终端', el('div', {}, el('div', { class: 'row' }, command, button('连接', async () => {
    socket?.close()
    session?.dispose()
    display.replaceChildren()
    session = new window.Terminal({ cursorBlink: true, fontSize: 13, scrollback: 2000, theme: { background: '#111c2d', foreground: '#d6e4f5' } })
    fit = new window.FitAddon.FitAddon()
    session.loadAddon(fit)
    session.open(display)
    fit.fit()
    socket = await terminal({ node_id: requireNode(), container, command: [command.value] }, text => {
      try { const message = JSON.parse(text); if (message.type === 'ready') { execId = message.exec_id; resize(); session.focus(); return } if (message.error) { report(message.error, true); return } } catch { /* 终端输出通常不是 JSON。 */ }
      session.write(text)
    })
    session.onData(data => { if (socket?.readyState === WebSocket.OPEN) socket.send(data) })
    session.onResize(resize)
  }), button('发送 Ctrl+C', () => socket?.send('\u0003')), button('断开', () => socket?.close())), display))
  const observer = new ResizeObserver(() => fit?.fit())
  observer.observe(display)
  closeTerminal = () => { observer.disconnect(); socket?.close(); session?.dispose(); session=undefined; execId=''; closeTerminal=()=>{} }

}
async function initialize() {
  entries = await nodes()
  const checklist = el('div', { class: 'checklist' })
  for (const entry of entries) {
    const check = el('input', { type: 'checkbox', value: entry.node.id })
    checklist.append(el('label', {}, check, entry.node.name))
  }
  const engine = el('input', { type: 'checkbox' })
  wizard.append(panel('接入向导', el('p', { class: 'muted' }, '先在节点页面手动接入 Hub 宿主机 Agent，再关联 ET 实例并指定控制机。Server 部署在控制机，其余节点使用普通 Agent。'), checklist,
    el('label', { class: 'row' }, engine, '缺少 Docker 时允许安装 Engine 与 Compose（已有安装直接复用）'),
    button('安装并接入所选节点', async () => {
      await job('/install', { nodes: [...checklist.querySelectorAll('input:checked')].map(input => input.value), allow_engine_install: engine.checked })
      await loadEnvironments()
    }, 'primary')))
  await loadEnvironments()
  const console = await api('/console')
  main.querySelector('header').append(nativeLink(console.url, '打开 Portainer 管理台 ↗'))
}
async function loadEnvironments() {
  const response = await api('/environments')
  const bindings = response.bindings || []
  node.replaceChildren(el('option', { value: '' }, '选择节点环境'), ...bindings.map(binding => el('option', { value: binding.node_id }, entries.find(entry => entry.node.id === binding.node_id)?.node.name || binding.node_id)))
  if (bindings.length) { node.value = bindings[0].node_id; await refresh() }
}
window.addEventListener('pagehide', () => closeTerminal())
initialize().catch(error => report(error.message, true))
