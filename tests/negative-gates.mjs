/**
 * 负向门禁：故意破坏契约/请求，确认检查会失败。
 * 健康轮询启动；try/finally 恢复契约并清理进程；端口可配置。
 *
 * 运行：node tests/negative-gates.mjs
 */
import { readFile, writeFile, mkdtemp, rm } from 'node:fs/promises'
import { spawn } from 'node:child_process'
import path from 'node:path'
import os from 'node:os'
import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import { randomBytes } from 'node:crypto'
import https from 'node:https'
import http from 'node:http'
import net from 'node:net'
import { loadContract } from './lib/http-assert.mjs'

const root = process.cwd()
const require2 = createRequire(path.join(root, 'web', 'package.json'))
const { load } = require2('js-yaml')
const { dump } = require2('js-yaml')

const results = []
let hub
let runDir
const contractPath = path.join(root, 'docs', 'api', 'openapi.yaml')
let originalContract = ''

async function check(label, fn) {
  try {
    await fn()
  } catch (e) {
    results.push({ label, ok: false, detail: String(e.message || e).slice(0, 240) })
    console.error(`FAIL ${label}：${e.message || e}`)
    return
  }
  results.push({ label, ok: true, detail: '通过' })
  console.log(`PASS ${label}`)
}

async function withMutatedContract(mutate) {
  const doc = load(originalContract)
  mutate(doc)
  const file = path.join(runDir, `contract-${randomBytes(6).toString('hex')}.yaml`)
  await writeFile(file, dump(doc, { lineWidth: 120 }), 'utf8')
  return file
}

function run(cmd, args, cwd, env = process.env, timeoutMs = 10000) {
  return new Promise((resolve) => {
    const c = spawn(cmd, args, { cwd, env, shell: false, windowsHide: true })
    let out = ''
    let spawnError = null
    let timedOut = false
    const append = (chunk) => {
      out += chunk.toString()
      if (out.length > 20000) out = out.slice(-20000)
    }
    c.stdout.on('data', append)
    c.stderr.on('data', append)
    c.on('error', (error) => { spawnError = error })
    const timer = setTimeout(() => {
      timedOut = true
      c.kill()
    }, timeoutMs)
    c.on('close', (code, signal) => {
      clearTimeout(timer)
      resolve({ code, signal, out, spawnError, timedOut })
    })
  })
}

async function freePort() {
  return new Promise((resolve, reject) => {
    const server = net.createServer()
    server.once('error', reject)
    server.listen(0, '127.0.0.1', () => {
      const { port } = server.address()
      server.close((error) => error ? reject(error) : resolve(port))
    })
  })
}

async function expectGateFailure(label, contractFile, expected) {
  await check(label, async () => {
    const result = await run(node, [apiContract], root, { ...process.env, OPSD_CONTRACT_PATH: contractFile })
    assert.equal(result.spawnError, null, `檢查器未能启动：${result.spawnError}`)
    assert.equal(result.timedOut, false, '检查器超时')
    assert.notEqual(result.code, null, `检查器被信号终止：${result.signal}`)
    assert.notEqual(result.code, 0, `损坏契约意外通过：${result.out}`)
    assert.ok(result.out.includes(expected), `未出现预期门禁诊断「${expected}」：${result.out}`)
    assert.ok(!/Cannot find module|ERR_MODULE_NOT_FOUND|ENOENT|SyntaxError|TypeError:|ReferenceError:/.test(result.out), `检查器自身异常：${result.out}`)
  })
}

const node = process.execPath
const apiContract = path.join(root, 'web', 'scripts', 'api-contract.mjs')
let base
let healthPort
let agentPort

async function waitHealth(timeoutMs = 15000) {
  const start = Date.now()
  while (Date.now() - start < timeoutMs) {
    const ok = await new Promise((resolve) => {
      const req = http.request({ hostname: '127.0.0.1', port: healthPort, path: '/api/v1/health' }, (res) => {
        res.resume()
        resolve(res.statusCode === 200)
      })
      req.on('error', () => resolve(false))
      req.setTimeout(1000, () => {
        req.destroy()
        resolve(false)
      })
      req.end()
    })
    if (ok) return
    await new Promise((r) => setTimeout(r, 200))
  }
  throw new Error(`主控健康检查超时\n${logs.join('').slice(-3000)}`)
}

const logs = []
function launch(name, args) {
  const exe = path.join(root, 'target', 'debug', name + (process.platform === 'win32' ? '.exe' : ''))
  const c = spawn(exe, args, { cwd: root, windowsHide: true })
  c.stdout.on('data', (b) => logs.push(b.toString()))
  c.stderr.on('data', (b) => logs.push(b.toString()))
  return c
}

async function stop(c) {
  if (!c || c.exitCode !== null) return
  const done = new Promise((r) => c.on('exit', r))
  c.kill()
  await Promise.race([done, new Promise((r) => setTimeout(r, 2000))])
}

function raw(pathname, { method = 'POST', body, contentType } = {}) {
  return new Promise((resolve) => {
    const req = https.request(
      {
        hostname: 'localhost',
        port: base,
        path: `/${entrance}${pathname}`,
        method,
        ca,
        headers: {
          Origin: `https://localhost:${base}`,
          ...(body !== undefined
            ? { 'Content-Type': contentType || 'application/json', 'Content-Length': Buffer.byteLength(body) }
            : {}),
        },
      },
      (res) => {
        let text = ''
        res.on('data', (b) => (text += b))
        res.on('end', () => resolve({ status: res.statusCode, text, headers: res.headers }))
      },
    )
    req.on('error', (e) => resolve({ error: e.message }))
    req.setTimeout(5000, () => req.destroy())
    if (body !== undefined) req.write(body)
    req.end()
  })
}

let entrance = ''
let ca = null

try {
  originalContract = await readFile(contractPath, 'utf8')
  runDir = await mkdtemp(path.join(os.tmpdir(), 'opsd-neg-'))
  base = Number(process.env.OPSD_TEST_PORT || await freePort())
  healthPort = Number(process.env.OPSD_TEST_HEALTH_PORT || await freePort())
  agentPort = Number(process.env.OPSD_TEST_AGENT_PORT || await freePort())
  const hubDir = path.join(runDir, 'hub')
  const password = randomBytes(16).toString('hex')
  const passwordFile = path.join(runDir, 'password')
  await writeFile(passwordFile, password, { mode: 0o600 })
  const exe = (name) => path.join(root, 'target', 'debug', name + (process.platform === 'win32' ? '.exe' : ''))
  const initOut = await new Promise((resolve, reject) => {
    const c = spawn(exe('opsd-hub'), ['--data-dir', hubDir, 'init', '--password-file', passwordFile], {
      cwd: root,
      windowsHide: true,
    })
    let o = ''
    c.stdout.on('data', (b) => (o += b))
    c.stderr.on('data', (b) => (o += b))
    c.on('exit', (code) => (code === 0 ? resolve(o) : reject(new Error(o))))
  })
  entrance = initOut.match(/控制台安全入口：([A-Za-z0-9\-._~]{16})/)?.[1]
  assert.ok(entrance)
  ca = await readFile(path.join(hubDir, 'pki', 'ca.pem'))

  // --- 契约结构负向（6+3） ---
  const scenarios = [
    ['删 WS 101 应失败', (doc) => delete doc.paths['/api/v1/nodes/{id}/stream'].get.responses['101'], '101'],
    ['分享机器接口匿名应失败', (doc) => { doc.paths['/share/{token}/api/v1/public/summary'].get.security = [] }, '不得使用 security: []'],
    ['删 422 应失败', (doc) => delete doc.paths['/api/v1/auth/login'].post.responses['422'], '缺少 422'],
    ['删 400 text/plain 应失败', (doc) => delete doc.paths['/api/v1/auth/login'].post.responses['400'].content['text/plain'], '400 必须允许框架纯文本'],
    ['415 错误响应形状应失败', (doc) => { doc.components.responses.UnsupportedMediaType.content = { 'application/json': { schema: { type: 'object' } } } }, '必须声明 text/plain'],
    ['422 错误响应形状应失败', (doc) => { doc.components.responses.UnprocessableEntity.content = { 'application/json': { schema: { type: 'object' } } } }, '必须声明 text/plain'],
    ['破坏 $ref JSON Pointer 应失败', (doc) => { doc.paths['/api/v1/auth/login'].post.responses['200'].content['application/json'].schema = { $ref: '#/components/schemas/NoSuch~1Thing' } }, '无法解析'],
  ]
  for (const [label, mutate, expected] of scenarios) {
    const altered = await withMutatedContract(mutate)
    await expectGateFailure(label, altered, expected)
  }
  await check('契约故障注入不改写正式文件', async () => {
    assert.equal(await readFile(contractPath, 'utf8'), originalContract)
  })

  await check('响应校验拒绝错误媒体类型', async () => {
    const { assertHttpResponse } = await loadContract(contractPath)
    assert.throws(() => assertHttpResponse(
      { status: 200, data: {}, headers: { 'content-type': 'application/jsonx' } },
      { status: 200, route: '/api/v1/auth/login', method: 'post', contentType: 'application/json' },
    ), /Content-Type 不符/)
    assert.doesNotThrow(() => assertHttpResponse(
      { status: 200, data: {}, headers: { 'content-type': 'application/json; charset=utf-8' } },
      { status: 200, route: '/api/v1/auth/login', method: 'post', contentType: 'application/json' },
    ))
  })
  await check('响应校验匹配纯文本 schema', async () => {
    const { assertHttpResponse, responseSchema } = await loadContract(contractPath)
    assert.throws(() => assertHttpResponse(
      { status: 415, data: 'wrong', headers: { 'content-type': 'text/plain; charset=utf-8' } },
      { status: 415, route: '/api/v1/auth/login', method: 'post', kind: 'plain', schema: { type: 'number' }, contentType: 'text/plain' },
    ), /不符合契约 schema/)
    assert.deepEqual(responseSchema('/api/v1/auth/login', 'post', 422, 'text/plain'), { type: 'string' })
  })
  await check('缺少响应媒体类型时无法取得 schema', async () => {
    const altered = await withMutatedContract((doc) => {
      delete doc.paths['/api/v1/auth/login'].post.responses['200'].content['application/json']
    })
    const broken = await loadContract(altered)
    assert.throws(() => broken.responseSchema('/api/v1/auth/login', 'post', 200), /契约应描述/)
  })

  // --- 运行时负向 ---
  hub = launch('opsd-hub', [
    '--data-dir',
    hubDir,
    'serve',
    '--listen',
    `127.0.0.1:${base}`,
    '--agent-listen',
    `127.0.0.1:${agentPort}`,
    '--health-listen',
    `127.0.0.1:${healthPort}`,
    '--origin',
    `https://localhost:${base}`,
  ])
  await waitHealth()

  // 运行时：服务端/校验器必须拒绝 —— 这里是正向断言，不是 expectFail
  {
    const label = '未知请求字段应 422'
    const r = await raw('/api/v1/auth/login', { body: JSON.stringify({ password, nope: 1 }) })
    if (r.status === 422) {
      results.push({ label, ok: true, detail: '422 纯文本' })
      console.log(`PASS ${label}`)
    } else {
      results.push({ label, ok: false, detail: `got ${r.status} ${r.text}` })
      console.error(`FAIL ${label}`, r.status, r.text)
    }
  }
  {
    const label = '错误 Content-Type 应 415'
    const r = await raw('/api/v1/auth/login', { body: JSON.stringify({ password }), contentType: 'text/plain' })
    if (r.status === 415) {
      results.push({ label, ok: true, detail: '415 纯文本' })
      console.log(`PASS ${label}`)
    } else {
      results.push({ label, ok: false, detail: `got ${r.status} ${r.text}` })
      console.error(`FAIL ${label}`, r.status, r.text)
    }
  }
  {
    const label = 'schema 外字段应失败'
    const login = await raw('/api/v1/auth/login', { body: JSON.stringify({ password }) })
    const { assertSchema, responseSchema } = await loadContract(contractPath)
    const captured = JSON.parse(login.text)
    const poisoned = { ...captured, __injected__: true }
    let rejected = false
    try {
      assertSchema(poisoned, responseSchema('/api/v1/auth/login', 'post', 200), '登录响应副本+额外字段')
    } catch {
      rejected = true
    }
    if (rejected) {
      results.push({ label, ok: true, detail: '校验器拒绝响应副本额外字段' })
      console.log(`PASS ${label}`)
    } else {
      results.push({ label, ok: false, detail: '校验器未拒绝' })
      console.error(`FAIL ${label}`)
    }
  }
} finally {
  await stop(hub)
  if (originalContract) await writeFile(contractPath, originalContract, 'utf8')
  if (runDir) await rm(runDir, { recursive: true, force: true }).catch(() => {})
}

const failed = results.filter((r) => !r.ok)
console.log(`\n负向验证：${results.length - failed.length}/${results.length} 按预期失败`)
if (logs.length) console.log('--- 主控日志尾 ---\n' + logs.join('').slice(-800))
if (failed.length) {
  for (const f of failed) console.error(' -', f.label, f.detail)
  process.exit(1)
}
