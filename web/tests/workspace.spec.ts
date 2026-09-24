import { test, expect } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";
import { demoNodes, demoPeers } from "../src/demo";
const base = "http://127.0.0.1:5173";
const firewall = base + "/?demo=1&page=firewall&node=C052";
test("页面与抽屉的自动无障碍检查", async ({ page }) => {
  await page.goto(firewall);
  await expect(page.getByRole("region", { name: "防火墙规则", exact: true })).toBeVisible();
  for (const drawer of [false, true]) {
    if (drawer) await page.getByRole("button", { name: "新建规则", exact: true }).click();
    const result = await new AxeBuilder({ page }).withTags(["wcag2a", "wcag2aa", "wcag21aa"]).analyze();
    expect(result.violations.map(v=>({id:v.id,help:v.help,nodes:v.nodes.map(n=>n.target)}))).toEqual([]);
  }
});
test("真实接口形状的失败保留、结构化计划与跨节点定位", async ({ page }) => {
  const submitted: any[] = [];
  let reject = true;
  await page.route("**/api/v1/**", async (route) => {
    const url = new URL(route.request().url());
    const pathname = url.pathname;
    let body: any = {};
    let status = 200;
    if (pathname.endsWith("/auth/me")) body = { csrf: "test-csrf" };
    else if (pathname.endsWith("/nodes")) body = demoNodes;
    else if (pathname.endsWith("/tasks")) body = [];
    else if (pathname.endsWith("/peer-addresses")) body = demoPeers;
    else if (pathname.endsWith("/events")) {
      await route.fulfill({
        contentType: "text/event-stream",
        body: ": 就绪\n\n",
      });
      return;
    } else if (pathname.endsWith("/actions")) {
      const data = route.request().postDataJSON();
      submitted.push({
        path: pathname,
        data,
        csrf: route.request().headers()["x-csrf-token"],
      });
      if (reject) {
        status = 400;
        body = { error: "规则指纹已变化，请重新生成计划" };
      } else {
        status = 202;
        body = { task_id: "test-operation-1" };
      }
    }
    await route.fulfill({
      status,
      contentType: "application/json",
      body: JSON.stringify(body),
    });
  });
  await page.goto(base + "/?page=firewall&node=C052");
  await page.getByRole("button", { name: "新建规则", exact: true }).click();
  const dialog = page.getByRole("dialog");
  await dialog.getByLabel("来源 CIDR").fill("192.0.2.0/24");
  await dialog.getByLabel("目标端口").fill("80,443");
  await dialog.getByRole("button", { name: "生成计划", exact: true }).click();
  await expect(dialog.getByRole("alert")).toHaveText(
    "规则指纹已变化，请重新生成计划",
  );
  await expect(dialog.getByLabel("目标端口")).toHaveValue("80,443");
  expect(submitted[0].path).toBe("/api/v1/nodes/C052/actions");
  expect(submitted[0].csrf).toBe("test-csrf");
  expect(submitted[0].data.action).toMatchObject({
    type: "firewall_plan",
    operation: {
      type: "rule_put",
      rule: { family: 4, ports: [80, 443], source: "192.0.2.0/24" },
    },
  });
  reject = false;
  await dialog.getByRole("button", { name: "生成计划", exact: true }).click();
  await expect(dialog).toHaveCount(0);
  await expect(
    page.getByText("已提交任务 test-ope，等待执行结果"),
  ).toBeVisible();
  await page
    .getByRole("navigation", { name: "主导航" })
    .getByRole("button", { name: "容器", exact: true })
    .click();
  await page.getByLabel("节点环境").selectOption("C052");
  // 行尾只保留一个「更多」入口；演示中 C052 的第二个容器处于已停止状态
  const stopped = page
    .getByRole("region", { name: "Docker 容器", exact: true })
    .locator("tr.data-row", { hasText: "AstrBot" });
  await expect(stopped.locator(".ui-status")).toHaveText("已停止");
  await stopped
    .getByRole("button", { name: "更多操作", exact: true })
    .click();
  await page.getByRole("menuitem", { name: "启动", exact: true }).click();
  expect(submitted.at(-1).path).toBe("/api/v1/nodes/C052/actions");
  expect(submitted.at(-1).data.action).toMatchObject({
    type: "docker",
    container: "C052-1",
    operation: "start",
  });
});
test("防火墙首屏密度、分页、筛选及长文本", async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.goto(firewall);
  await expect(
    page.getByText("演示 · 不执行操作", { exact: true }),
  ).toBeVisible();
  const table = page.getByRole("region", { name: "防火墙规则", exact: true });
  await expect(table.locator("tr.data-row")).toHaveCount(50);
  const metrics = await table.locator("tr.data-row").evaluateAll((rows) => {
    const first = rows[0].getBoundingClientRect();
    const scroll = rows[0]
      .closest(".table-scroll")!
      .getBoundingClientRect();
    const head = rows[0]
      .closest("table")!
      .querySelector("thead")!
      .getBoundingClientRect();
    // 完全落在滚动区可视范围内、且不被表头遮挡的行才算完整可见
    const visible = rows.filter((row) => {
      const r = row.getBoundingClientRect();
      return (
        r.top >= head.bottom - 1 &&
        r.bottom <= scroll.bottom + 0.5 &&
        r.bottom <= innerHeight
      );
    }).length;
    return { top: first.top, height: first.height, visible, headHeight: head.height };
  });
  console.log("防火墙密度", metrics);
  expect(metrics.top).toBeLessThanOrEqual(220);
  // 设计定稿把数据行从 32px 提到 36px，同时压缩了表头以上的高度
  expect(metrics.height).toBe(36);
  expect(metrics.visible).toBeGreaterThanOrEqual(18);
  expect(
    await page
      .locator(".content")
      .evaluate((e) => getComputedStyle(e).fontSize),
  ).toBe("13px");
  await page.screenshot({
    path: "test-results/firewall-light.png",
    fullPage: true,
  });
  await table.getByRole("button", { name: "下一页" }).click();
  await expect(table.locator("tr.data-row")).toHaveCount(10);
  await page.getByLabel("搜索规则").fill("不存在的来源");
  await expect(table.getByText("没有匹配结果")).toBeVisible();
  await page.getByLabel("搜索规则").fill("");
  await page.getByLabel("筛选动作").selectOption("drop");
  await expect(table.locator("tr.data-row")).toHaveCount(10);
  await page.getByRole("tab", { name: "节点地址", exact: true }).click();
  await expect(
    page
      .getByRole("region", { name: "节点地址同步", exact: true })
      .getByText("离线 · 待同步"),
  ).toBeVisible();
});
test("亮暗主题文字对比度及暗色截图", async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.goto(firewall);
  for (const theme of ["light", "dark"]) {
    await page.evaluate((theme) => {
      document.documentElement.dataset.theme = theme;
    }, theme);
    const ratios = await page.evaluate(() => {
      const style = getComputedStyle(document.documentElement);
      const rgb = (key: string) => {
        let v = style.getPropertyValue(key).trim().slice(1);
        if (v.length === 3)
          v = v
            .split("")
            .map((c) => c + c)
            .join("");
        return [0, 2, 4]
          .map((i) => parseInt(v.slice(i, i + 2), 16) / 255)
          .map((v) =>
            v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4,
          );
      };
      const lum = (key: string) => {
        const c = rgb(key);
        return c[0] * 0.2126 + c[1] * 0.7152 + c[2] * 0.0722;
      };
      const pairs = [
        ["--text", "--panel"],
        ["--muted", "--panel"],
        ["--accent", "--panel"],
        ["--danger", "--danger-bg"],
        ["--warning", "--warning-bg"],
      ];
      return pairs.map(([a, b]) => {
        const x = lum(a),
          y = lum(b);
        return {
          pair: a + "/" + b,
          value: (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05),
        };
      });
    });
    for (const ratio of ratios)
      expect(ratio.value, theme + ": " + ratio.pair).toBeGreaterThanOrEqual(
        4.5,
      );
  }
  await page.screenshot({
    path: "test-results/firewall-dark.png",
    fullPage: true,
  });
});
test("抽屉焦点约束、恢复与演示无变更请求", async ({ page }) => {
  const writes: string[] = [];
  page.on("request", (request) => {
    if (["POST", "PUT", "PATCH", "DELETE"].includes(request.method()))
      writes.push(request.url());
  });
  await page.goto(firewall);
  const trigger = page.getByRole("button", { name: "新建规则", exact: true });
  await trigger.click();
  const dialog = page.getByRole("dialog");
  await expect(dialog).toBeVisible();
  await dialog.getByLabel("来源 CIDR").fill("192.0.2.0/24");
  await dialog.getByRole("button", { name: "生成计划", exact: true }).click();
  await expect(
    page.getByText("演示模式：表单已校验，未提交服务器操作"),
  ).toBeVisible();
  for (let i = 0; i < 18; i++) {
    await page.keyboard.press("Tab");
    expect(
      await dialog.evaluate((d) => d.contains(document.activeElement)),
    ).toBeTruthy();
  }
  await page.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0);
  await expect(trigger).toBeFocused();
  expect(writes).toEqual([]);
});
test("各页复用组件、错误状态和终端收起保留", async ({ page }) => {
  await page.goto(base + "/?demo=1&page=nodes");
  await expect(page.locator("tr.data-row")).toHaveCount(12);
  await page
    .getByRole("navigation", { name: "主导航" })
    .getByRole("button", { name: "容器", exact: true })
    .click();
  await page.getByLabel("节点环境").selectOption("C052");
  await expect(
    page.getByRole("button", { name: "Vaultwarden", exact: true }),
  ).toBeVisible();
  await page
    .getByRole("region", { name: "Docker 容器", exact: true })
    .getByRole("button", { name: "更多操作", exact: true })
    .first()
    .click();
  await page.getByRole("menuitem", { name: "查看日志", exact: true }).click();
  await expect(page.getByRole("dialog")).toBeVisible();
  await expect(page.locator(".terminal-surface")).toHaveCount(1);
  await page.getByRole("button", { name: "收起会话", exact: true }).click();
  await page
    .getByRole("navigation", { name: "主导航" })
    .getByRole("button", { name: "控制台", exact: true })
    .click();
  await expect(page.locator(".terminal-surface")).toHaveCount(1);
  await page.getByRole("button", { name: "容器会话 1 个" }).click();
  await expect(page.getByRole("dialog")).toBeVisible();
  await page.getByRole("button", { name: "结束当前会话" }).click();
  await expect(page.locator(".terminal-surface")).toHaveCount(0);
  await page
    .getByRole("navigation", { name: "主导航" })
    .getByRole("button", { name: "防火墙", exact: true })
    .click();
  await page.getByLabel("节点环境").selectOption("C081");
  await expect(
    page.getByRole("alert").filter({ hasText: "读取失败" }),
  ).toBeVisible();
  await expect(page.getByText("没有受管规则；外部规则未计入此表")).toHaveCount(
    0,
  );
  await page.getByLabel("节点环境").selectOption("C062");
  await expect(
    page.getByRole("alert").filter({ hasText: "维护来源发生外部变化" }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "新建规则", exact: true }),
  ).toBeDisabled();
});
test("移动端页面无横向溢出且表格独立滚动", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto(firewall);
  for (const name of ["防火墙", "容器", "节点", "控制台", "数据库", "存储", "设置"]) {
    await page
      .getByRole("navigation", { name: "主导航" })
      .getByRole("button", { name, exact: true })
      .click();
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
      name,
    ).toBeTruthy();
  }
  await page
    .getByRole("navigation", { name: "主导航" })
    .getByRole("button", { name: "防火墙", exact: true })
    .click();
  expect(
    await page
      .locator(".table-scroll")
      .evaluate((e) => e.scrollWidth > e.clientWidth),
  ).toBeTruthy();
  await page.screenshot({
    path: "test-results/firewall-mobile.png",
    fullPage: true,
  });
});
test("组件样板区分加载、失败、空数据与无匹配", async ({ page }) => {
  await page.goto(base + "/?demo=1&components=1");
  await expect(
    page.getByRole("region", { name: "组件样板表格", exact: true }),
  ).toBeVisible();
  for (const [value, text] of [
    ["loading", "正在读取…"],
    ["error", "读取失败：模拟读取错误"],
    ["unknown", "尚未采集，资源数量未知"],
    ["empty", "暂无资源"],
  ]) {
    await page.getByLabel("样板数据状态").selectOption(value);
    await expect(page.getByText(text, { exact: true })).toBeVisible();
  }
  await page.getByLabel("样板数据状态").selectOption("data");
  await page.getByLabel("筛选样板").fill("不匹配");
  await expect(page.getByText("没有匹配结果")).toBeVisible();
});
