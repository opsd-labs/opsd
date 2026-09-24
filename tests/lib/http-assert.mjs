/**
 * 共享 HTTP 契约校验：integration.mjs 与 negative-gates.mjs 共用。
 * 覆盖状态码、Content-Type、JSON Schema（Ajv）与纯文本框架错误。
 */
import { readFile } from 'node:fs/promises'
import { createRequire } from 'node:module'
import assert from 'node:assert/strict'
import path from 'node:path'

const require2 = createRequire(path.join(process.cwd(), 'web', 'package.json'))
const Ajv = require2('ajv')
const addFormats = require2('ajv-formats')
const { load: loadYaml } = require2('js-yaml')

const ROOT_ID = 'https://opsd.local/contract.json'

/** 解析 $ref（含 JSON Pointer ~0 / ~1 转义）。 */
export function deref(openapi, schema) {
  if (!schema || typeof schema !== 'object') return schema
  if (!schema.$ref) return schema
  const ref = String(schema.$ref)
  assert.ok(ref.startsWith('#/'), `只支持 OpenAPI 内部引用，收到 ${ref}`)
  const pointer = ref.startsWith('#') ? ref.slice(1) : ref.replace(/^.*#/, '')
  const parts = pointer
    .split('/')
    .filter((p) => p.length > 0)
    .map((p) => p.replace(/~1/g, '/').replace(/~0/g, '~'))
  let cur = openapi
  for (const part of parts) {
    assert.ok(cur && Object.hasOwn(cur, part), `无法解析 OpenAPI 引用 ${ref}`)
    cur = cur[part]
  }
  return cur
}

/** OpenAPI 3.1 → JSON Schema（Ajv 可编译）。 */
export function oasToAjvSchema(schema) {
  if (!schema || typeof schema !== 'object') return schema
  if (Array.isArray(schema)) return schema.map(oasToAjvSchema)
  const out = Object.fromEntries(Object.entries(schema).map(([key, value]) => [key, oasToAjvSchema(value)]))
  if (Array.isArray(out.type)) {
    const types = out.type.filter((x) => x !== 'null')
    const nullable = out.type.includes('null')
    if (nullable) {
      const { type: _t, ...rest } = out
      const inner = { ...rest, ...(types.length ? { type: types.length === 1 ? types[0] : types } : {}) }
      return { anyOf: [inner, { type: 'null' }] }
    }
    out.type = types.length === 1 ? types[0] : types
  }
  if (Array.isArray(out.oneOf)) {
    const nullBranches = out.oneOf.filter((branch) => branch && branch.type === 'null')
    const rest = out.oneOf.filter((branch) => !(branch && branch.type === 'null'))
    if (nullBranches.length === 1 && rest.length === 1) out.oneOf = [{ anyOf: [rest[0], { type: 'null' }] }]
  }
  return out
}

export function rewriteRefs(node) {
  if (!node || typeof node !== 'object') return node
  if (Array.isArray(node)) return node.map(rewriteRefs)
  const out = {}
  for (const [k, v] of Object.entries(node)) {
    if (k === '$ref' && typeof v === 'string') {
      out[k] = v.startsWith('#') ? ROOT_ID + v : v
    } else {
      out[k] = rewriteRefs(v)
    }
  }
  return out
}

function mediaTypeOf(value) {
  return String(value ?? '').split(';', 1)[0].trim().toLowerCase()
}

/** 加载契约并构建可复用校验器工厂。 */
export async function loadContract(contractPath) {
  const openapi = loadYaml(await readFile(contractPath, 'utf8'))
  const ajv = new Ajv({ allErrors: true, strict: false, validateSchema: false })
  addFormats(ajv)
  const rootSchema = {
    $id: ROOT_ID,
    components: {
      schemas: Object.fromEntries(
        Object.entries(openapi.components?.schemas ?? {}).map(([k, v]) => [
          k,
          rewriteRefs(oasToAjvSchema(v)),
        ]),
      ),
    },
  }
  ajv.addSchema(rootSchema)
  const cache = new Map()

  function assertSchema(value, schema, label) {
    const prepared = rewriteRefs(oasToAjvSchema(schema))
    const key = JSON.stringify(prepared)
    let validate = cache.get(key)
    if (!validate) {
      validate = ajv.compile(prepared)
      cache.set(key, validate)
    }
    if (!validate(value)) {
      throw new assert.AssertionError({
        message: `${label} 不符合契约 schema：${ajv.errorsText(validate.errors, { separator: '; ' })}`,
      })
    }
  }

  function responseSchema(route, method, status, mediaType = 'application/json') {
    const item = openapi.paths?.[route] ?? openapi.paths?.[route.replace(/^\//, '')]
    const op = item?.[method.toLowerCase()]
    let resp = deref(openapi, op?.responses?.[String(status)])
    const content = Object.entries(resp?.content ?? {}).find(([type]) => mediaTypeOf(type) === mediaTypeOf(mediaType))?.[1]
    assert.ok(content?.schema, `契约应描述 ${method.toUpperCase()} ${route} 的 ${status} ${mediaType} 响应`)
    return content.schema
  }

  /**
   * @param {{status:number, data:any, headers?:Record<string,string>}} res
   * @param {{status:number, route:string, method:string, kind?:'json'|'plain', schema?:object, contentType?:string}} spec
   */
  function assertHttpResponse(res, spec) {
    const { status, route, method, kind = 'json', schema, contentType } = spec
    const label = `${method.toUpperCase()} ${route} ${status}`
    assert.equal(res.status, status, `${label} 状态码不符，实际 ${res.status}`)
    if (contentType) {
      const ct = String(res.headers?.['content-type'] ?? res.contentType ?? '')
      assert.equal(mediaTypeOf(ct), mediaTypeOf(contentType), `${label} Content-Type 不符，应为 ${contentType}，实际 ${ct}`)
    }
    if (kind === 'plain' || kind === 'binary') {
      assert.equal(typeof res.data, 'string', `${label} 应为纯文本`)
    } else {
      assert.notEqual(typeof res.data, 'string', `${label} 应为 JSON 体`)
    }
    if (schema) assertSchema(res.data, schema, label)
  }

  return { openapi, assertSchema, responseSchema, assertHttpResponse, deref: (s) => deref(openapi, s) }
}
