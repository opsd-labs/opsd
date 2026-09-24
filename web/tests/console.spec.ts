import { test, expect } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";
const base = "http://127.0.0.1:5173";

test("一级菜单为精简后的七项，任务不再占用一级入口", async ({ page }) => {
  await page.goto(base + "/?demo=1&page=console");
  const nav = page.getByRole("navigation", { name: "主导航" });
  await expect(nav.getByRole("button")).toHaveText([
    "控制台",
    "节点",
    "数据库",
    "容器",
    "防火墙",
    "存储",
    "设置",
  ]);
  await expect(nav.getByRole("button", { name: "任务", exact: true })).toHaveCount(0);
  // 任务改由顶栏面板承载
  await page.getByRole("button", { name: /任务/ }).first().click();
  await expect(page.getByRole("dialog")).toBeVisible();
  await expect(page.getByRole("dialog")).toContainText("任务与异常");
});

test("旧地址 page=overview 归入控制台", async ({ page }) => {
  await page.goto(base + "/?demo=1&page=overview");
  await expect(
    page.getByRole("region", { name: "全部节点状态", exact: true }),
  ).toBeVisible();
});

test("控制台展示全部节点，离线、读取失败与未上报互不混同", async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.goto(base + "/?demo=1&page=console");
  const table = page.getByRole("region", { name: "全部节点状态", exact: true });
  await expect(table.locator("tr.data-row")).toHaveCount(12);
  // 列顺序：节点 连接 指标 EasyTier CPU 内存 磁盘 运行时长 容器 防火墙 地址同步 操作
  const cells = (row: ReturnType<typeof table.locator>) =>
    row.locator("td");

  // C061 离线，且指标因长时间没有新采样而陈旧
  const offline = table.locator("tr.data-row", { hasText: "C061" });
  await expect(cells(offline).nth(1)).toHaveText("离线快照");
  await expect(cells(offline).nth(2)).toHaveText("数据陈旧");

  // C081 两个采集源都读取失败，不得显示为 0 或「未知」
  const failed = table.locator("tr.data-row", { hasText: "C081" });
  await expect(cells(failed).nth(2)).toHaveText("采集失败");
  await expect(cells(failed).nth(8)).toHaveText("读取失败");
  await expect(cells(failed).nth(9)).toHaveText("读取失败");

  // C101 模拟尚未升级的 Agent：没有指标能力位
  const legacy = table.locator("tr.data-row", { hasText: "C101" });
  await expect(cells(legacy).nth(2)).toHaveText("未上报指标");

  // 正常节点显示真实百分比，而不是占位
  const healthy = table.locator("tr.data-row", { hasText: "C052" });
  await expect(cells(healthy).nth(2)).toHaveText("正常");
  await expect(cells(healthy).nth(4)).toHaveText(/%$/);
  await expect(cells(healthy).nth(5)).toHaveText(/%$/);
  await expect(cells(healthy).nth(6)).toHaveText(/%$/);
  // 采集失败的节点在数值列显示占位，绝不显示 0
  await expect(cells(failed).nth(4)).toHaveText("—");
});

test("状态标签为轻底语义色并带符号，灰度下仍可区分", async ({ page }) => {
  await page.goto(base + "/?demo=1&page=console");
  const badge = page
    .getByRole("region", { name: "全部节点状态", exact: true })
    .locator(".ui-status")
    .first();
  await expect(badge.locator("svg.status-mark")).toHaveCount(1);
  const style = await badge.evaluate((el) => {
    const s = getComputedStyle(el);
    return { background: s.backgroundColor, color: s.color, height: el.getBoundingClientRect().height };
  });
  // 轻底：不是透明，也与正文色不同
  expect(style.background).not.toBe("rgba(0, 0, 0, 0)");
  expect(style.background).not.toBe(style.color);
  expect(style.height).toBeGreaterThanOrEqual(22);
});

test("需要关注区直接列出离线、读取失败与异常任务", async ({ page }) => {
  await page.goto(base + "/?demo=1&page=console");
  const attention = page.getByRole("region", { name: "需要关注" });
  await expect(attention).toContainText("节点离线");
  await expect(attention).toContainText("容器资源读取失败");
  await expect(attention).toContainText("防火墙读取失败");
});

test("维护来源区分受管与只读，未接管不得显示为 opsd", async ({ page }) => {
  await page.goto(base + "/?demo=1&page=firewall&node=C052");
  const owned = page
    .getByRole("region", { name: "防火墙规则", exact: true })
    .locator(".affiliation")
    .first();
  await expect(owned).toHaveText("opsd");
  await page.goto(base + "/?demo=1&page=firewall&node=C001");
  const readOnly = page
    .getByRole("region", { name: "防火墙规则", exact: true })
    .locator(".affiliation")
    .first();
  await expect(readOnly).toHaveText("1Panel · 只读");
  await expect(readOnly).toHaveAttribute("title", /只读/);
});

test("防火墙规则表首行不超过 220px，并在 1440×900 完整显示至少 18 条 36px 规则", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.goto(base + "/?demo=1&page=firewall&node=C052");
  const table = page.getByRole("region", { name: "防火墙规则", exact: true });
  await expect(table.locator("tr.data-row")).toHaveCount(50);
  const metrics = await table.locator("tr.data-row").evaluateAll((rows) => {
    const first = rows[0].getBoundingClientRect();
    const height = first.height;
    const visible = rows.filter(
      (r) => r.getBoundingClientRect().bottom <= window.innerHeight,
    ).length;
    return { top: first.top, height, visible };
  });
  expect(metrics.top).toBeLessThanOrEqual(220);
  expect(metrics.height).toBe(36);
  expect(metrics.visible).toBeGreaterThanOrEqual(18);
});

test("容器列表按节点组织，端口显示映射方向且镜像缩写不丢完整地址", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.goto(base + "/?demo=1&page=docker");
  const all = page.getByRole("region", { name: "Docker 容器", exact: true });
  // 全部环境视图保留节点列
  await expect(all.locator("thead th", { hasText: "所属节点" })).toHaveCount(1);
  await page.getByLabel("节点环境").selectOption("C052");
  const single = page.getByRole("region", { name: "Docker 容器", exact: true });
  // 单节点视图省略节点列
  await expect(single.locator("thead th", { hasText: "所属节点" })).toHaveCount(0);
  await expect(single.locator("thead th", { hasText: "Stack" })).toHaveCount(0);
  // 端口以映射方向呈现
  await expect(single.locator(".port-map").first()).toContainText("→");
});

test("节点指标面板展示曲线与四种指标状态", async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.goto(base + "/?demo=1&page=nodes");

  // 正常节点：展开后出现曲线、汇总与挂载点
  await page.getByRole("button", { name: "C052 · 日本", exact: true }).click();
  const detail = page.locator(".node-detail");
  await expect(detail).toBeVisible();
  await expect(detail.locator(".metrics-chart")).toHaveCount(6);
  await expect(detail.locator(".metrics-chart svg path.line").first()).toHaveCount(1);
  await expect(
    detail.getByRole("heading", { name: "挂载点", exact: true }),
  ).toBeVisible();
  await expect(
    detail.getByRole("heading", { name: "网络接口", exact: true }),
  ).toBeVisible();
  // 曲线说明里给出了采样点数量，说明确实画了数据
  await expect(detail.locator(".metrics-chart figcaption").first()).toContainText(
    "个采样点",
  );

  // 切换区间不应报错
  await detail.getByRole("button", { name: "近 7 天", exact: true }).click();
  await expect(detail.locator(".metrics-chart").first()).toBeVisible();

  // 采集失败的节点：明确报失败，且数值列不留 0
  await page.getByRole("button", { name: "C081 · 新加坡", exact: true }).click();
  const failed = page.locator(".node-detail");
  await expect(failed.getByText("指标采集失败", { exact: false })).toBeVisible();
  await expect(failed.locator(".metric-tile strong").first()).toHaveText("—");

  // 旧 Agent 没有能力位：显示未上报，而不是失败或零
  await page.getByRole("button", { name: "C101 · 香港", exact: true }).click();
  const legacy = page.locator(".node-detail");
  await expect(
    legacy.getByText("未上报指标", { exact: false }).first(),
  ).toBeVisible();
  await expect(
    legacy.getByText("该 Agent 版本没有指标采样能力", { exact: false }),
  ).toBeVisible();
});

test("节点页提供堡垒机终端与结构化文件管理", async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.goto(base + "/?demo=1&page=nodes");
  await page.getByRole("button", { name: "C052 · 日本", exact: true }).click();
  const workbench = page.locator(".node-workbench");
  await expect(workbench).toBeVisible();

  // 宿主机终端：演示模式明确说明未连接服务器
  const shell = workbench.locator(".host-shell");
  await expect(shell.locator(".terminal-host .xterm")).toHaveCount(1);
  await expect(shell.locator(".terminal-host")).toContainText("演示模式");
  // 只允许白名单内的 shell，界面必须说明这一点
  await expect(
    shell.getByText("宿主机终端以 Agent 的权限运行", { exact: false }),
  ).toBeVisible();
  await expect(shell.getByLabel("起始目录")).toBeVisible();

  // 文件管理：目录列表区分类型，符号链接单列
  const files = workbench.locator(".file-manager");
  await expect(files.getByText("符号链接").first()).toBeVisible();
  await expect(files.locator(".file-table tbody tr")).toHaveCount(4);
  // 边界说明常驻可见，便于核对拒绝范围
  await expect(
    files.getByText("拒绝符号链接", { exact: false }),
  ).toBeVisible();
  await expect(
    files.getByText("失败不会破坏原文件", { exact: false }),
  ).toBeVisible();
  // 变更操作有明确入口
  await expect(files.getByLabel("新目录名称")).toBeVisible();
  await expect(files.getByLabel("新名称")).toBeVisible();
});

test("线路质量表把没测到的延迟显示为未知而不是 0", async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 1000 });
  await page.goto(base + "/?demo=1&page=nodes");
  await page.getByRole("button", { name: "C052 · 日本", exact: true }).click();

  const quality = page
    .locator(".metric-tables > div", { hasText: "线路质量" })
    .first();
  await expect(quality).toBeVisible();
  // 探测比采样稀疏，必须说清数字是什么时候测的
  await expect(quality).toContainText("每 60 秒探测一次");
  await expect(quality).toContainText("没测到就是未知");

  const rows = quality.locator("tbody tr");
  await expect(rows).toHaveCount(4);

  // 测到的显示毫秒值
  const reached = rows.filter({ hasText: "100.100.201.41" });
  await expect(reached).toContainText("182.6 ms");
  await expect(reached).toContainText("可达");

  // 没测到的必须是「未知」+ 原因，绝不能是 0 ms
  const lost = rows.filter({ hasText: "100.100.201.71" });
  await expect(lost).toContainText("未知");
  await expect(lost).toContainText("100% 丢包");
  await expect(lost).not.toContainText("0.0 ms");

  // 工具缺失与网络不通是两回事，必须分开说
  const noPing = rows.filter({ hasText: "100.100.201.91" });
  await expect(noPing).toContainText("没有 ping 命令");
  await expect(noPing).not.toContainText("0.0 ms");
  await expect(quality).toContainText("最近一次探测");
});

test("存储预检给出结论式报告而不是分数", async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.goto(base + "/?demo=1&page=storage");
  const verdict = page.locator(".storage-verdict");
  await expect(verdict).toBeVisible();
  await expect(verdict).toContainText("暂不满足部署条件");
  // 预检必须是只读的，这一点要写在页面上
  await expect(
    verdict.getByText("预检只读", { exact: false }),
  ).toBeVisible();

  // 有节点没采集时，结论必须明说"还不完整"，而不是假装不满足条件
  await expect(verdict).toContainText("尚未采集");

  // 一致性检查逐条给出结论与说明
  const checks = page.locator(".storage-checks li");
  await expect(checks).toHaveCount(6);
  await expect(checks.first()).toContainText("候选节点不少于 4 台");
  await expect(checks.first()).toContainText("4 台已确认可用");
  // 每条检查都要同时有结论徽标和说明文字，不能只报一个名字
  for (const check of await checks.all()) {
    await expect(check.locator(".ui-status")).toBeVisible();
    await expect(check.locator(".check-name")).not.toBeEmpty();
    await expect(check.locator(".muted")).not.toBeEmpty();
  }

  // 四种结论都要能出现在同一页上，且待采集不等于不适合
  const labels = await page.locator(".storage-node .ui-status").allInnerTexts();
  expect(labels).toContain("适合部署");
  expect(labels).toContain("需要先处理");
  expect(labels).toContain("不适合");
  expect(labels).toContain("待采集");
  // 每个节点都必须给出原因，而不是只有一个结论
  for (const node of await page.locator(".storage-node").all()) {
    await expect(node.locator(".storage-reasons li").first()).toBeVisible();
  }

  // 展开后能看到磁盘明细与排除原因
  await page.locator(".storage-node").first().locator("header").click();
  const detail = page.locator(".storage-detail").first();
  await expect(detail.getByText("可用于存储集群")).toBeVisible();
  await expect(detail.getByText("已排除的磁盘")).toBeVisible();

  // 危险操作的边界必须写清楚
  await expect(
    page.getByText("只批准没有任何先前使用痕迹的磁盘", { exact: false }),
  ).toBeVisible();
});

test("存储集群定义与破坏性计划必须显式确认", async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.goto(base + "/?demo=1&page=storage");

  // 集群定义区可用，但演示模式不会真的生成计划
  await expect(page.getByLabel("集群名称")).toHaveValue("silo-prod");
  await expect(page.getByLabel("镜像标签")).toBeVisible();
  // 镜像仓库固定，且明示不接受 latest
  await expect(page.getByText("不接受 latest", { exact: false })).toBeVisible();
  // 节点选择只列出预检判定为适合的节点
  const picker = page.locator(".node-picker");
  await expect(picker.getByText("C001 · 美国", { exact: false })).toBeVisible();
  await expect(picker.getByText("C052 · 日本", { exact: false })).toHaveCount(0);

  // 演示模式不得生成计划，也不得下发任务
  await page.getByRole("button", { name: "生成部署计划", exact: true }).click();
  await expect(page.locator(".storage-plan")).toHaveCount(0);
  await expect(page.getByText("未生成计划", { exact: false })).toBeVisible();
  await expect(page.getByText("不会下发任何任务", { exact: false })).toBeVisible();

  // 破坏性操作的边界说明必须常驻
  await expect(
    page.getByText("任一不符即整节点拒绝", { exact: false }),
  ).toHaveCount(0); // 没有计划时不展示计划细则
  await expect(
    page.getByText("只批准没有任何先前使用痕迹的磁盘", { exact: false }),
  ).toBeVisible();
});

test("数据库页只读：未知不等于正常，也不提供任何写入口", async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 1000 });
  await page.goto(base + "/?demo=1&page=database");

  // 结论式报告：先给结论，再给跨节点一致性检查
  const verdict = page.locator(".storage-verdict").first();
  await expect(verdict).toBeVisible();
  await expect(verdict).toContainText("需要立即处理");
  await expect(verdict).toContainText("未知");
  // 只读边界必须写在页面上
  await expect(verdict).toContainText("不建库、不建用户、不改配置");
  await expect(verdict).toContainText("密码留在节点本机");

  // 只有把节点放在一起才能发现的问题必须单独列出
  const checks = page.locator(".storage-checks li");
  await expect(checks).toHaveCount(2);
  await expect(checks.first()).toContainText("各节点看到的成员数不一致");
  await expect(checks.first()).toContainText("通常意味着有节点掉线");

  // C071 从未巡检：必须显示「尚未巡检」+ 原因，不能显示成 0 或正常
  const unchecked = page.locator(".storage-node", { hasText: "C071" });
  await expect(unchecked).toContainText("尚未巡检");
  await expect(unchecked.locator(".db-unknown")).toContainText("尚未巡检");

  // C052 看到的成员数不对 → 硬性异常；ProxySQL 未配置 → 未知项单独列出
  const mismatch = page.locator(".storage-node", { hasText: "C052" });
  await expect(mismatch).toContainText("需要立即处理");
  await expect(mismatch).toContainText("集群成员数与期望不符");
  await expect(mismatch.locator(".db-unknown")).toContainText("ProxySQL 未知");

  // C041 队列积压是需要注意，而不是异常
  const warn = page.locator(".storage-node", { hasText: "C041" });
  await expect(warn).toContainText("需要注意");
  await expect(warn).toContainText("接收队列积压");

  // 展开后能看到事实明细，缺失值显示「未知」而不是 0
  await mismatch.locator("header").click();
  const detail = mismatch.locator(".storage-detail");
  await expect(detail).toContainText("wsrep");
  await expect(detail).toContainText("last_committed");
  // C052 没有配置 ProxySQL 只读账号：这里必须写清原因，而不是显示 0 个后端
  await expect(detail).toContainText("未配置这一部分的只读巡检账号");
  await expect(detail).toContainText("成员数4 / 期望 5");

  // C001 一切正常：展开后能看到 ProxySQL 后端与最重的查询摘要
  const healthy = page.locator(".storage-node", { hasText: "C001" });
  await healthy.locator("header").click();
  const healthyDetail = healthy.locator(".storage-detail");
  await expect(healthyDetail).toContainText("100.100.201.41:3306");
  await expect(healthyDetail).toContainText("最重的查询摘要");
  await expect(healthyDetail).toContainText("0A1B2C3D4E5F6071");
  // 摘要只给哈希与计数：整页都不许出现 SQL 正文
  await expect(page.locator("body")).not.toContainText("digest_text");
  await expect(page.locator("body")).not.toContainText("SELECT *");

  // 未知项计数必须提醒人，而不是被折叠掉
  await expect(
    page.locator(".ui-notice", { hasText: "未知不是正常" }),
  ).toBeVisible();

  // 页面上不得出现任何写数据库的入口
  for (const forbidden of ["建库", "建用户", "改配置", "触发 SST", "kill", "写入"]) {
    const buttons = page.getByRole("button", { name: forbidden });
    // 「不建库、不建用户、不改配置、不触发 SST」只出现在说明文字里
    await expect(buttons).toHaveCount(0);
  }
});

test("数据库页在演示模式下不发起任何变更请求", async ({ page }) => {
  const changes: string[] = [];
  page.on("request", (request) => {
    if (request.method() !== "GET") changes.push(`${request.method()} ${request.url()}`);
  });
  await page.goto(base + "/?demo=1&page=database");
  await page.getByRole("button", { name: "全部节点巡检" }).click();
  await page.getByRole("button", { name: "巡检" }).first().click();
  await expect(
    page.locator(".ui-notice", { hasText: "演示模式：未提交巡检任务" }),
  ).toBeVisible();
  expect(changes).toEqual([]);
});

test("数据库页在首屏就能看到全部节点的结论", async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.goto(base + "/?demo=1&page=database");
  const metrics = await page.locator(".storage-node").evaluateAll((nodes) => {
    const first = nodes[0].getBoundingClientRect();
    const visible = nodes.filter(
      (node) => node.getBoundingClientRect().bottom <= innerHeight,
    ).length;
    const row = nodes[0].querySelector("table.file-table td");
    return {
      top: first.top,
      height: first.height,
      visible,
      total: nodes.length,
      rowHeight: row ? row.getBoundingClientRect().height : 0,
    };
  });
  console.log("数据库页密度", metrics);
  // 首屏要能同时看到"结论"和"全部节点"，否则运维就得先滚动才敢判断
  expect(metrics.visible).toBe(5);
  expect(metrics.total).toBe(5);
  await expect(page.locator(".storage-verdict").first()).toBeVisible();
  await expect(page.locator(".storage-checks")).toBeVisible();
  expect(metrics.top).toBeLessThanOrEqual(400);
  // 明细表格沿用 36px 数据行
  await page.locator(".storage-node").first().locator("header").click();
  expect(
    await page
      .locator(".storage-detail table.file-table td")
      .first()
      .evaluate((e) => e.getBoundingClientRect().height),
  ).toBe(36);
  await expect(
    page.locator(".storage-node", { hasText: "C001" }).locator(".storage-detail"),
  ).toBeVisible();
});

test("控制台、数据库、存储与防火墙页面通过自动无障碍检查", async ({ page }) => {
  // 每页等到"内容确实渲染出来"再扫，否则扫的是空壳
  for (const [url, ready] of [
    [base + "/?demo=1&page=console", "tr.data-row"],
    [base + "/?demo=1&page=firewall&node=C052", "tr.data-row"],
    [base + "/?demo=1&page=database", ".storage-node"],
    [base + "/?demo=1&page=storage", ".storage-node"],
  ]) {
    await page.goto(url);
    await expect(page.locator(ready).first()).toBeVisible();
    const result = await new AxeBuilder({ page })
      .withTags(["wcag2a", "wcag2aa", "wcag21aa"])
      .analyze();
    expect(
      result.violations.map((v) => ({
        id: v.id,
        help: v.help,
        nodes: v.nodes.map((n) => n.target),
      })),
      url,
    ).toEqual([]);
  }
});
