/**
 * 分享面的端到端验证：真实主控 + 真实浏览器。
 *
 * 重点是那条隔离承诺——**浏览器不会把控制台会话 Cookie 发给分享路径**。
 * 这一点只能在实际浏览器里看请求头才能确认，单元测试与接口测试都覆盖不到。
 */
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

// 分享页由主仓库维护，浏览器依赖由 tools 提供。
const root = path.resolve(import.meta.dirname, "..");
const run = path.join(root, ".data", "share-e2e-" + Date.now());
await mkdir(run, { recursive: true });
const exe = (n) =>
  path.join(root, "target", "debug", n + (process.platform === "win32" ? ".exe" : ""));
const hubDir = path.join(run, "hub");
const password = randomBytes(24).toString("hex");
const passwordFile = path.join(run, "password");
await writeFile(passwordFile, password, { mode: 0o600 });

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
      "127.0.0.1:19443",
      "--health-listen",
      "127.0.0.1:19543",
      "--origin",
      `https://localhost:${base}`,
      "--web",
      "web/dist",
    ],
    { cwd: root, windowsHide: true },
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

try {
  hub = launch();
  for (let i = 0; i < 60; i++) {
    try {
      if ((await request("/auth/me")).status === 401) break;
    } catch {}
    await new Promise((r) => setTimeout(r, 200));
  }
  const login = await request("/auth/login", "POST", { password });
  assert.equal(login.status, 200);
  cookie = login.cookie.split(";")[0];
  csrf = login.data.csrf;
  // 会话 Cookie 必须限定在入口路径之下
  assert.ok(
    login.cookie.includes(`Path=/${entrance}/`),
    `会话 Cookie 应限定在入口路径，实际：${login.cookie}`,
  );

  // 分享页与内置控制台共用真实 Hub，令牌仅留在本次进程内。
  await request("/share/settings", "PUT", {
    enabled: true,
    site: { name: "十二节点状态", description: "只读展示", footer: "" },
  });
  const token = await request("/share/tokens", "POST", { label: "浏览器验证", expires_hours: 1 });
  assert.equal(token.status, 200);
  const shareToken = token.data.token;

  const browser = await chromium.launch({
    executablePath: process.env.OPSD_BROWSER_EXECUTABLE,
  });
  const context = await browser.newContext({ ignoreHTTPSErrors: true });
  const consolePage = await context.newPage();
  await consolePage.goto(`https://localhost:${base}/${entrance}/`);
  await consolePage.getByLabel('密码', { exact: true }).fill(password);
  await consolePage.getByRole('button', { name: '登录', exact: true }).click();
  await expect(consolePage.locator('.shell')).toBeVisible();
  await consolePage.getByRole('button', { name: '设置', exact: true }).click();
  await expect(consolePage.locator('.entrance-value')).toHaveText(entrance);
  await consolePage.getByRole('tab', { name: '审计日志' }).click();
  await expect(consolePage.locator('.audit-table tbody tr').first()).not.toContainText('暂无审计记录');

  await consolePage.getByRole('tab', { name: '分享', exact: true }).click();
  await expect(consolePage.getByText('公开分享：已启用')).toBeVisible();
  await consolePage.getByPlaceholder('标签（如：团队演示）').fill('界面分享验证');
  await consolePage.getByRole('button', { name: '创建', exact: true }).click();
  const createdLink = consolePage.locator('a[href*="/share/"]');
  await expect(createdLink).toBeVisible();
  assert.ok((await createdLink.getAttribute('href')).startsWith(`https://localhost:${base}/share/`));
  consolePage.on('dialog', dialog => dialog.accept());
  const shareItem = consolePage.locator('.share-item').filter({ hasText: '界面分享验证' });
  await shareItem.getByRole('button', { name: '撤销' }).click();
  await expect(shareItem).toContainText('已撤销');
  await consolePage.getByRole('button', { name: '关闭分享' }).click();
  await expect(consolePage.getByText('公开分享：已关闭')).toBeVisible();
  await consolePage.getByRole('button', { name: '启用分享' }).click();
  await expect(consolePage.getByText('公开分享：已启用')).toBeVisible();

  await consolePage.getByRole('tab', { name: '主题', exact: true }).click();
  const zip = makeZip({
    'theme.json': JSON.stringify({ short: 'browser-console', name: '浏览器主题', version: '1.0.0', surfaces: ['console'], console_frontend: { api_version: 1 } }),
    'index.html': '<!doctype html><html><head><title>浏览器主题</title></head><body>外部控制台验证</body></html>',
  });
  await consolePage.locator('input[type=file]').setInputFiles({ name: 'console.zip', mimeType: 'application/zip', buffer: zip });
  const themeItem = consolePage.locator('.theme-item').filter({ hasText: 'browser-console' });
  await expect(themeItem).toContainText('浏览器主题');
  await consolePage.locator('input[type=file]').setInputFiles({ name: 'console.zip', mimeType: 'application/zip', buffer: zip });
  await expect(consolePage.locator('.settings-error')).toContainText('已存在');
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
  assert.equal((await request('/themes/active')).data.console_frontend, null);

  const registration = await request('/enrollment-tokens', 'POST', { name: '浏览器节点', public_addresses: [], ssh_port: 22 });
  assert.equal(registration.status, 200);
  const tokenFile = path.join(run, 'agent-token');
  const agentDir = path.join(run, 'agent');
  await writeFile(tokenFile, registration.data.token, { mode: 0o600 });
  await command('opsd-agent', ['--data-dir', agentDir, 'enroll', '--hub', `https://localhost:${base}`, '--agent-url', 'wss://localhost:19443/agent', '--ca', path.join(hubDir, 'pki', 'ca.pem'), '--fingerprint', registration.data.ca_fingerprint, '--token-file', tokenFile]);
  agent = spawn(exe('opsd-agent'), ['--data-dir', agentDir, 'run'], { cwd: root, windowsHide: true });
  agent.stdout.on('data', b => logs.push(b.toString()));
  agent.stderr.on('data', b => logs.push(b.toString()));
  await expect.poll(async () => (await request('/nodes')).data[0]?.connected, { timeout: 20000 }).toBe(true);
  await consolePage.getByRole('button', { name: 'Docker', exact: true }).click();
  await expect(consolePage.getByRole('button', { name: '刷新', exact: true })).toBeVisible();
  const refreshResponse = consolePage.waitForResponse(r => r.url().endsWith(`/nodes/${registration.data.node_id}/actions`) && r.request().method() === 'POST');
  await consolePage.getByRole('button', { name: '刷新', exact: true }).click();
  const submitted = await refreshResponse;
  assert.equal(submitted.status(), 202);
  assert.equal(submitted.request().postDataJSON().action.type, 'inspect');
  const submittedTask = await submitted.json();
  await expect.poll(async () => (await request(`/tasks/${submittedTask.task_id}`)).data.status, { timeout: 30000 }).toBe('succeeded');
  await expect(consolePage.getByText('盘点任务已提交，完成后列表将自动更新')).toBeVisible();

  const page = await context.newPage();
  const seen = [];
  page.on("request", (req) => {
    const url = new URL(req.url());
    if (url.pathname.startsWith("/share/")) {
      seen.push({ path: url.pathname, cookie: req.headers()["cookie"] || "" });
    }
  });
  await page.goto(`https://localhost:${base}/share/${shareToken}/`, {
    waitUntil: "networkidle",
  });
  await page.waitForTimeout(500);

  const info = await page.evaluate(() => ({
    title: document.title,
    heading: document.querySelector("h1")?.textContent?.trim() || "",
    hasConsole: !!document.querySelector(".app-shell"),
    bodyText: document.body.innerText.replace(/\s+/g, " ").slice(0, 120),
    assetCount: performance.getEntriesByType("resource").length,
  }));
  // 分享页不得加载控制台外壳
  assert.equal(info.hasConsole, false, "分享页不应包含控制台外壳");
  assert.ok(info.assetCount > 0, "分享页资源应加载成功");
  // 关键：没有任何一个分享路径请求带上控制台会话 Cookie
  assert.ok(seen.length > 0, "应观察到分享路径请求");
  const leaked = seen.filter((r) => r.cookie.includes("opsd_session"));
  assert.equal(
    leaked.length,
    0,
    `分享路径不应收到控制台会话 Cookie，实际：${JSON.stringify(leaked)}`,
  );

  // 令牌无效时门禁直接丢弃：浏览器会看到连接失败而不是错误页
  let dropped = false;
  try {
    const bad = await context.newPage();
    await bad.goto(`https://localhost:${base}/share/wrongtoken00000/data/nodes`, {
      timeout: 8000,
    });
  } catch {
    dropped = true;
  }
  assert.ok(dropped, "无效令牌应导致连接被丢弃");

  console.log(
    JSON.stringify(
      {
        内置控制台: '真实登录、设置、分享、ZIP、冲突、错误、全局切换、恢复卸载及 Agent 盘点任务通过',
        分享页: info,
        分享路径请求数: seen.length,
        带控制台Cookie的请求: leaked.length,
        无效令牌被丢弃: dropped,
      },
      null,
      1,
    ),
  );
  await browser.close();
} finally {
  await stop(agent);
  await stop(hub);
  await writeFile(path.join(run, "hub.log"), logs.join(""));
  await rm(run, { recursive: true, force: true }).catch(() => {});
}
