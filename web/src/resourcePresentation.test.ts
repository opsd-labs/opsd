import { describe, expect, it } from "vitest";
import {
  chooseStep,
  clockSkewExceeded,
  finiteNumber,
  firewallPorts,
  formatBytes,
  formatPercent,
  formatUptime,
  metricsStatus,
  ownership,
  parseDockerPorts,
  parseUnpublishedPorts,
  protocolLabel,
  resourceStatus,
  resultStatus,
  segmentSeries,
  usagePercent,
} from "./resourcePresentation";

describe("端口解析", () => {
  it("解析通配绑定与具体绑定", () => {
    expect(parseDockerPorts("0.0.0.0:8080 → 80/tcp")).toEqual([
      { address: "0.0.0.0", host: "8080", container: "80", protocol: "tcp" },
    ]);
    expect(parseDockerPorts("127.0.0.1:5432 -> 5432/udp")).toEqual([
      { address: "127.0.0.1", host: "5432", container: "5432", protocol: "udp" },
    ]);
  });

  it("支持 IPv6 绑定与端口范围", () => {
    expect(parseDockerPorts("[::1]:9000 → 9000/tcp")).toEqual([
      { address: "[::1]", host: "9000", container: "9000", protocol: "tcp" },
    ]);
    expect(parseDockerPorts("8000-8010 → 80/tcp")).toEqual([
      { address: undefined, host: "8000-8010", container: "80", protocol: "tcp" },
    ]);
  });

  it("多渠道映射按分隔符拆分", () => {
    expect(parseDockerPorts("80:80/tcp;443:443/tcp")).toHaveLength(2);
  });

  it("无法解析或越界时返回 null，由调用方保留原文", () => {
    for (const bad of [
      "not-a-port",
      "0.0.0.0:99999 → 80/tcp",
      "0.0.0.0:80 → 70000/tcp",
      "999.1.1.1:80 → 80/tcp",
      "8000-7000 → 80/tcp",
      "80 → 80/http",
    ])
      expect(parseDockerPorts(bad), bad).toBeNull();
  });

  it("空值与未发布端口分开处理", () => {
    expect(parseDockerPorts("")).toEqual([]);
    expect(parseDockerPorts(null)).toEqual([]);
    expect(parseUnpublishedPorts("80/tcp（未发布）")).toEqual([
      { port: "80", protocol: "tcp" },
    ]);
    expect(parseUnpublishedPorts("乱码")).toBeNull();
  });

  it("防火墙端口把 any 显示为全部", () => {
    expect(protocolLabel("any")).toBe("全部");
    expect(protocolLabel("tcp")).toBe("TCP");
    expect(protocolLabel("")).toBe("全部");
    expect(firewallPorts([22, 443])).toBe("22,443");
    expect(firewallPorts([])).toBe("全部");
    expect(firewallPorts(null)).toBe("全部");
  });
});

describe("状态判定", () => {
  it("离线优先于具体状态", () => {
    expect(resourceStatus({ kind: "container", offline: true, state: "running" }))
      .toMatchObject({ text: "离线快照", mark: "pending" });
    // 地址同步在离线时仍要表达「待同步」，不是单纯的快照
    expect(resourceStatus({ kind: "sync", offline: true, enabled: true }))
      .toMatchObject({ text: "离线 · 待同步", mark: "pending" });
  });

  it("未知不得归入已停止或成功", () => {
    expect(resourceStatus({ kind: "container" })).toMatchObject({
      text: "未知",
      mark: "unknown",
    });
    expect(resourceStatus({ kind: "rule" })).toMatchObject({ text: "未知" });
    expect(resourceStatus({ kind: "node" })).toMatchObject({ text: "未知" });
  });

  it("未知优先于已采集但状态缺失", () => {
    expect(resourceStatus({ kind: "container", unknown: true, state: "exited" }))
      .toMatchObject({ text: "未知" });
  });

  it("容器状态映射完整", () => {
    const expected: Record<string, string> = {
      running: "运行中",
      exited: "已停止",
      paused: "已暂停",
      created: "已创建",
      restarting: "重启中",
      dead: "异常",
    };
    for (const [state, text] of Object.entries(expected))
      expect(resourceStatus({ kind: "container", state }).text, state).toBe(text);
  });

  it("规则与服务状态区分启用与停用", () => {
    expect(resourceStatus({ kind: "rule", enabled: true })).toMatchObject({
      text: "启用",
      mark: "active",
    });
    expect(resourceStatus({ kind: "rule", enabled: false })).toMatchObject({
      text: "停用",
      mark: "paused",
    });
    expect(resourceStatus({ kind: "sync", enabled: false })).toMatchObject({
      text: "待同步",
      mark: "pending",
    });
  });

  it("结果文案映射到语义色", () => {
    expect(resultStatus("已同步")).toMatchObject({ tone: "success" });
    expect(resultStatus("待同步")).toMatchObject({ tone: "warning" });
    expect(resultStatus("冲突")).toMatchObject({ tone: "danger" });
    expect(resultStatus("只读发现")).toMatchObject({ tone: "neutral" });
    expect(resultStatus(undefined)).toMatchObject({ mark: "unknown" });
  });
});

describe("维护来源", () => {
  it("区分受管、系统与只读来源", () => {
    expect(ownership("opsd")).toBe("opsd");
    expect(ownership("Docker")).toBe("Docker · 系统");
    expect(ownership("1Panel")).toBe("1Panel · 只读");
    expect(ownership(null)).toBe("维护来源未知");
    expect(ownership("")).toBe("维护来源未知");
  });
});

describe("指标状态", () => {
  const capable = { capabilities: { metrics: true } };

  it("四种未就绪情况互不冒充", () => {
    // 旧 Agent 没有能力位
    expect(metricsStatus({ capabilities: {} }, null, 100)).toMatchObject({
      state: "unsupported",
      text: "未上报指标",
    });
    // 有能力位但还没有第一份采样
    expect(metricsStatus(capable, null, 100)).toMatchObject({
      state: "pending",
      text: "尚未上报",
    });
    // 采集失败必须报失败，不能显示 0
    expect(
      metricsStatus(capable, { received_at: 100, error: "读取 /proc 失败" }, 100),
    ).toMatchObject({ state: "error", text: "采集失败", tone: "danger" });
    // 采样在但已过期
    expect(
      metricsStatus(capable, { received_at: 10, sample: {} }, 1000),
    ).toMatchObject({ state: "stale", text: "数据陈旧" });
  });

  it("正常上报才算正常", () => {
    expect(
      metricsStatus(capable, { received_at: 1000, sample: {}, error: null }, 1010),
    ).toMatchObject({ state: "ok", text: "正常", tone: "success" });
  });

  it("时钟偏移超过阈值时提示", () => {
    expect(clockSkewExceeded({ clock_offset: 61 }, 60)).toBe(true);
    expect(clockSkewExceeded({ clock_offset: -61 }, 60)).toBe(true);
    expect(clockSkewExceeded({ clock_offset: 5 }, 60)).toBe(false);
    expect(clockSkewExceeded(null, 60)).toBe(false);
  });
});

describe("指标格式化", () => {
  it("字节数换算且缺值不显示为零", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(1536)).toBe("1.5 KiB");
    expect(formatBytes(1024 * 1024 * 5)).toBe("5.0 MiB");
    expect(formatBytes(1024 ** 4 * 3)).toBe("3.0 TiB");
    expect(formatBytes(null)).toBe("—");
    expect(formatBytes(-1)).toBe("—");
    expect(formatBytes(Number.NaN)).toBe("—");
  });

  it("百分比与使用率不因缺值或除零而变成零", () => {
    expect(formatPercent(12.34)).toBe("12.3%");
    expect(formatPercent(null)).toBe("—");
    expect(usagePercent(50, 200)).toBe(25);
    expect(usagePercent(1, 0)).toBeNull();
    expect(usagePercent(null, 100)).toBeNull();
    // 超出总量时封顶，不出现大于 100%
    expect(usagePercent(300, 200)).toBe(100);
  });

  it("运行时长按量级选择单位", () => {
    expect(formatUptime(0)).toBe("—");
    expect(formatUptime(600)).toBe("10 分");
    expect(formatUptime(3600 * 5 + 60)).toBe("5 小时 1 分");
    expect(formatUptime(86400 * 12 + 3600 * 3)).toBe("12 天 3 小时");
  });

  it("步长随区间增长且落在允许的层级边界上", () => {
    expect(chooseStep(3600)).toBe(15);
    expect(chooseStep(86400)).toBe(600);
    expect(chooseStep(86400 * 30)).toBe(21600);
    // 极大区间封顶在一天，避免点数爆炸
    expect(chooseStep(86400 * 400)).toBe(86400);
  });
});

describe("曲线分段", () => {
  const at = (n: number) => ({ at: n, cpu_usage: 10 });

  it("连续点留在同一段", () => {
    const segments = segmentSeries(
      [at(0), at(60), at(120), at(180)],
      "cpu_usage",
      60,
    );
    expect(segments).toHaveLength(1);
    expect(segments[0]).toHaveLength(4);
  });

  it("间隔超过两倍步长即断开，不补点也不连线", () => {
    // 0 与 60 连续；600 与 660 连续；中间 540 秒没有数据
    const segments = segmentSeries(
      [at(0), at(60), at(600), at(660)],
      "cpu_usage",
      60,
    );
    expect(segments).toHaveLength(2);
    expect(segments[0].map((p) => p.at)).toEqual([0, 60]);
    expect(segments[1].map((p) => p.at)).toEqual([600, 660]);
  });

  it("恰好两倍步长不算断档", () => {
    const segments = segmentSeries([at(0), at(120)], "cpu_usage", 60);
    expect(segments).toHaveLength(1);
  });

  it("缺失值被跳过而不是当作零", () => {
    // Number(null) 是 0，只靠 isFinite 会把缺失值画成真实的零
    const points = [
      { at: 0, cpu_usage: 5 },
      { at: 60, cpu_usage: null },
      { at: 120, cpu_usage: Number.NaN },
      { at: 180, cpu_usage: 7 },
    ];
    const values = segmentSeries(points as any, "cpu_usage", 60)
      .flat()
      .map((p) => p.value);
    expect(values).toEqual([5, 7]);
    expect(values).not.toContain(0);
  });

  it("单个缺失采样不会把曲线切断", () => {
    // 只在中间少了一个采样，前后都有数据，仍应连成一段
    const points = [
      { at: 0, cpu_usage: 5 },
      { at: 60, cpu_usage: undefined },
      { at: 120, cpu_usage: 6 },
    ];
    expect(segmentSeries(points as any, "cpu_usage", 60)).toHaveLength(1);
  });

  it("空串与 null 一样按缺失处理", () => {
    expect(finiteNumber("")).toBeNull();
    expect(finiteNumber(null)).toBeNull();
    expect(finiteNumber(undefined)).toBeNull();
    expect(finiteNumber(Number.NaN)).toBeNull();
    expect(finiteNumber("12.5")).toBe(12.5);
    // 真实的 0 必须保留
    expect(finiteNumber(0)).toBe(0);
  });

  it("孤立点自成一段，便于单独画点", () => {
    const segments = segmentSeries(
      [at(0), at(60), at(3000)],
      "cpu_usage",
      60,
    );
    expect(segments).toHaveLength(2);
    expect(segments[1]).toHaveLength(1);
  });

  it("空输入不产生任何段", () => {
    expect(segmentSeries([], "cpu_usage", 60)).toEqual([]);
  });
});
