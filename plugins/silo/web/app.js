import { api, nodes, el, page, panel, button, table, field, select, input, report, json, job, nativeLink } from './ui.js'

const main = page('Silo 分布式存储', '明确选择数据盘或已有目录，经 Portainer 编排集群，再等待所有成员收敛。')
const clusters = el('div')
const wizard = el('details', {}, el('summary', {}, '部署新集群'))
const tools = el('div')
main.append(el('div', { class: 'row' }, button('刷新集群', refresh)), clusters, wizard, tools)
let entries = []
let activeCluster

async function refresh() {
  const response = await api('/clusters')
  clusters.replaceChildren(...response.clusters.filter(Boolean).map(cluster => panel(cluster.name,
    el('div', { class: 'row' }, el('span', { class: 'tag' }, cluster.phase || '等待准备'),
      cluster.console_url ? nativeLink(cluster.console_url, '打开 Silo 管理台 ↗') : null,
      button('进度', () => showProgress(cluster)),
      button('读取管理状态', async () => { const status = await job('/status', { cluster: cluster.name }); tools.replaceChildren(panel('管理状态', el('pre', {}, json(status)))) }),
      cluster.phase === 'ready' ? button('桶与用户', () => manage(cluster)) : button('核实后恢复', async () => {
        if (!confirm('恢复将先核对已准备目录、容器镜像、挂载和 ET 绑定，再继续剩余步骤。确认继续？')) return
        await job('/install', { name: cluster.name, resume: true })
        await refresh()
      }, 'primary')),
    cluster.last_error ? el('p', { class: 'danger-note' }, cluster.last_error) : null,
    el('p', { class: 'muted' }, `${Object.keys(cluster.prepared || {}).length} 个节点已准备 · ${Object.keys(cluster.containers || {}).length} 个成员容器已创建`))))
  if (!response.clusters.length) clusters.append(panel('集群', el('p', { class: 'muted' }, '尚未部署集群。请先在插件管理中配置管理用户与密码，并通过 Docker 插件接入节点。')))
}
function showProgress(cluster) {
  tools.replaceChildren(panel(`${cluster.name} / 部署进度`, table(['节点', '目录', '容器'], (cluster.spec?.members || []).map(member => [
    entries.find(entry => entry.node.id === member.node_id)?.node.name || member.node_id,
    cluster.prepared?.[member.node_id]?.mounts?.join(', ') || '未准备', cluster.containers?.[member.node_id] || '未创建',
  ])), el('p', { class: 'muted' }, '准备全部节点后创建所有成员，最后统一等待集群健康；失败进度会持久保存。')))
}
function manage(cluster) {
  activeCluster = cluster
  const bucket = input('桶名称')
  const user = input('访问用户名称')
  const secret = input('在插件秘密存储中登记的名称')
  const policy = select([['readonly', '只读'], ['readwrite', '读写'], ['writeonly', '只写']])
  policy.value = 'readonly'
  const result = el('pre', {}, '操作结果显示在这里')
  const run = async (path, body) => { result.textContent = json(await job(path, { cluster: activeCluster.name, ...body })) }
  tools.replaceChildren(el('div', { class: 'grid' },
    panel(`${cluster.name} / 桶`, field('名称', bucket), el('div', { class: 'row' }, button('列出桶', () => run('/bucket', { operation: 'list' })),
      button('创建', () => run('/bucket', { operation: 'create', bucket: bucket.value }), 'primary'), button('删除空桶', async () => {
        if (confirm(`删除空桶 ${bucket.value}？`)) await run('/bucket', { operation: 'remove', bucket: bucket.value })
      }, 'danger'))),
    panel('访问用户', field('用户名', user), field('访问密钥引用', secret), field('权限', policy), el('div', { class: 'row' },
      button('列出用户', () => run('/user', { operation: 'list' })), button('创建并绑定策略', () => run('/user', { operation: 'create', user: user.value, secret_name: secret.value, policy: policy.value }), 'primary'),
      button('删除用户', async () => { if (confirm(`删除用户 ${user.value}？`)) await run('/user', { operation: 'remove', user: user.value }) }, 'danger')))), panel('结果', result))
}
async function initialize() {
  entries = await nodes()
  const name = input('例如 objects')
  const members = el('div')
  const node = select(entries.filter(entry => entry.node.capabilities?.plugins && entry.node.capabilities?.execution_mode === 'host').map(entry => [entry.node.id, entry.node.name]))
  const selections = new Map()
  const confirmed = el('input', { type: 'checkbox' })
  wizard.append(panel('部署向导', field('集群名称', name), el('div', { class: 'row' }, field('添加成员', node), button('盘点并选择数据目录', async () => {
    if (!node.value) throw new Error('请选择节点')
    if (selections.has(node.value)) throw new Error('此节点已在向导中')
    const id = node.value
    const inventory = await job('/inventory', { node_id: id })
    addMember(id, inventory)
  })), members,
  el('label', { class: 'row danger-note' }, confirmed, '我已核对下列新盘设备与容量，确认允许清空所选新盘。已有路径不格式化。'),
  button('准备全部节点并部署集群', async () => {
    if (selections.size < 4) throw new Error('首批分布式部署至少选择四个节点')
    const selected = [...selections].map(([node_id, value]) => ({ node_id, storage: value.read() }))
    const destructive = selected.flatMap(member => member.storage.filter(storage => storage.mode === 'new').map(storage => `${member.node_id}: ${storage.device} (${storage.expected_size} 字节) → ${storage.mount}`))
    if (destructive.length && !confirmed.checked) throw new Error('新盘清空需要明确确认')
    const message = destructive.length ? `将清空以下设备：\n${destructive.join('\n')}\n确认执行？` : '使用所选已有目录，不格式化，确认部署？'
    if (!confirm(message)) return
    await job('/install', { name: name.value, members: selected, acknowledge_destructive: confirmed.checked })
    await refresh()
  }, 'primary')))

  function addMember(id, inventory) {
    const entriesBox = el('div')
    const rows = []
    const member = panel(entries.find(entry => entry.node.id === id)?.node.name || id,
      inventory.gaps?.length ? el('p', { class: 'danger-note' }, `盘点缺口：${inventory.gaps.join('；')}`) : null,
      table(['设备', '容量', '序列号', '使用情况'], (inventory.devices || []).filter(disk => disk.kind === 'disk').map(disk => [disk.path, `${(disk.size / 1073741824).toFixed(1)} GiB`, disk.serial || '—', inventory.risks?.find(risk => risk.device === disk.name)?.reasons.join('；') || '未发现使用痕迹'])),
      entriesBox, el('div', { class: 'row' }, button('添加新盘选择', () => addSelection('new')), button('添加已有路径', () => addSelection('existing')), button('移除此节点', () => { selections.delete(id); member.remove() })))
    members.append(member)
    selections.set(id, { read() { if (!rows.length) throw new Error('每个成员至少选择一个目录'); return rows.map(row => row()) } })
    function addSelection(mode) {
      const box = el('div', { class: 'storage-selection' })
      let read
      if (mode === 'new') {
        const candidates = (inventory.devices || []).filter(disk => disk.kind === 'disk' && inventory.risks?.some(risk => risk.device === disk.name && !Object.entries(risk).some(([key, value]) => key !== 'device' && key !== 'reasons' && value === true)))
        const disk = select(candidates.map(disk => [disk.path, `${disk.path} · ${(disk.size / 1073741824).toFixed(1)} GiB`]), '明确选择将被清空的磁盘')
        const mount = input('/data/disk1')
        box.append(field('新盘（将清空）', disk), field('挂载路径', mount))
        read = () => { const device = candidates.find(item => item.path === disk.value); if (!device) throw new Error('请选择具体新盘'); if (!mount.value) throw new Error('请填写挂载路径'); return { mode, device: device.path, expected_size: device.size, serial: device.serial, mount: mount.value } }
      } else {
        const path = input('/srv/silo/data')
        box.append(field('已有路径（不格式化）', path))
        read = () => { if (!path.value) throw new Error('请填写已有目录路径'); return { mode, path: path.value } }
      }
      rows.push(read)
      box.append(button('移除本项', () => { rows.splice(rows.indexOf(read), 1); box.remove() }))
      entriesBox.append(box)
    }
  }
  await refresh()
}
initialize().catch(error => report(error.message, true))
