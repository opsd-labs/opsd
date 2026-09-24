import type { Entry } from "./api";
import type { Task } from "./api";
const rows = [
  ["C001", "美国", "Pmail、MariaDB"],
  ["C011", "台湾", "Sub2API、Pocket ID"],
  ["C021", "香港", "EasyTier、GOST"],
  ["C041", "美国", "OpenList、Nowen Note"],
  ["C051", "广州", "FRP、Homepage"],
  ["C052", "日本", "Vaultwarden、AstrBot"],
  ["C061", "广州", "Gotify、FRPS"],
  ["C062", "深圳", "NapCat"],
  ["C071", "美国", "Komari、MariaDB"],
  ["C081", "新加坡", "图片渲染、GOST"],
  ["C091", "日本", "Forgejo、OpenList"],
  ["C101", "香港", "New API、EasyTier"],
];
export const demoNodes: Entry[] = rows.map(([id, area, services], i) => ({
  connected: i !== 6,
  node: {
    id,
    name: `${id} · ${area}`,
    public_addresses: [],
    overlay_address: `100.100.201.${Number(id.slice(1))}`,
    ssh_port: 22,
    revoked: false,
    last_seen: 0,
    address_version: 0,
    capabilities: {
      os: "linux",
      docker: true,
      compose: true,
      systemd: true,
      firewall: ["nftables"],
      metrics: true,
      probe: true,
    },
    inventory: {
      collected_at: 0,
      docker: {
        state: "ok",
        data: {
          containers: services
            .split("、")
            .map((name, j) => ({
              id: `${id}-${j}`,
              names: [name],
              image: `${name.toLowerCase()}:latest`,
              state: "running",
              status: "演示状态",
              project: name.toLowerCase(),
              protected: name.includes("MariaDB"),
              ports: [],
              mounts: [],
            })),
          images: [],
          networks: [],
          volumes: [],
          version: { Version: "演示" },
        },
      },
      firewall: {
        state: "ok",
        data: {
          backend: i % 2 ? "ufw" : "nftables",
          policy: {
            adopted: false,
            rules: [],
            peers: { version: 0, addresses: [] },
          },
          external_rules: {},
        },
      },
      stacks: [],
    },
  },
}));
// 文档示例地址仅供视觉与交互验收，绝不登记到真实节点。
export const demoPeers = {
  version: 12,
  addresses: rows.flatMap((_, i) => [
    `203.0.113.${i + 1}`,
    `2001:db8:1234:5678:abcd:ef01:2345:${i + 1}`,
  ]),
};
for (const [i, entry] of demoNodes.entries()) {
  const n = entry.node;
  n.public_addresses = demoPeers.addresses.slice(i * 2, i * 2 + 2);
  n.address_version = i === 6 ? 10 : 12;
  n.last_seen = 1788924600;
  n.ssh_port = n.id === "C071" ? 65522 : 22;
  const inventory = n.inventory;
  inventory.collected_at = 1788924600;
  inventory.firewall.data.conflict =
    i === 7 ? "维护来源发生外部变化（演示场景）" : null;
  inventory.firewall.data.policy.adopted = i !== 0;
  inventory.firewall.data.policy.peers = demoPeers;
  inventory.firewall.data.policy.rules = Array.from({ length: 60 }, (_, r) => ({
    id: `rule-${r + 1}`,
    family: r % 5 === 0 ? 6 : 4,
    direction: r % 7 === 0 ? "forward" : "input",
    protocol: r % 3 === 0 ? "udp" : "tcp",
    source:
      r % 5 === 0
        ? "2001:db8:1234:5678:abcd:ef01:2345:6789/128"
        : r % 4 === 0
          ? "192.0.2.0/24"
          : null,
    destination: null,
    ports: [r < 3 ? [22, 80, 443][r] : 8000 + r],
    action: r % 6 === 0 ? "drop" : "accept",
    enabled: r % 11 !== 0,
  }));
  inventory.firewall.data.external_rules = {
    "原始规则（演示）": "外部规则尚未结构化，仅提供原文查看，不代表空规则集。",
  };
  inventory.stacks = inventory.docker.data.containers.map((c: any) => ({
    project: c.project,
    directory: `/opt/stacks/${c.project}`,
    revision: 3,
    protected: c.protected,
    files: ["compose.yml"],
    env_files: [".env"],
  }));
  for (const c of inventory.docker.data.containers) {
    c.image =
      "registry.example.com/infrastructure/production/" +
      c.image +
      "-2026.09.09";
    c.ports = [
      { IP: "0.0.0.0", PublicPort: 8080, PrivatePort: 80, Type: "tcp" },
    ];
    c.mounts = [{ Type: "volume", Name: `${c.id}-data`, Destination: "/data" }];
  }
  inventory.docker.data.images = [
    {
      Id: `sha256:${n.id}`,
      RepoTags: ["registry.example.com/ops/service:stable"],
      Size: 146800640,
    },
  ];
  inventory.docker.data.networks = [
    { Id: `net-${n.id}`, Name: "bridge", Driver: "bridge" },
  ];
  inventory.docker.data.volumes = [
    {
      Name: `${n.id}-data`,
      Driver: "local",
      Mountpoint: `/var/lib/docker/volumes/${n.id}-data/_data`,
    },
  ];
  if (i === 9) {
    inventory.firewall = {
      state: "error",
      error: "防火墙管理器读取失败（演示场景）",
    };
    inventory.docker = {
      state: "error",
      error: "Docker socket 读取失败（演示场景）",
    };
  }
}
// 演示必须覆盖运行之外的常见状态，否则「已停止」「异常」与可用操作无从检验。
for (const [id, index, state] of [
  ["C052", 1, "exited"],
  ["C071", 1, "dead"],
] as const) {
  const containers =
    demoNodes.find((n) => n.node.id === id)?.node.inventory?.docker?.data
      ?.containers;
  const container = containers?.[index];
  if (container) container.state = state;
}

// 指标状态也要覆盖全部四种：正常、采集失败、数据陈旧、以及旧 Agent 没有能力位。
// C101 模拟尚未升级的 Agent；C081 模拟采集失败；C061 离线，其数据因此陈旧。
const C101 = "C101";
const C081 = "C081";
const C061 = "C061";
// 能力位的五个必填项在老 Agent 上也在（Rust 侧不是 Option），缺的是带 serde(default) 的那几个，
// 序列化后表现为 false 而不是键缺失，所以这里用 metrics/hostinfo 为 false 来表达「未上报」。
for (const entry of demoNodes)
  if (entry.node.id === C101)
    entry.node.capabilities = {
      os: "linux",
      docker: true,
      compose: true,
      systemd: true,
      firewall: ["nftables"],
      metrics: false,
      hostinfo: false,
    };

/** 生成一份可辨认的演示指标记录，并覆盖四种状态。 */
export function demoMetrics(at = Math.floor(Date.now() / 1000)): Record<
  string,
  unknown
> {
  const records: Record<string, unknown> = {};
  for (const entry of demoNodes) {
    const id = entry.node.id;
    if (id === C101) continue; // 没有能力位 → 「未上报指标」
    if (id === C081) {
      records[id] = {
        node_id: id,
        received_at: at,
        clock_offset: 1,
        sample: null,
        error: "读取 /proc/stat 失败（演示场景）",
      };
      continue;
    }
    const seed = Number(id.slice(1)) || 1;
    // C061 离线：时刻停在很久以前，控制台据此显示「数据陈旧」
    const received = id === C061 ? at - 900 : at;
    const core = 4;
    const memoryTotal = 8 * 1024 ** 3;
    records[id] = {
      node_id: id,
      received_at: received,
      clock_offset: id === "C091" ? 95 : 0,
      error: null,
      sample: {
        at: received,
        cpu_usage: 8 + (seed % 40),
        cpu_per_core: Array.from(
          { length: core },
          (_, i) => 5 + ((seed + i * 7) % 45),
        ),
        load1: 0.3 + (seed % 20) / 10,
        load5: 0.2 + (seed % 15) / 10,
        load15: 0.1 + (seed % 10) / 10,
        memory_total: memoryTotal,
        memory_used: memoryTotal * (0.25 + (seed % 50) / 200),
        memory_available: memoryTotal * 0.5,
        swap_total: 2 * 1024 ** 3,
        swap_used: (seed % 5) * 1024 ** 2,
        disks: [
          {
            mount: "/",
            filesystem: "/dev/sda1",
            total: 100 * 1024 ** 3,
            used: (20 + (seed % 60)) * 1024 ** 3,
            inode_total: 6_553_600,
            inode_used: 100_000 + seed * 1000,
          },
          {
            mount: "/var/lib/docker",
            filesystem: "/dev/sdb1",
            total: 200 * 1024 ** 3,
            used: (40 + (seed % 80)) * 1024 ** 3,
            inode_total: 13_107_200,
            inode_used: 300_000 + seed * 2000,
          },
        ],
        disk_read_bytes_per_second: (seed % 30) * 1024 ** 2,
        disk_write_bytes_per_second: (seed % 20) * 1024 ** 2,
        interfaces: [
          {
            name: "eth0",
            rx_bytes_per_second: (1 + (seed % 50)) * 1024 ** 2,
            tx_bytes_per_second: (1 + (seed % 30)) * 1024 ** 2,
            rx_errors: 0,
            tx_errors: 0,
          },
          {
            name: "easytier0",
            rx_bytes_per_second: (seed % 10) * 1024 ** 2,
            tx_bytes_per_second: (seed % 8) * 1024 ** 2,
            rx_errors: 0,
            tx_errors: seed % 3,
          },
        ],
        uptime: 3600 * (24 * (seed % 200) + (seed % 24)),
        processes: 90 + seed * 7,
        tcp_connections: 30 + seed * 3,
        udp_connections: 5 + (seed % 12),
        // 线路质量：四种情况都要出现——正常、偏慢、不可达、以及"没测到"。
        // 没测到的对端也不会被补成 0，而是带着原因留在这里。
        peers: [
          {
            node_id: "C001",
            address: "100.100.201.1",
            method: "icmp",
            latency_ms: 0.4 + (seed % 5) * 0.3,
            reachable: true,
            error: null,
            probed_at: received - (seed % 30),
          },
          {
            node_id: "C041",
            address: "100.100.201.41",
            method: "icmp",
            latency_ms: 182.6,
            reachable: true,
            error: null,
            probed_at: received - (seed % 30),
          },
          {
            node_id: "C071",
            address: "100.100.201.71",
            method: "icmp",
            latency_ms: null,
            reachable: false,
            error: "ICMP 无回应（100% 丢包）",
            probed_at: received - (seed % 30),
          },
          {
            node_id: "C091",
            address: "100.100.201.91",
            method: "icmp",
            latency_ms: null,
            reachable: false,
            error: "本机没有 ping 命令",
            probed_at: received - (seed % 30),
          },
        ],
      },
    };
  }
  return records;
}

export const demoTasks: Task[] = rows.map(([id], i) => ({
  id: `demo-task-${i + 1}`,
  node_id: id,
  key: `demo-key-${i + 1}`,
  action: {
    type: "peer_sync",
    set: { version: 1, addresses: ["100.100.201.1", "100.100.201.2"] },
  },
  digest: `demo-digest-${i + 1}`,
  status: i === 6 ? "blocked" : i === 7 ? "failed" : "succeeded",
  result: null,
  error:
    i === 6 ? "节点离线，等待同步" : i === 7 ? "外部规则冲突，尚未发布" : null,
  created_at: 1788924500,
  updated_at: 1788924600,
}));
