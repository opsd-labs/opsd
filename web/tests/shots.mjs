/**
 * 截取页面截图，用于人工核对视觉密度与暗色模式。
 *
 * 例：node tests/shots.mjs database "../docs/ui/database"
 */
import { chromium } from "playwright";
import { mkdir } from "node:fs/promises";
import path from "node:path";

const page = process.argv[2];
// 默认落到仓库的 docs/ui（本脚本在 web/tests 下）
const prefix = process.argv[3] || `../../docs/ui/${page}`;
const base = "http://127.0.0.1:5173";
const executablePath = process.env.OPSD_BROWSER_EXECUTABLE;

const browser = await chromium.launch(executablePath ? { executablePath } : {});
await mkdir(path.dirname(path.resolve(import.meta.dirname, prefix)), {
  recursive: true,
});
for (const theme of ["light", "dark"]) {
  const context = await browser.newContext({
    viewport: { width: 1440, height: 1100 },
    colorScheme: theme,
  });
  const target = await context.newPage();
  await target.goto(`${base}/?demo=1&page=${page}`);
  await target.waitForTimeout(1200);
  // 展开第一台节点，让明细也进入截图
  const header = target.locator(".storage-node > header").first();
  if (await header.count()) await header.click().catch(() => {});
  await target.waitForTimeout(300);
  const file = path.resolve(import.meta.dirname, `${prefix}-${theme}.png`);
  await target.screenshot({ path: file, fullPage: true });
  console.log(file);
  await context.close();
}
await browser.close();
