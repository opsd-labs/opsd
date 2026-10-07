/**
 * OpenAPI 契约的代码生成与漂移检查。
 *
 * 契约源文件是 docs/api/openapi.yaml，生成物是 web/src/api/generated.ts。
 * 两者必须同步：改接口先改契约，再重新生成；CI 用 `node tools/api.mjs check`
 * 拦住「改了契约没重新生成」和「手改了生成物」两种情况。
 *
 * 用锁定版本的 openapi-typescript 的 Node API 直接生成，不经过子进程，
 * 因此生成结果与平台、shell 无关，漂移检查比较的是字节。
 */
import { readFile, writeFile, mkdir } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import openapiTS, { astToString } from 'openapi-typescript'

const here = dirname(fileURLToPath(import.meta.url))
const contract = resolve(here, '../docs/api/openapi.yaml')
const target = resolve(here, '../web/src/api/generated.ts')

/** 生成物抬头。刻意不含时间戳，否则漂移检查永远失败。 */
const banner = `/**
 * 本文件由 openapi-typescript 依据 docs/api/openapi.yaml 生成，请勿手工修改。
 * 重新生成：npm run api:generate --prefix tools
 * 校验一致性：npm run api:check --prefix tools
 */
`

async function render() {
  const ast = await openapiTS(new URL(`file://${contract.replace(/\\/g, '/')}`), {
    // 键序固定，保证同一份契约每次生成出同样的字节
    alphabetize: true
  })
  return banner + astToString(ast)
}

async function generate() {
  const text = await render()
  await mkdir(dirname(target), { recursive: true })
  await writeFile(target, text, 'utf8')
  const lines = text.split('\n').length
  console.log(`已生成 ${target}（${lines} 行）`)
}

async function check() {
  let committed
  try {
    committed = await readFile(target, 'utf8')
  } catch {
    console.error(`生成物不存在：${target}\n请先运行 npm run api:generate --prefix tools`)
    process.exit(1)
  }

  const expected = await render()
  if (committed === expected) {
    console.log(`契约与生成物一致：${target}`)
    return
  }

  const committedLines = committed.split('\n')
  const expectedLines = expected.split('\n')
  const firstDiff = committedLines.findIndex((line, i) => line !== expectedLines[i])
  console.error(
    '契约与生成物不一致：docs/api/openapi.yaml 与 web/src/api/generated.ts 已经漂移。\n' +
      `首个差异在第 ${firstDiff + 1} 行：\n` +
      `  生成物：${(committedLines[firstDiff] ?? '<无>').slice(0, 120)}\n` +
      `  期望值：${(expectedLines[firstDiff] ?? '<无>').slice(0, 120)}\n` +
      '若改动是契约本身，运行 npm run api:generate --prefix tools 提交新的生成物；' +
      '若你手改了生成物，请改契约后重新生成。'
  )
  process.exit(1)
}

const mode = process.argv[2]
if (mode === 'write') {
  await generate()
} else if (mode === 'check') {
  await check()
} else {
  console.error('用法：node tools/api.mjs <write|check>')
  process.exit(2)
}
