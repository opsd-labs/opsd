import { describe, expect, it } from "vitest";
import {
  UNKNOWN,
  dbAgeHeader,
  dbBinlog,
  dbBytes,
  dbDuration,
  dbLatency,
  dbOnOff,
  dbRatio,
  dbSince,
  dbToneColor,
  dbToneText,
  dbUntil,
  dbValue,
} from "./databasePresentation";

/**
 * 这些用例守的是一条约束：**未知不等于 0，也不等于正常**。
 * 每一条都刻意把 null/undefined 与真实的 0 放在一起对照——
 * 把两者混起来正是运维视图最危险的假象。
 */
describe("数据库视图：0 与「未知」必须分开", () => {
  it("数值缺失显示未知，而真实的 0 显示 0", () => {
    expect(dbValue(null)).toBe(UNKNOWN);
    expect(dbValue(undefined)).toBe(UNKNOWN);
    expect(dbValue(0)).toBe("0");
    // 队列长度为 0 是"队列是空的"，没采集到是"不知道"，两者不能混
    expect(dbValue(0, " 条")).toBe("0 条");
    expect(dbValue(null, " 条")).toBe(UNKNOWN);
  });

  it("布尔事实缺失不默认成否", () => {
    expect(dbOnOff(null)).toBe(UNKNOWN);
    expect(dbOnOff(undefined)).toBe(UNKNOWN);
    expect(dbOnOff(true)).toBe("是");
    expect(dbOnOff(false)).toBe("否");
  });

  it("时长缺失不显示 0 分", () => {
    expect(dbDuration(null)).toBe(UNKNOWN);
    expect(dbDuration(0)).toBe("—"); // 0 秒本身没有可读时长
    expect(dbDuration(3600)).toBe("1 小时 0 分");
  });

  it("新鲜度：没有成功记录时不能说刚刚成功", () => {
    expect(dbSince(null, 1_000)).toBe(UNKNOWN);
    expect(dbSince(0, 1_000)).toBe(UNKNOWN);
    expect(dbSince(400, 1_000)).toBe("10 分前");
    // 一分钟以内说「刚刚」，而不是「0 分前」
    expect(dbSince(990, 1_000)).toBe("刚刚");
    // 时钟偏差导致的负数要夹到 0，而不是显示"-5 分前"
    expect(dbSince(1_300, 1_000)).toBe("刚刚");
  });

  it("定时器：没有下次触发时显示未知", () => {
    expect(dbUntil(null, 1_000)).toBe(UNKNOWN);
    expect(dbUntil(0, 1_000)).toBe(UNKNOWN);
    expect(dbUntil(1_600, 1_000)).toBe("10 分后");
    expect(dbUntil(1_020, 1_000)).toBe("即将触发");
  });

  it("体积：0 字节显示 0 B，没采集到显示未知", () => {
    expect(dbBytes(0)).toBe("0 B");
    expect(dbBytes(null)).toBe(UNKNOWN);
    expect(dbBytes(1_048_576)).toBe("1.0 MiB");
  });

  it("延迟：微秒换算成毫秒，缺失不补 0", () => {
    expect(dbLatency(321)).toBe("0.3 ms");
    expect(dbLatency(1_500)).toBe("1.5 ms");
    expect(dbLatency(null)).toBe(UNKNOWN);
  });

  it("流控比例缺失时不显示 0.0000", () => {
    // 0.0000 是"没有暂停"，未知是"不知道有没有暂停"，语义完全不同
    expect(dbRatio(0)).toBe("0.0000");
    expect(dbRatio(null)).toBe(UNKNOWN);
  });

  it("归档头：没检查过与检查不通过是两回事", () => {
    expect(dbAgeHeader(null)).toBe(UNKNOWN);
    expect(dbAgeHeader(true)).toBe("正常");
    expect(dbAgeHeader(false)).toBe("不是 age 格式");
  });

  it("binlog：没开启是「没有」，位点缺失才是未知", () => {
    expect(dbBinlog(null, null)).toBe("未开启 binlog");
    expect(dbBinlog("mariadb-bin.000042", 812_345_678)).toBe(
      "mariadb-bin.000042:812345678",
    );
    expect(dbBinlog("mariadb-bin.000042", null)).toBe(
      `mariadb-bin.000042:${UNKNOWN}`,
    );
  });
});

describe("数据库视图：结论用词", () => {
  it("三种语气各有明确说法", () => {
    expect(dbToneText("critical")).toBe("需要立即处理");
    expect(dbToneText("warning")).toBe("需要注意");
    expect(dbToneText("ok")).toBe("未发现异常");
  });

  it("语气映射到状态色，未知不参与其中", () => {
    expect(dbToneColor("critical")).toBe("danger");
    expect(dbToneColor("warning")).toBe("warning");
    expect(dbToneColor("ok")).toBe("success");
  });
});
