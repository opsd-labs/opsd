/** 内置控制台、插件页面与真实 Agent 的浏览器验收。 */
import { spawn } from "node:child_process";
import { mkdir, writeFile, readFile, rm } from "node:fs/promises";
import path from "node:path";
import https from "node:https";
import { randomBytes } from "node:crypto";
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { makeZip } from './lib/theme-zip.mjs';
const require = createRequire(new URL("../tools/package.json", import.meta.url));
const { chromium, expect } = require("@playwright/test");

// 浏览器依赖由 tools 提供，插件页面随独立插件包分发。
const root = path.resolve(import.meta.dirname, "..");
const installAcceptance = process.env.OPSD_INSTALL_ACCEPTANCE || '';
const installFailures = installAcceptance === 'host' && process.arch === 'x64';
const sensitiveValues = new Set();
const run = path.join(root, ".data", "console-e2e-" + Date.now());
await mkdir(run, { recursive: true });
const exe = (n) =>
  path.join(process.env.OPSD_TEST_BIN_DIR || path.join(root, "target", "debug"), n + (process.platform === "win32" ? ".exe" : ""));
const hubDir = path.join(run, "hub");
const password = randomBytes(24).toString("hex");
const passwordFile = path.join(run, "password");
await writeFile(passwordFile, password, { mode: 0o600 });
sensitiveValues.add(password);

// 发布验收在独立 Linux runner 内执行真实安装，只清理本次预先确认不存在的 Agent 路径。
function rootShell(input) {
  return new Promise((resolve, reject) => {
    const child = spawn('sudo', ['-n', 'bash'], { cwd: root, stdio: ['pipe', 'pipe', 'pipe'] });
    let output = '';
    child.stdout.on('data', b => output += b);
    child.stderr.on('data', b => output += b);
    child.on('error', reject);
    child.on('exit', code => {
      for (const value of sensitiveValues) output = output.replaceAll(value, '[凭据已隐藏]');
      resolve({ code, output });
    });
    child.stdin.end('set -Eeuo pipefail\n' + input);
  });
}
async function rootSuccess(input) {
  const result = await rootShell(input);
  assert.ok(result.code === 0, `安装环境操作失败：${result.output.slice(-3000)}`);
  return result.output;
}
let ownsAgentPaths = false;
if (installAcceptance) {
  assert.ok(['host', 'docker'].includes(installAcceptance), '安装验收模式必须为 host 或 docker');
  await rootSuccess('set -eu\ntest ! -e /var/lib/opsd-agent\ntest ! -e /usr/local/bin/opsd-agent\ntest ! -e /etc/systemd/system/opsd-agent.service\nif docker container inspect opsd-agent >/dev/null 2>&1; then exit 1; fi\n');
  ownsAgentPaths = true;
  if (installFailures) await rootSuccess('apt-get remove -y jq\n');
}

function command(name, args) {
  return new Promise((resolve, reject) => {
    const child = spawn(exe(name), args, { cwd: root, windowsHide: true });
    let output = "";
    child.stdout.on("data", (b) => (output += b));
    child.stderr.on("data", (b) => (output += b));
    child.on("error", reject);
    child.on("exit", (c) => (c === 0 ? resolve(output) : reject(new Error(output))));
  });
}

const initOutput = await command("opsd-hub", [
  "--data-dir",
  hubDir,
  "init",
  "--password-file",
  passwordFile,
]);
const entrance = initOutput.match(/控制台安全入口：([A-Za-z0-9\-._~]{16})/)?.[1];
assert.ok(entrance, "应输出安全入口");
const ca = await readFile(path.join(hubDir, "pki", "ca.pem"));

const base = 19143;
let cookie = "";
let csrf = "";
function request(route, method = "GET", body) {
  return new Promise((resolve, reject) => {
    const data = body === undefined ? undefined : JSON.stringify(body);
    const req = https.request(
      {
        hostname: "localhost",
        port: base,
        path: `/${entrance}/api/v1${route}`,
        method,
        ca,
        headers: {
          Origin: `https://localhost:${base}`,
          Cookie: cookie,
          "X-CSRF-Token": csrf,
          ...(data
            ? { "Content-Type": "application/json", "Content-Length": Buffer.byteLength(data) }
            : {}),
        },
      },
      (res) => {
        let text = "";
        res.on("data", (b) => (text += b));
        res.on("end", () => {
          let value;
          try {
            value = JSON.parse(text);
          } catch {
            value = text;
          }
          resolve({ status: res.statusCode, data: value, cookie: res.headers["set-cookie"]?.[0] });
        });
      },
    );
    req.on("error", reject);
    req.setTimeout(5000, () => req.destroy(new Error("请求超时")));
    if (data) req.write(data);
    req.end();
  });
}

let hub;
let agent;
const logs = [];
function launch() {
  const c = spawn(
    exe("opsd-hub"),
    [
      "--data-dir",
      hubDir,
      "serve",
      "--listen",
      `127.0.0.1:${base}`,
      "--agent-listen",
      "127.0.0.1:8444",
      "--health-listen",
      "127.0.0.1:19543",
      "--origin",
      `https://localhost:${base}`,
      "--web",
      "web/dist",
    ],
    { cwd: root, windowsHide: true, env: {
      ...process.env,
      OPSD_AGENT_RELEASE: process.env.OPSD_AGENT_RELEASE || `agent-${'a'.repeat(40)}`,
      OPSD_AGENT_IMAGE: '',
      OPSD_BUNDLED_AGENT_IMAGE: process.env.OPSD_BUNDLED_AGENT_IMAGE || `ghcr.io/opsd-labs/opsd-agent@sha256:${'b'.repeat(64)}`,
    } },
  );
  c.stdout.on("data", (b) => logs.push(b.toString()));
  c.stderr.on("data", (b) => logs.push(b.toString()));
  return c;
}
async function stop(c) {
  if (!c || c.exitCode !== null) return;
  const done = new Promise((r) => c.on("exit", r));
  c.kill();
  await done;
}
async function waitForHub() {
  for (let i = 0; i < 60; i++) {
    try {
      const status = (await request('/auth/me')).status;
      if (status === 200 || status === 401) return;
    } catch (error) {
      if (!['ECONNREFUSED', 'ECONNRESET'].includes(error.code)) throw error;
    }
    await new Promise(resolve => setTimeout(resolve, 200));
  }
  throw new Error('主控启动超时');
}

try {
  hub = launch();
  await waitForHub();
  const login = await request("/auth/login", "POST", { password });
  assert.equal(login.status, 200);
  cookie = login.cookie.split(";")[0];
  csrf = login.data.csrf;
  // 会话 Cookie 必须限定在入口路径之下
  assert.ok(
    login.cookie.includes(`Path=/${entrance}/`),
    `会话 Cookie 应限定在入口路径，实际：${login.cookie}`,
  );

  const browser = await chromium.launch({
    executablePath: process.env.OPSD_BROWSER_EXECUTABLE,
  });
  const context = await browser.newContext({ ignoreHTTPSErrors: true });
  await context.grantPermissions(['clipboard-read', 'clipboard-write'], { origin: `https://localhost:${base}` });
  const consolePage = await context.newPage();
  await consolePage.goto(`https://localhost:${base}/${entrance}/`);
  await consolePage.getByLabel('密码', { exact: true }).fill(password);
  await consolePage.getByRole('button', { name: '登录', exact: true }).click();
  await expect(consolePage.locator('.shell')).toBeVisible();
  consolePage.on('dialog', dialog => dialog.accept());
  if (!installAcceptance) {
  await consolePage.getByRole('button', { name: '设置', exact: true }).click();
  await expect(consolePage.locator('.entrance-value')).toHaveText(entrance);
  await consolePage.getByRole('tab', { name: '审计日志' }).click();
  const audit = await request('/audit?limit=50');
  assert.equal(audit.status, 200);
  assert.ok(Array.isArray(audit.data.records));
  assert.equal(audit.data.records.length, 0);
  await expect(consolePage.locator('.audit-table tbody tr').first()).toContainText('暂无审计记录');

  await consolePage.getByRole('tab', { name: '输出 API', exact: true }).click();
  await consolePage.getByPlaceholder('密钥用途，例如 Prometheus').fill('浏览器输出验证');
  await consolePage.getByRole('button', { name: '创建只读密钥', exact: true }).click();
  const exportItem = consolePage.locator('.export-item').filter({ hasText: '浏览器输出验证' });
  await expect(exportItem).toContainText('有效');
  await expect(consolePage.getByText('密钥仅显示一次，请保存：')).toBeVisible();
  await exportItem.getByRole('button', { name: '撤销' }).click();
  await expect(exportItem).toContainText('已撤销');

  await consolePage.getByRole('tab', { name: '主题', exact: true }).click();
  const zip = makeZip({
    'theme.json': JSON.stringify({ short: 'browser-console', name: '浏览器主题', version: '1.0.0', surfaces: ['console'], console_frontend: { api_version: 2 } }),
    'index.html': '<!doctype html><html><head><title>浏览器主题</title></head><body>外部控制台验证</body></html>',
  });
  await consolePage.locator('input[type=file]').setInputFiles({ name: 'console.zip', mimeType: 'application/zip', buffer: zip });
  const themeItem = consolePage.locator('.theme-item').filter({ hasText: 'browser-console' });
  await expect(themeItem).toContainText('浏览器主题');
  const conflictResponse = consolePage.waitForResponse(r => r.url().endsWith('/themes/console/package') && r.request().method() === 'POST');
  await consolePage.locator('input[type=file]').setInputFiles({ name: 'console.zip', mimeType: 'application/zip', buffer: zip });
  assert.equal((await conflictResponse).status(), 409);
  await expect(consolePage.locator('.settings-error')).toContainText('已安装');
  await consolePage.getByPlaceholder('https://github.com/opsd-labs/opsd-theme-web').fill('https://localhost/repo');
  await consolePage.getByRole('button', { name: '解析', exact: true }).click();
  await expect(consolePage.locator('.settings-error').filter({ hasText: 'GitHub' })).toBeVisible();
  await themeItem.getByRole('button', { name: '切换', exact: true }).click();
  await expect(consolePage.getByText('外部控制台验证')).toBeVisible();
  await consolePage.goto(`https://localhost:${base}/${entrance}/frontend/default/?page=settings`);
  await consolePage.getByRole('tab', { name: '主题', exact: true }).click();
  await expect(consolePage.locator('.theme-item').filter({ hasText: 'browser-console' })).toContainText('当前激活');
  await consolePage.locator('.theme-item').filter({ hasText: 'browser-console' }).getByRole('button', { name: '卸载' }).click();
  await expect(consolePage.locator('.shell')).toBeVisible();
  const recovered = await request('/themes/active');
  assert.equal(recovered.status, 200);
  assert.equal(recovered.data.console_frontend.short, 'default');
  }

  await consolePage.getByRole('button', { name: '节点', exact: true }).click();
  await expect(consolePage.getByRole('button', { name: '添加节点', exact: true })).toBeVisible();
  await expect(consolePage.getByText('还没有节点，点击“添加节点”开始接入')).toBeVisible();
  await consolePage.getByRole('button', { name: '添加节点', exact: true }).click();
  const enrollmentDrawer = consolePage.getByRole('dialog', { name: '添加节点', exact: true });
  if (!installAcceptance) {
  await enrollmentDrawer.getByLabel('节点名称', { exact: true }).fill('浏览器 Docker 节点');
  await expect(enrollmentDrawer.getByLabel('公网 IP（选填）', { exact: true })).toHaveCount(0);
  await expect(enrollmentDrawer.getByLabel('覆盖网 IP（选填）', { exact: true })).toHaveCount(0);
  await enrollmentDrawer.getByLabel('安装方式', { exact: true }).selectOption('docker');
  const dockerRegistrationResponse = consolePage.waitForResponse(r => r.url().endsWith('/enrollment-tokens') && r.request().method() === 'POST');
  await enrollmentDrawer.getByRole('button', { name: '生成注册信息', exact: true }).click();
  const dockerResponse = await dockerRegistrationResponse;
  assert.equal(dockerResponse.status(), 200);
  const dockerRegistration = await dockerResponse.json();
  assert.equal(dockerRegistration.install_mode, 'docker');
  await expect(enrollmentDrawer.getByText('注册信息已生成，等待 Agent 接入')).toBeVisible();
  sensitiveValues.add(dockerRegistration.token);
  assert.equal(dockerResponse.headers()['cache-control'], 'no-store');
  assert.ok(dockerRegistration.installation.script.includes(dockerRegistration.agent_image));
  assert.ok(dockerRegistration.installation.script.includes('OPSD_AGENT_MODE=container'));
  const dockerDownloadResponse = consolePage.waitForEvent('download');
  await enrollmentDrawer.getByRole('button', { name: '下载安装脚本', exact: true }).click();
  const dockerScriptFile = path.join(run, 'docker-install.sh');
  await (await dockerDownloadResponse).saveAs(dockerScriptFile);
  assert.ok(await readFile(dockerScriptFile, 'utf8') === dockerRegistration.installation.script, '下载的 Docker 脚本应与创建响应一致');
  await enrollmentDrawer.locator('.drawer__footer').getByRole('button', { name: '关闭', exact: true }).click();
  const dockerPending = consolePage.locator('.enrollment-row').filter({ hasText: '浏览器 Docker 节点' });
  await expect(dockerPending).toContainText('待注册');
  // 这里只给界面提供过期摘要，取消与重建仍调用真实 Hub；安装器的实际到期拒绝在发布验收覆盖。
  const summaryPattern = '**/api/v1/enrollment-tokens';
  await consolePage.route(summaryPattern, async route => {
    if (route.request().method() !== 'GET') return route.continue();
    const response = await route.fetch();
    const entries = await response.json();
    for (const entry of entries) if (entry.node_id === dockerRegistration.node_id) { entry.status = 'expired'; entry.expires_at = Math.floor(Date.now() / 1000) - 1; }
    await route.fulfill({ response, json: entries });
  });
  await consolePage.reload();
  await expect(dockerPending).toContainText('已过期');
  const oldDeletePattern = `**/api/v1/enrollment-tokens/${dockerRegistration.node_id}`;
  await consolePage.route(oldDeletePattern, route => route.fulfill({ status: 400, contentType: 'application/json', body: JSON.stringify({ error: '旧注册状态已变化，请先核实' }) }));
  let accidentalCreations = 0;
  const observeCreation = req => { if (req.url().endsWith('/enrollment-tokens') && req.method() === 'POST') accidentalCreations++; };
  consolePage.on('request', observeCreation);
  await dockerPending.getByRole('button', { name: '重新生成', exact: true }).click();
  await expect(consolePage.locator('.nodes-page > .enrollment-error')).toContainText('旧注册状态已变化');
  assert.equal(accidentalCreations, 0, '取消旧注册失败时不得继续创建新节点');
  consolePage.off('request', observeCreation);
  await consolePage.unroute(oldDeletePattern);
  const regeneratedResponse = consolePage.waitForResponse(r => r.url().endsWith('/enrollment-tokens') && r.request().method() === 'POST');
  await dockerPending.getByRole('button', { name: '重新生成', exact: true }).click();
  const regenerated = await regeneratedResponse;
  assert.equal(regenerated.status(), 200);
  const newDockerRegistration = await regenerated.json();
  sensitiveValues.add(newDockerRegistration.token);
  assert.equal(newDockerRegistration.install_mode, 'docker');
  assert.ok(newDockerRegistration.node_id !== dockerRegistration.node_id, '旧注册应取消后重建');
  await enrollmentDrawer.locator('.drawer__footer').getByRole('button', { name: '关闭', exact: true }).click();
  await consolePage.unroute(summaryPattern);
  await dockerPending.getByRole('button', { name: '取消注册', exact: true }).click();
  await expect(dockerPending).toHaveCount(0);

  await consolePage.getByRole('button', { name: '添加节点', exact: true }).click();
  }
  await enrollmentDrawer.getByLabel('节点名称', { exact: true }).fill('浏览器节点');
  if (installAcceptance) await enrollmentDrawer.getByLabel('安装方式', { exact: true }).selectOption(installAcceptance);
  const registrationResponse = consolePage.waitForResponse(r => r.url().endsWith('/enrollment-tokens') && r.request().method() === 'POST');
  await enrollmentDrawer.getByRole('button', { name: '生成注册信息', exact: true }).click();
  const response = await registrationResponse;
  assert.equal(response.status(), 200);
  const registration = await response.json();
  sensitiveValues.add(registration.token);
  assert.equal(registration.install_mode, installAcceptance || 'host');
  assert.ok(registration.installation?.script, '配置发行元数据后应提供专属脚本');
  const generatedCommand = await enrollmentDrawer.getByLabel('一键安装命令', { exact: true }).inputValue();
  const encodedScript = generatedCommand.match(/^printf '%s' '([A-Za-z0-9+/=]+)' \| base64 --decode \| bash$/)?.[1];
  assert.ok(encodedScript, '应生成单行自包含命令');
  sensitiveValues.add(encodedScript);
  assert.ok(Buffer.from(encodedScript, 'base64').toString('utf8') === registration.installation.script, '复制命令与下载脚本必须使用同一内容');
  await enrollmentDrawer.getByRole('button', { name: '复制一键安装命令', exact: true }).click();
  await expect(enrollmentDrawer.getByRole('status')).toContainText('已复制');
  assert.ok(await consolePage.evaluate(() => navigator.clipboard.readText()) === generatedCommand, '剪贴板应获得完整命令');
  const scriptFile = path.join(run, 'opsd-install.sh');
  const scriptDownloadResponse = consolePage.waitForEvent('download');
  await enrollmentDrawer.getByRole('button', { name: '下载安装脚本', exact: true }).click();
  await (await scriptDownloadResponse).saveAs(scriptFile);
  assert.ok(await readFile(scriptFile, 'utf8') === registration.installation.script, '下载脚本应完整保留原内容');
  if (!installAcceptance) {
    await consolePage.evaluate(() => { navigator.clipboard.writeText = async () => { throw new Error('测试复制失败分支'); }; });
    await enrollmentDrawer.getByRole('button', { name: '复制一键安装命令', exact: true }).click();
    await expect(enrollmentDrawer.getByRole('alert')).toContainText('自动复制失败');
    assert.ok(await enrollmentDrawer.getByLabel('一键安装命令', { exact: true }).evaluate(el => el.selectionEnd - el.selectionStart === el.value.length), '复制失败后应选中完整命令');
    await enrollmentDrawer.getByText('高级：下载原始令牌与手动接入', { exact: true }).click();
  }
  const tokenFile = path.join(run, 'agent-token');
  if (!installAcceptance) {
  const downloadResponse = consolePage.waitForEvent('download');
  await enrollmentDrawer.getByRole('button', { name: '下载令牌文件', exact: true }).click();
  await (await downloadResponse).saveAs(tokenFile);
  assert.ok((await readFile(tokenFile, 'utf8')).trim() === registration.token, '下载文件应包含本次令牌');
  }
  await enrollmentDrawer.locator('.drawer__footer').getByRole('button', { name: '关闭', exact: true }).click();
  await consolePage.reload();
  const hostPending = consolePage.locator('.enrollment-row').filter({ hasText: '浏览器节点' });
  await expect(hostPending).toContainText('待注册');
  const summaries = await request('/enrollment-tokens');
  assert.equal(summaries.data.length, 1);
  assert.ok(!('token' in summaries.data[0]), '重新加载的注册摘要不得返回令牌');
  await expect(consolePage.locator('.enrollment-command')).toHaveCount(0);
  const agentDir = path.join(run, 'agent');
  if (!installAcceptance) {
  await command('opsd-agent', ['--data-dir', agentDir, 'enroll', '--hub', `https://localhost:${base}`, '--agent-url', registration.agent_url, '--ca', path.join(hubDir, 'pki', 'ca.pem'), '--fingerprint', registration.ca_fingerprint, '--token-file', tokenFile]);
  agent = spawn(exe('opsd-agent'), ['--data-dir', agentDir, 'run'], { cwd: root, windowsHide: true });
  agent.stdout.on('data', b => logs.push(b.toString()));
  agent.stderr.on('data', b => logs.push(b.toString()));
  } else {
    // 正式制品验收执行页面原样命令；另一架构直接执行下载脚本，重入再覆盖另一入口。
    if (installFailures) {
      const source = registration.installation.script;
      const missingRelease = source.replace(`release='${registration.installation.release}'`, `release='agent-${'0'.repeat(40)}'`);
      const missingDownload = await rootShell(missingRelease);
      assert.ok(missingDownload.code !== 0 && missingDownload.output.includes('404') && !missingDownload.output.includes('安装完成：'), '必须确实因发行包不存在而失败，不能把其他环境错误算作下载拒绝');
      const expired = await rootShell(source.replace(/^expires_at=\d+$/m, 'expires_at=0'));
      assert.ok(expired.code !== 0 && expired.output.includes('令牌已过期'), '过期安装信息必须停止');
      const badCa = await rootShell(source.replace(`fingerprint='${registration.ca_fingerprint}'`, `fingerprint='${'0'.repeat(64)}'`));
      assert.ok(badCa.code !== 0 && badCa.output.includes('主控 CA 指纹不匹配') && !badCa.output.includes('安装完成：'), '必须确实因 CA 指纹不匹配停止注册');
      assert.equal((await request('/nodes')).data.length, 0);
      await rootSuccess("printf '%s' '{\"node_id\":\"other\",\"hub\":\"https://other.invalid\",\"agent_url\":\"wss://other.invalid:8444/agent\"}' > /var/lib/opsd-agent/config.json\n");
      const conflict = await rootShell(source);
      assert.ok(conflict.code !== 0 && conflict.output.includes('身份与本次节点或 Hub 不一致'), '不能覆盖其他身份');
      const saved = await rootSuccess('cat /var/lib/opsd-agent/config.json\n');
      assert.equal(JSON.parse(saved).node_id, 'other');
      await rootSuccess('rm -f /var/lib/opsd-agent/config.json\n');
      const startFailure = await rootShell(source.replace('ExecStart=/usr/local/bin/opsd-agent --data-dir /var/lib/opsd-agent run', 'ExecStart=/bin/false'));
      assert.ok(startFailure.code !== 0 && (startFailure.output.includes('启动常驻服务') || startFailure.output.includes('尚未确认节点在线')) && !startFailure.output.includes('安装完成：'), '必须确实在服务启动或等待在线阶段失败');
      assert.equal((await request('/nodes')).data.length, 1, '启动失败应保留已注册身份');
    }
    const installResult = process.arch === 'arm64'
      ? await rootShell(`bash '${scriptFile.replaceAll("'", "'\\''")}'\n`)
      : await rootShell(generatedCommand + '\n');
    assert.ok(installResult.code === 0 && installResult.output.includes('Hub 已确认节点在线'), `一键安装未完成：${installResult.output.slice(-3000)}`);
    const repeated = process.arch === 'arm64' ? await rootShell(generatedCommand + '\n') : await rootShell(`bash '${scriptFile.replaceAll("'", "'\\''")}'\n`);
    assert.ok(repeated.code === 0, `重复执行应复用身份：${repeated.output.slice(-3000)}`);
    assert.equal((await request('/nodes')).data.length, 1, '重复执行不得生成第二个节点');
    const permissionOutput = await rootSuccess('stat -c "%a" /var/lib/opsd-agent /var/lib/opsd-agent/client-key.pem\ntest ! -e /var/lib/opsd-agent/enrollment.token\n');
    assert.deepEqual(permissionOutput.trim().split(/\s+/), ['700', '600']);
    if (installAcceptance === 'docker') {
      const mounts = JSON.parse(await rootSuccess("docker inspect opsd-agent --format '{{json .Mounts}}'\n"));
      assert.ok(!mounts.some(m => /token/.test(m.Source) || /token/.test(m.Destination)), '常驻容器不应挂载注册令牌');
    }
    if (installFailures) {
      await stop(hub); hub = undefined;
      const unavailable = await rootShell(generatedCommand + '\n');
      assert.ok(unavailable.code !== 0 && unavailable.output.includes('尚未确认节点在线'), 'mTLS 不可达应报告失败并保留身份');
      hub = launch();
      await waitForHub();
    }
    await expect.poll(async () => (await request('/nodes')).data[0]?.connected, { timeout: 60000 }).toBe(true);
    await rootSuccess(installAcceptance === 'host' ? 'systemctl stop opsd-agent.service\n' : 'docker stop opsd-agent >/dev/null\n');
    await expect.poll(async () => (await request('/nodes')).data[0]?.connected, { timeout: 20000 }).toBe(false);
    await rootSuccess(installAcceptance === 'host' ? 'systemctl start opsd-agent.service\n' : 'docker start opsd-agent >/dev/null\n');
  }
  await expect.poll(async () => (await request('/nodes')).data[0]?.connected, { timeout: 20000 }).toBe(true);
  await expect(hostPending).toHaveCount(0);
  await expect(consolePage.locator('.node-header')).toContainText('浏览器节点');
  await expect(consolePage.locator('.node-header')).toContainText('在线');
  await expect(consolePage.getByRole('heading', { name: '控制线路与自动互信' })).toBeVisible();
  const inspected = await request(`/nodes/${registration.node_id}/actions`, 'POST', { idempotency_key: 'browser-inspect', action: { type: 'inspect' } });
  assert.equal(inspected.status, 202);
  await expect.poll(async () => (await request('/tasks/' + inspected.data.task_id)).data.status, { timeout: 30000 }).toBe('succeeded');

  if (installAcceptance) {
    console.log(JSON.stringify({ 一键安装: '页面原样命令、下载脚本、重复执行、权限、服务重启恢复与真实上线通过', 模式: installAcceptance, 架构: process.arch, 定点失败: installFailures ? '下载、过期、CA、已有身份、启动失败与 mTLS 不可达通过' : '复用 x64 宿主机的失败分支证据' }));
  } else {
  await consolePage.getByRole('button', { name: '插件', exact: true }).click();
  await expect(consolePage.getByText('暂无插件。安装独立发行的插件 ZIP 后，按依赖顺序启用。')).toBeVisible();
  for (const id of ['et', 'docker', 'silo']) {
    const zipPath = path.join(root, '.release/plugins', id + '-0.1.0.zip');
    const installed = consolePage.waitForResponse(response => response.url().endsWith('/plugins/package') && response.request().method() === 'POST');
    await consolePage.locator('.upload input[type=file]').setInputFiles(zipPath);
    assert.equal((await installed).status(), 200);
    const card = consolePage.locator('.plugin-card').filter({ hasText: id === 'et' ? 'EasyTier 网络' : id === 'docker' ? 'Docker / Portainer' : 'Silo 分布式存储' });
    await card.getByRole('button', { name: '启用此版本', exact: true }).click();
    await expect(card).toContainText('已启用');
  }
  // 页面只读数据使用接口夹具；上游 ET 写入与双控制连接已由 HTTP 集成验收。
  await consolePage.route('**/api/v1/plugins/et/api/**', route => {
    const path = new URL(route.request().url()).pathname.split('/api/').at(-1);
    const value = path === 'machines' ? { machines: [{ machine_id: '11111111-1111-4111-8111-111111111111', info: { hostname: '浏览器 ET 机器', easytier_version: '2.6.4', report_time: '当前夹具时间' } }] }
      : path === 'console' ? { url: 'http://100.100.201.101:61018/' }
      : path === 'instances' ? { running_inst_ids: ['22222222-2222-4222-8222-222222222222'], disabled_inst_ids: [] }
      : path === 'config' ? { hostname: '浏览器 ET 机器', virtual_ipv4: '100.100.201.91', network_length: 10, dev_name: 'jkfujr-easytier' }
      : { info: { map: {} } };
    return route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(value) });
  });
  await consolePage.getByRole('button', { name: 'EasyTier 网络', exact: true }).click();
  const etFrame = consolePage.frameLocator('iframe');
  await expect(etFrame.getByRole('heading', { name: 'EasyTier 网络', exact: true })).toBeVisible();
  await etFrame.getByRole('button', { name: '查看实例', exact: true }).click();
  await etFrame.getByRole('button', { name: '配置与链路', exact: true }).click();
  await expect(etFrame.getByText('实例配置与关联', { exact: true })).toBeVisible();
  assert.equal(await etFrame.getByRole('link', { name: '打开 ET 管理台 ↗' }).getAttribute('target'), '_blank');
  await consolePage.getByRole('button', { name: '插件管理', exact: true }).click();
  await consolePage.route('**/api/v1/plugins/docker/api/**', route => {
    const path = new URL(route.request().url()).pathname.split('/api/').at(-1);
    const value = path === 'environments' ? { bindings: [{ node_id: registration.node_id, endpoint_id: 1 }], environments: [] }
      : path === 'console' ? { url: 'https://100.100.201.91:9443/' }
      : path === 'logs' ? { text: '浏览器日志夹具' }
      : { containers: [{ Id: 'abc123', Names: ['/existing-app'], Image: 'nginx:stable', Status: '运行中', Labels: {} }], stacks: [], images: [], networks: [], volumes: { Volumes: [] } };
    return route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(value) });
  });
  await consolePage.getByRole('button', { name: 'Docker / Portainer', exact: true }).click();
  const dockerFrame = consolePage.frameLocator('iframe');
  await expect(dockerFrame.getByText('/existing-app', { exact: true })).toBeVisible();
  await dockerFrame.getByRole('button', { name: '日志', exact: true }).click();
  await expect(dockerFrame.getByText('浏览器日志夹具', { exact: true })).toBeVisible();
  await dockerFrame.getByRole('button', { name: 'Stack', exact: true }).click();
  await expect(dockerFrame.getByRole('button', { name: '新建 Stack', exact: true })).toBeVisible();
  await consolePage.getByRole('button', { name: '插件管理', exact: true }).click();
  await consolePage.getByRole('button', { name: 'Silo 分布式存储', exact: true }).click();
  const siloFrame = consolePage.frameLocator('iframe');
  await expect(siloFrame.getByRole('heading', { name: 'Silo 分布式存储', exact: true })).toBeVisible();
  await siloFrame.getByText('部署新集群', { exact: true }).click();
  await expect(siloFrame.getByText('我已核对下列新盘设备与容量，确认允许清空所选新盘。已有路径不格式化。', { exact: true })).toBeVisible();
  await consolePage.setViewportSize({ width: 390, height: 844 });
  for (const frame of [siloFrame]) {
    const dimensions = await frame.locator('main').evaluate(element => ({ width: element.scrollWidth, viewport: innerWidth }));
    assert.ok(dimensions.width <= dimensions.viewport + 1, '插件页面不应出现横向溢出');
  }
  console.log('通过：真实登录、输出密钥、主题 v2、无 IP 安装、真实 Agent 上线、插件包管理和三个独立页面；业务页只读上游使用夹具，未声明真实 Portainer 或 Silo 部署通过。');
  }
  await browser.close();
} finally {
  await stop(agent);
  await stop(hub);
  if (ownsAgentPaths) {
    await rootSuccess('if systemctl cat opsd-agent.service >/dev/null 2>&1; then systemctl disable --now opsd-agent.service; fi\nif docker container inspect opsd-agent >/dev/null 2>&1; then docker rm -f opsd-agent; fi\nrm -f /etc/systemd/system/opsd-agent.service /usr/local/bin/opsd-agent\nrm -rf -- /var/lib/opsd-agent\nsystemctl daemon-reload\n');
  }
  await writeFile(path.join(run, "hub.log"), logs.join(""));
  await rm(run, { recursive: true, force: true }).catch(() => {});
}
