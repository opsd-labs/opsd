const parts = location.pathname.split('/').filter(Boolean)
export const entrance = '/' + parts[0]
export const plugin = parts[2]
let csrf = ''

export async function core(path, method = 'GET', body) {
  if (method !== 'GET' && !csrf) csrf = (await core('/api/v1/auth/me')).csrf
  const headers = new Headers()
  if (body !== undefined) headers.set('Content-Type', 'application/json')
  if (method !== 'GET') headers.set('X-CSRF-Token', csrf)
  const response = await fetch(entrance + path, { method, headers, body: body === undefined ? undefined : JSON.stringify(body) })
  const result = await response.json().catch(() => ({ error: `请求失败（${response.status}）` }))
  if (!response.ok) throw new Error(result.error || `请求失败（${response.status}）`)
  return result
}
export const api = (path, method = 'GET', body) => core(`/api/v1/plugins/${plugin}/api${path}`, method, body)

export function el(tag, attrs = {}, ...children) {
  const item = document.createElement(tag)
  for (const [key, value] of Object.entries(attrs)) {
    if (key.startsWith('on')) item.addEventListener(key.slice(2).toLowerCase(), value)
    else if (key === 'class') item.className = value
    else if (key in item) item[key] = value
    else item.setAttribute(key, value)
  }
  for (const child of children.flat(Infinity)) if (child !== undefined && child !== null) item.append(child instanceof Node ? child : document.createTextNode(String(child)))
  return item
}
export function button(label, action, kind = '') {
  return el('button', { class: kind, type: 'button', onClick: async event => {
    const target = event.currentTarget
    target.disabled = true
    try { await action() } catch (error) { report(error.message, true) } finally { target.disabled = false }
  } }, label)
}
export const field = (label, input) => el('label', { class: 'field' }, el('span', {}, label), input)
export const input = (placeholder = '', type = 'text', value = '') => el('input', { placeholder, type, value })
export const json = value => JSON.stringify(value, null, 2)
export const editor = (value = {}, rows = 9) => el('textarea', { rows, value: typeof value === 'string' ? value : json(value), spellcheck: false })
export const panel = (title, ...body) => el('section', { class: 'panel' }, el('h2', {}, title), ...body)
export function table(columns, rows) {
  return el('div', { class: 'table-scroll' }, el('table', {}, el('thead', {}, el('tr', {}, columns.map(title => el('th', {}, title)))),
    el('tbody', {}, rows.length ? rows.map(row => el('tr', {}, row.map(value => el('td', {}, value)))) : el('tr', {}, el('td', { colSpan: columns.length, class: 'muted' }, '暂无数据')))))
}
export function report(text, failed = false) {
  const region = document.querySelector('#notice')
  region.className = failed ? 'notice error' : 'notice'
  region.textContent = text
}
export function page(title, description) {
  const main = el('main', {}, el('header', {}, el('div', {}, el('p', { class: 'eyebrow' }, 'OPSD / 独立插件'), el('h1', {}, title), el('p', { class: 'muted' }, description))))
  document.body.replaceChildren(main)
  main.append(el('p', { id: 'notice', role: 'status', class: 'notice', 'aria-live': 'polite' }))
  return main
}
export async function nodes() {
  return (await core('/api/v1/nodes')).filter(entry => !entry.node.revoked)
}
export function select(options, placeholder = '请选择') {
  return el('select', {}, el('option', { value: '' }, placeholder), options.map(([value, text]) => el('option', { value }, text)))
}
export function nativeLink(url, label) {
  const address = new URL(url)
  if (!['http:', 'https:'].includes(address.protocol)) throw new Error('管理地址无效')
  return el('a', { class: 'button', href: address.href, target: '_blank', rel: 'noopener noreferrer' }, label)
}
export async function job(path, body, onTask) {
  const accepted = await api(path, 'POST', { ...body, idempotency_key: crypto.randomUUID() })
  report(`任务已受理：${accepted.task_id}，等待实际执行`)
  while (true) {
    const task = await core(`/api/v1/tasks/${encodeURIComponent(accepted.task_id)}`)
    onTask?.(task)
    if (task.status === 'succeeded') { report('任务执行成功'); return task.result }
    if (['failed', 'uncertain', 'blocked', 'cancelled', 'rolled_back'].includes(task.status)) throw new Error(`${task.status === 'uncertain' ? '结果待核实' : '执行未成功'}：${task.error || task.status}（${task.id}）`)
    await new Promise(resolve => setTimeout(resolve, 1500))
  }
}
export async function terminal(body, receive) {
  const session = await core('/api/v1/auth/me')
  const url = new URL(`${entrance}/api/v1/plugins/${plugin}/stream/terminal`, location.origin)
  url.protocol = 'wss:'
  url.searchParams.set('csrf', session.csrf)
  const socket = new WebSocket(url)
  socket.binaryType = 'arraybuffer'
  socket.onopen = () => socket.send(JSON.stringify(body))
  socket.onmessage = event => receive(typeof event.data === 'string' ? event.data : new TextDecoder().decode(event.data))
  socket.onerror = () => report('终端连接失败', true)
  socket.onclose = () => report('终端已断开')
  window.addEventListener('pagehide', () => socket.close(), { once: true })
  return socket
}
