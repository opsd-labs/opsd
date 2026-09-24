/**
 * OpenAPI 契约的结构性门禁。
 *
 * 检查项：
 * 1. 唯一 WebSocket 端点必须 101，禁止 2xx/403；
 * 2. 分享机器接口必须 Bearer，禁止 security: []；
 * 3. 全部 JSON 写操作必须声明 400（业务 JSON + 框架纯文本）、415、422。
 *
 * 运行：node scripts/api-contract.mjs
 */
import { readFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { createRequire } from 'node:module'

const require = createRequire(import.meta.url)
const { load } = require('js-yaml')

const here = dirname(fileURLToPath(import.meta.url))
const contract = resolve(process.env.OPSD_CONTRACT_PATH || resolve(here, '../../docs/api/openapi.yaml'))

const doc = load(await readFile(contract, 'utf8'))
const failures = []

function fail(msg) {
  failures.push(msg)
}

/** 展开内部 $ref；支持 JSON Pointer 的 ~0 / ~1 转义。 */
function deref(obj, label = '引用') {
  if (!obj || typeof obj !== 'object' || !obj.$ref) return obj
  const ref = String(obj.$ref)
  if (!ref.startsWith('#/')) {
    fail(`${label} 使用不支持的外部引用 ${ref}`)
    return undefined
  }
  const parts = ref.slice(2).split('/').map((p) => p.replace(/~1/g, '/').replace(/~0/g, '~'))
  let cur = doc
  for (const part of parts) {
    if (!cur || typeof cur !== 'object' || !Object.hasOwn(cur, part)) {
      fail(`${label} 的 $ref 无法解析：${ref}`)
      return undefined
    }
    cur = cur[part]
  }
  return cur
}

function schemaIsString(schema, label) {
  const resolved = deref(schema, label)
  if (!resolved || typeof resolved !== 'object') return false
  if (resolved.type === 'string') return true
  if (Array.isArray(resolved.anyOf) || Array.isArray(resolved.oneOf)) {
    return [...(resolved.anyOf ?? []), ...(resolved.oneOf ?? [])].some((branch) => schemaIsString(branch, label))
  }
  return false
}

function requireMediaSchema(response, mediaType, label) {
  const resolved = deref(response, label)
  const schema = resolved?.content?.[mediaType]?.schema
  if (!schema) {
    fail(`${label} 必须声明 ${mediaType} schema`)
    return
  }
  if (mediaType === 'text/plain' && !schemaIsString(schema, `${label} ${mediaType}`)) {
    fail(`${label} ${mediaType} schema 必須是 string`)
  }
}

function checkRefs(value, label = 'OpenAPI') {
  if (!value || typeof value !== 'object') return
  if (Array.isArray(value)) {
    value.forEach((item, index) => checkRefs(item, `${label}[${index}]`))
    return
  }
  if (typeof value.$ref === 'string') deref(value, label)
  for (const [key, item] of Object.entries(value)) checkRefs(item, `${label}.${key}`)
}

checkRefs(doc)

/** 唯一声明 101 的操作视为 WebSocket 升级端点。 */
const wsOps = []
for (const [path, item] of Object.entries(doc.paths ?? {})) {
  for (const [method, op] of Object.entries(item ?? {})) {
    if (!op || typeof op !== 'object' || !op.responses) continue
    if (Object.prototype.hasOwnProperty.call(op.responses, '101')) {
      wsOps.push({ path, method, op })
    }
  }
}

if (wsOps.length !== 1) {
  fail(`应恰好有一个 WebSocket 101 端点，实际 ${wsOps.length} 个：${wsOps.map((x) => `${x.method.toUpperCase()} ${x.path}`).join(', ') || '（无）'}`)
} else {
  const { path, method, op } = wsOps[0]
  const codes = Object.keys(op.responses)
  if (method !== 'get') {
    fail(`WS 端点应是 GET，实际 ${method.toUpperCase()} ${path}`)
  }
  for (const code of codes) {
    if (code.startsWith('2') && code !== '101') {
      fail(`WS 端点 ${path} 不应声明 2xx 响应 ${code}`)
    }
  }
  if (!codes.includes('101')) {
    fail(`WS 端点 ${path} 必须保留 101 响应`)
  }
  if (codes.includes('403')) {
    fail(`WS 端点 ${path} 不应声明 403：握手失败在升级前是 400，不是 CSRF 的 403`)
  }
}

// 分享**机器接口**不得写 security: []。
for (const [path, item] of Object.entries(doc.paths ?? {})) {
  if (!path.includes('/share/{token}/api/v1/public/')) continue
  for (const [method, op] of Object.entries(item ?? {})) {
    if (!op || typeof op !== 'object') continue
    const sec = op.security ?? doc.security
    if (Array.isArray(sec) && sec.length === 0) {
      fail(`分享机器接口 ${method.toUpperCase()} ${path} 不得使用 security: []`)
    }
  }
}

// 全部 JSON 写操作必须声明框架拒绝 400 双 content / 415 / 422。
const jsonWriteOps = []
for (const [path, item] of Object.entries(doc.paths ?? {})) {
  for (const [method, op] of Object.entries(item ?? {})) {
    if (!op || typeof op !== 'object') continue
    if (!['post', 'put', 'patch', 'delete'].includes(method)) continue
    const body = deref(op.requestBody, `${method.toUpperCase()} ${path} requestBody`)
    const content = body?.content ?? {}
    // 字节流上传单独描述，不套 415 JSON
    if (content['application/zip'] || content['application/octet-stream']) continue
    if (!content['application/json']) continue
    jsonWriteOps.push({ path, method, op })
  }
}

if (jsonWriteOps.length === 0) {
  fail('未发现任何 JSON 写操作，枚举逻辑可能失效')
}

for (const { path, method, op } of jsonWriteOps) {
  const label = `${method.toUpperCase()} ${path}`
  const r400 = deref(op.responses?.['400'], `${label} 400`)
  if (!r400) {
    fail(`${label} 缺少 400`)
  } else {
    const c = r400?.content ?? {}
    if (!c['application/json']) {
      fail(`${label} 的 400 必须允许业务 JSON（application/json）`)
    } else if (!c['application/json'].schema) {
      fail(`${label} 的 400 application/json 必须声明 schema`)
    }
    if (!c['text/plain']) {
      fail(`${label} 的 400 必须允许框架纯文本（text/plain）`)
    } else if (!schemaIsString(c['text/plain'].schema, `${label} 400 text/plain`)) {
      fail(`${label} 的 400 text/plain schema 必须是 string`)
    }
  }
  if (!op.responses?.['415'] && !op.responses?.[415]) {
    fail(`${label} 缺少 415（Content-Type 不对时的框架纯文本）`)
  } else {
    requireMediaSchema(op.responses?.['415'] ?? op.responses?.[415], 'text/plain', `${label} 415`)
  }
  if (!op.responses?.['422'] && !op.responses?.[422]) {
    fail(`${label} 缺少 422（反序列化失败/未知字段的框架纯文本）`)
  } else {
    requireMediaSchema(op.responses?.['422'] ?? op.responses?.[422], 'text/plain', `${label} 422`)
  }
}

if (failures.length) {
  console.error('契约结构检查未通过：')
  for (const f of failures) console.error(`  - ${f}`)
  process.exit(1)
}
console.log(
  `契约结构检查通过：WS 101×1；分享机器接口无匿名 security；JSON 写操作 ${jsonWriteOps.length} 个均声明 400 双 content / 415 / 422`,
)
