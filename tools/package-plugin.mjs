import { readFile, copyFile, mkdir, mkdtemp, stat, chmod, rm } from 'node:fs/promises'
import { resolve, join, dirname } from 'node:path'
import { tmpdir } from 'node:os'
import { execFileSync } from 'node:child_process'

const [id, binaries = '.release/plugin-binaries', output = '.release/plugins'] = process.argv.slice(2)
if (!id || !/^[a-z][a-z0-9-]*$/.test(id)) throw new Error('用法：node tools/package-plugin.mjs <插件编号> [程序目录] [输出目录]')
const root = resolve(import.meta.dirname, '..')
const source = join(root, 'plugins', id)
const manifest = JSON.parse(await readFile(join(source, 'plugin.json'), 'utf8'))
const target = resolve(output, `${manifest.id}-${manifest.version}.zip`)
await mkdir(dirname(target), { recursive: true })
const staging = await mkdtemp(join(tmpdir(), `opsd-plugin-${id}-`))
try {
  for (const file of ['plugin.json', 'api.json']) await copyFile(join(source, file), join(staging, file))
  let count = 0
  for (const arch of ['amd64', 'arm64']) {
    const binary = resolve(binaries, `linux-${arch}`, `opsd-plugin-${id}`)
    if (!await stat(binary).catch(() => null)) continue
    const folder = join(staging, `linux-${arch}`)
    await mkdir(folder)
    await copyFile(binary, join(folder, 'plugin'))
    await chmod(join(folder, 'plugin'), 0o500)
    count++
  }
  if (!count) throw new Error('没有可打包的 Linux 程序')
  execFileSync('zip', ['-q', '-r', target, '.'], { cwd: staging, stdio: 'inherit' })
  console.log(`已打包 ${manifest.id} ${manifest.version}：${target}`)
} finally {
  await rm(staging, { recursive: true, force: true })
}
