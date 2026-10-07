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
const require = createRequire(new URL("../tools/package.json", import.meta.url));
const { chromium } = require("playwright");

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

  // 造一个演示节点：直接用 enroll 太绕，这里用控制库写入一个节点记录
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
  // 把控制台会话 Cookie 装进浏览器，模拟"管理员已登录"
  await context.addCookies([
    {
      name: "opsd_session",
      value: cookie.replace("opsd_session=", ""),
      domain: "localhost",
      path: `/${entrance}/`,
      httpOnly: true,
      secure: true,
    },
  ]);
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
  await stop(hub);
  await writeFile(path.join(run, "hub.log"), logs.join(""));
  await rm(run, { recursive: true, force: true }).catch(() => {});
}
