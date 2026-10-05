/**
 * 按操作补齐 JSON 写接口的 400 双 content / 415 / 422。
 * 一次性工具脚本；路径相对本文件解析。
 */
import { readFile, writeFile } from 'node:fs/promises'
import { fileURLToPath } from 'node:url'
import { createRequire } from 'node:module'
const require = createRequire(import.meta.url)
const { load, dump } = require('js-yaml')

const path = fileURLToPath(new URL('../../docs/api/openapi.yaml', import.meta.url))
const doc = load(await readFile(path, 'utf8'))

function isJsonWrite(op) {
  if (!op || typeof op !== 'object') return false
  const content = op.requestBody?.content ?? {}
  if (content['application/zip'] || content['application/octet-stream']) return false
  return Boolean(content['application/json'])
}

function ensure(op) {
  op.responses = op.responses ?? {}
  let r400 = op.responses['400']
  if (!r400 || r400.$ref) {
    op.responses['400'] = {
      description: '业务校验失败 {"error"}；JSON 语法错误为框架纯文本。',
      content: {
        'application/json': { schema: { $ref: '#/components/schemas/Error' } },
        'text/plain': { schema: { type: 'string' } },
      },
    }
  } else {
    r400.content = r400.content ?? {}
    r400.content['application/json'] ??= { schema: { $ref: '#/components/schemas/Error' } }
    r400.content['text/plain'] ??= { schema: { type: 'string' } }
  }
  op.responses['415'] ??= { $ref: '#/components/responses/UnsupportedMediaType' }
  op.responses['422'] ??= { $ref: '#/components/responses/UnprocessableEntity' }
}

let n = 0
for (const item of Object.values(doc.paths ?? {})) {
  for (const [method, op] of Object.entries(item ?? {})) {
    if (!['post', 'put', 'patch', 'delete'].includes(method)) continue
    if (!isJsonWrite(op)) continue
    ensure(op)
    n++
  }
}
await writeFile(path, dump(doc, { lineWidth: 120 }), 'utf8')
console.log(`patched ${n} JSON write ops`)
