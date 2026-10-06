use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

pub const VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentInstallMode {
    Host,
    Docker,
}
impl Default for AgentInstallMode {
    fn default() -> Self {
        Self::Host
    }
}
pub fn digest(bytes: impl AsRef<[u8]>) -> String {
    hex::encode(Sha256::digest(bytes))
}
pub fn now() -> i64 {
    chrono::Utc::now().timestamp()
}
pub fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeCapabilities {
    pub os: String,
    pub docker: bool,
    pub compose: bool,
    pub systemd: bool,
    pub firewall: Vec<String>,
    /// 是否具备指标采样能力。旧 Agent 无此字段时按 false 处理，
    /// 控制台据此显示「未上报指标」而不是 0。
    #[serde(default)]
    pub metrics: bool,
    /// 是否具备主机与块设备盘点能力。
    #[serde(default)]
    pub hostinfo: bool,
    /// 是否具备存储集群准备与部署能力。
    #[serde(default)]
    pub storage: bool,
    /// 是否具备数据库只读巡检能力。能否真正采集到数据还取决于本机是否
    /// 配置了只读巡检账号——那是另一个事实，由巡检报告本身说明。
    #[serde(default)]
    pub database: bool,
    /// 是否具备对端延迟探测能力。缺少 `ping` 的节点仍然可以走 TCP 方式，
    /// 因此这里只是"平台具备能力"，真正能不能测到由探测结果自己说明。
    #[serde(default)]
    pub probe: bool,
    #[serde(default)]
    pub execution_mode: AgentInstallMode,
}

/// 单个挂载点的容量与 inode 用量。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DiskUsage {
    pub mount: String,
    pub filesystem: String,
    pub total: u64,
    pub used: u64,
    pub inode_total: u64,
    pub inode_used: u64,
}

/// 一个被探测的对端。
///
/// 由主控下发（它是唯一知道全部节点地址的一方），并且在目录变化时带版本号重发，
/// 因此 Agent 不需要自己发现邻居，也不会猜地址。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProbeTarget {
    pub node_id: String,
    /// 被探测地址。优先用覆盖网地址：它是节点之间真实的转发路径。
    pub address: String,
    /// 给出的端口时走 TCP 握手计时；为空时走 ICMP。
    /// ICMP 不产生任何服务端日志，是默认方式。
    #[serde(default)]
    pub port: Option<u16>,
}

/// 带版本号的对端探测目标集合，与 `PeerAddressSet` 一样走任务机制下发。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProbeTargetSet {
    pub version: i64,
    #[serde(default)]
    pub targets: Vec<ProbeTarget>,
}

/// 对某个对端的一次探测结果。
///
/// **`latency_ms` 为 `None` 表示没有测到，绝不补 0**：0.0 毫秒是"同机"，
/// 与"探测失败"是完全不同的两件事。失败原因单独放在 `error` 里。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PeerLatency {
    pub node_id: String,
    pub address: String,
    /// 探测方式：`icmp` 或 `tcp`。
    pub method: String,
    #[serde(default)]
    pub latency_ms: Option<f64>,
    /// 是否收到回应。与 `latency_ms` 一样是"事实"，不是判断。
    pub reachable: bool,
    /// 不可达或工具缺失时的人话原因。
    #[serde(default)]
    pub error: Option<String>,
    /// 这次测量发生的时刻。它可能早于所在的采样时刻：探测比采样稀疏，
    /// 界面与曲线据此知道"这个数字是什么时候测的"。
    #[serde(default)]
    pub probed_at: i64,
}

/// 单个网络接口的速率与累计错误。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InterfaceRate {
    pub name: String,
    pub rx_bytes_per_second: f64,
    pub tx_bytes_per_second: f64,
    pub rx_errors: u64,
    pub tx_errors: u64,
}

/// 一次主机指标采样。数值单位在字段名中写明，避免展示层猜单位。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MetricsSample {
    /// 采样时刻，由 Agent 本地时钟给出；主控据此估算时钟偏移。
    pub at: i64,
    pub cpu_usage: f64,
    pub cpu_per_core: Vec<f64>,
    pub load1: f64,
    pub load5: f64,
    pub load15: f64,
    pub memory_total: u64,
    pub memory_used: u64,
    pub memory_available: u64,
    pub swap_total: u64,
    pub swap_used: u64,
    pub disks: Vec<DiskUsage>,
    pub disk_read_bytes_per_second: f64,
    pub disk_write_bytes_per_second: f64,
    pub interfaces: Vec<InterfaceRate>,
    pub uptime: u64,
    pub processes: u64,
    pub tcp_connections: u64,
    pub udp_connections: u64,
    /// 到各个对端的延迟。探测频率低于采样频率，因此这个列表在多数采样里是空的
    /// （沿用上一次的结果），只有真正探测过的那一次带上新值。
    #[serde(default)]
    pub peers: Vec<PeerLatency>,
}

/// 指标上报。`sample` 与 `error` 互斥：
/// 采集失败时 `sample` 为空且 `error` 有值，主控必须显示「采集失败」而不是 0。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsReport {
    pub at: i64,
    #[serde(default)]
    pub sample: Option<MetricsSample>,
    #[serde(default)]
    pub error: Option<String>,
}

/// 主控侧保存的最近一次指标记录，含时钟偏移估计。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsRecord {
    pub node_id: String,
    /// 主控收到该帧的时刻。
    pub received_at: i64,
    /// 估算的时钟偏移（秒）：正值表示 Agent 时钟落后于主控。
    /// 它包含单向网络时延，因此只用于识别明显偏差，不作为精确校时。
    pub clock_offset: i64,
    #[serde(default)]
    pub sample: Option<MetricsSample>,
    #[serde(default)]
    pub error: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: String,
    pub name: String,
    pub public_addresses: Vec<String>,
    pub overlay_address: Option<String>,
    pub ssh_port: u16,
    pub revoked: bool,
    pub last_seen: i64,
    pub capabilities: Option<NodeCapabilities>,
    pub inventory: Option<Value>,
    pub address_version: i64,
    /// 以下字段只用于分享页展示，均为可选，且**不属于**敏感信息。
    /// 全部带 `default`，因此旧控制库与旧 Agent 都不受影响。
    #[serde(default)]
    pub region: Option<String>,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    /// 分享页排序权重，越大越靠前。
    #[serde(default)]
    pub weight: i64,
    /// 对未持令牌的访客隐藏该节点。
    #[serde(default)]
    pub hidden: bool,
    /// 面向公开展示的备注，绝不能放内部信息。
    #[serde(default)]
    pub public_remark: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerAddressSet {
    pub version: i64,
    pub addresses: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    Accepted,
    Running,
    Validating,
    Succeeded,
    Failed,
    Uncertain,
    RollbackPending,
    RolledBack,
    Blocked,
    Cancelled,
}
impl TaskStatus {
    pub fn terminal(&self) -> bool {
        matches!(
            self,
            Self::Succeeded
                | Self::Failed
                | Self::Uncertain
                | Self::RolledBack
                | Self::Blocked
                | Self::Cancelled
        )
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    Inspect {},
    /// 主机与块设备盘点。只读，是存储预检的唯一数据来源。
    HostInspect {},
    /// 数据库只读巡检：Galera / 主机侧 / ProxySQL / 备份链路。
    ///
    /// **不带任何参数**：语句固定在 Agent 里，因此没有 SQL 会经过网络，
    /// 也就不存在把只读通道变成任意查询通道的可能。
    DbInspect {},
    DockerInspect {
        container: String,
    },
    Docker {
        container: String,
        operation: DockerOperation,
    },
    PullImage {
        reference: String,
    },
    StackImport {
        project: String,
        directory: String,
        files: Vec<String>,
        env_files: Vec<String>,
    },
    StackCreate {
        project: String,
        content: String,
    },
    StackPlan {
        project: String,
        content: String,
    },
    StackApply {
        plan_id: String,
    },
    StackControl {
        project: String,
        operation: StackOperation,
    },
    FirewallPlan {
        operation: Value,
    },
    FirewallApply {
        plan_id: String,
        witness: Option<String>,
    },
    PeerSync {
        set: PeerAddressSet,
    },
    /// 下发对端探测目标列表。与 `PeerSync` 一样带版本号：
    /// 目录没变就不会重发，Agent 也就不需要自己发现邻居或猜地址。
    PeerProbeTargets {
        set: ProbeTargetSet,
    },
    /// 以下为结构化文件变更。它们走任务机制而不是流通道：
    /// 因此天然带幂等键、事件与审计，并且可以参与节点变更锁。
    FilePut {
        path: String,
        /// 内容摘要。只有暂存内容与它一致才会覆盖目标文件。
        digest: String,
        size: u64,
    },
    FileRemove {
        path: String,
        #[serde(default)]
        recursive: bool,
        /// 递归删除必须显式确认为 true，缺省即为拒绝。
        #[serde(default)]
        confirmed: bool,
    },
    FileRename {
        from: String,
        to: String,
    },
    FileMkdir {
        path: String,
    },
    FileChmod {
        path: String,
        mode: u32,
    },
    FileChown {
        path: String,
        uid: u32,
        gid: u32,
    },
    /// 存储集群的磁盘准备。**这是全系统唯一会销毁数据的操作**：
    /// 会清空 `devices` 中列出的每一块盘。
    StoragePrepare {
        cluster: String,
        /// 计划编号，仅用于把这次执行与计划、审计串起来。
        plan_id: String,
        devices: Vec<StorageDeviceTarget>,
        /// 文件系统类型，目前只接受 xfs。
        filesystem: String,
    },
    /// 部署集群容器。只写 opsd 自己的 Compose 文件，不碰其他项目。
    StorageDeploy {
        cluster: String,
        plan_id: String,
        /// 固定 release 标签；不接受 latest。
        image: String,
        /// 本节点参与集群的挂载点。
        mounts: Vec<String>,
        /// 集群全部节点地址（含本机），用于生成分布式拓扑。
        peers: Vec<String>,
        /// 访问凭据的键名，凭据本身由 Agent 本地保存，不经过网络。
        access_key: String,
        secret_key: String,
    },
    /// 读取集群健康与容量。
    StorageStatus {
        cluster: String,
    },
    /// 创建或更新桶。
    StorageBucket {
        cluster: String,
        bucket: String,
        #[serde(default)]
        remove: bool,
    },
    /// 创建访问用户并绑定策略。
    StorageUser {
        cluster: String,
        user: String,
        secret: String,
        #[serde(default = "readwrite")]
        policy: String,
        #[serde(default)]
        remove: bool,
    },
}
fn readwrite() -> String {
    "readwrite".into()
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DockerOperation {
    Start,
    Stop,
    Restart,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StackOperation {
    Start,
    Stop,
    Remove,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskEnvelope {
    pub id: String,
    pub node_id: String,
    pub key: String,
    pub action: Action,
    pub digest: String,
    pub status: TaskStatus,
    #[serde(default)]
    pub sequence: i64,
    pub result: Option<Value>,
    pub error: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}
impl TaskEnvelope {
    pub fn redacted(&self) -> Self {
        let mut task = self.clone();
        match &mut task.action {
            Action::StackCreate { content, .. } | Action::StackPlan { content, .. } => {
                *content = "[配置正文已隐藏]".into();
            }
            _ => {}
        }
        task
    }
    pub fn new(node_id: String, key: String, action: Action) -> Self {
        let digest = digest(serde_json::to_vec(&action).unwrap());
        Self {
            id: id(),
            node_id,
            key,
            action,
            digest,
            status: TaskStatus::Pending,
            sequence: 0,
            result: None,
            error: None,
            created_at: now(),
            updated_at: now(),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskEvent {
    pub id: String,
    pub task_id: String,
    pub sequence: i64,
    pub time: i64,
    pub status: TaskStatus,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Frame {
    Hello {
        version: u32,
        node_id: String,
        capabilities: NodeCapabilities,
    },
    Task {
        task: TaskEnvelope,
    },
    Report {
        task: TaskEnvelope,
    },
    Inventory {
        value: Value,
    },
    /// Agent 主动上报的主机指标；不经过任务机制，属于周期性遥测。
    Metrics {
        report: MetricsReport,
    },
    Heartbeat,
    StreamOpen {
        stream_id: String,
        kind: StreamKind,
        target: StreamTarget,
        /// 仅容器终端使用：在容器内执行的命令。
        #[serde(default)]
        command: Vec<String>,
    },
    StreamData {
        stream_id: String,
        data: String,
    },
    StreamClose {
        stream_id: String,
        error: Option<String>,
    },
    Probe {
        request_id: String,
        address: String,
        port: u16,
    },
    ProbeResult {
        request_id: String,
        ok: bool,
    },
}

/// 目录条目。只暴露展示与定位所需的字段，不含属主之外的任何标识。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirEntry {
    pub name: String,
    /// `file` / `dir` / `symlink`。符号链接单列，不伪装成目录。
    pub kind: String,
    pub size: u64,
    pub modified: i64,
    pub readonly: bool,
}

/// 单个路径的元信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileStat {
    pub name: String,
    pub path: String,
    pub kind: String,
    pub size: u64,
    pub modified: i64,
    pub readonly: bool,
}

/// 流会话的类型。宿主机终端与文件传输共用同一套流通道。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamKind {
    /// 容器内终端或日志。
    ContainerTerminal,
    /// 宿主机受控终端。
    HostShell,
    /// 列出目录。只读，因此走流式请求-响应而不是任务表。
    FileList,
    /// 读取文件（含下载）。
    FileRead,
    /// 写入文件（上传分块）。
    FileWrite,
}

/// 流会话的目标。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "target", rename_all = "snake_case")]
pub enum StreamTarget {
    Container {
        id: String,
    },
    Host {
        /// 固定使用的 shell，不接受调用方指定任意程序。
        #[serde(default = "default_shell")]
        shell: String,
        /// 会话起始目录。
        #[serde(default)]
        workdir: Option<String>,
    },
    Dir {
        path: String,
    },
    File {
        path: String,
        #[serde(default)]
        offset: u64,
        #[serde(default)]
        limit: u64,
    },
}
fn default_shell() -> String {
    "/bin/bash".into()
}

/// 块设备。字段来自 `lsblk -J -O`，保留其原始语义以便与运维直觉一致。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BlockDevice {
    pub name: String,
    pub path: String,
    /// `disk` / `part` / `lvm` / `raid` / `crypt` / `rom` 等，取自 lsblk 的 TYPE。
    pub kind: String,
    pub size: u64,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub serial: Option<String>,
    /// 是否为旋转介质；true 表示机械盘。
    pub rotational: bool,
    #[serde(default)]
    pub transport: Option<String>,
    #[serde(default)]
    pub mountpoints: Vec<String>,
    #[serde(default)]
    pub filesystem: Option<String>,
    #[serde(default)]
    pub uuid: Option<String>,
    /// 直接子设备名，用于把分区归到整盘上。
    #[serde(default)]
    pub children: Vec<String>,
}

/// 逐盘的风险标记。每条都为 true 时必须给出具体原因。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DriveRisk {
    pub device: String,
    /// 承载 `/` 或 `/boot`，绝不能拿来格式化。
    pub system_disk: bool,
    /// 已挂载**且**有文件系统，确定存有数据。
    pub has_data: bool,
    pub mounted: bool,
    /// 有文件系统但未挂载。**很可能仍有数据**，无法在不挂载的情况下确认，
    /// 因此同样不能自动放行。
    pub unmounted_filesystem: bool,
    /// 存在分区表。可能只是残留，但也是"有人用过"的证据。
    pub has_partitions: bool,
    pub lvm_member: bool,
    pub raid_member: bool,
    pub luks: bool,
    pub removable: bool,
    /// 容量为 0，通常是空读卡器或未就绪设备。
    pub zero_size: bool,
    /// 人类可读的具体原因，界面直接展示。
    pub reasons: Vec<String>,
}

impl DriveRisk {
    /// 是否可以安全地交给存储集群使用。
    ///
    /// 判定标准是**「没有任何先前使用的痕迹」**：只要挂着文件系统、有分区表、
    /// 是系统盘，或者属于 LVM/RAID/加密卷，都不能自动放行——格式化会毁掉上面的东西。
    /// 全新加入的空盘自然满足这个条件；用过但已 `wipefs` 清理的盘同样满足。
    ///
    /// 这个布尔量是分析逻辑的唯一依据，因此它必须与 `reasons` 完全一致，
    /// 否则界面会同时显示「可用于存储集群」和「容量为 0」这种自相矛盾的信息。
    pub fn usable(&self) -> bool {
        !self.system_disk
            && !self.has_data
            && !self.mounted
            && !self.unmounted_filesystem
            && !self.has_partitions
            && !self.lvm_member
            && !self.raid_member
            && !self.luks
            && !self.removable
            && !self.zero_size
    }
}

/// 物理网卡与链路速率。用于判断节点之间是否具备可用的存储带宽。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HostInterface {
    pub name: String,
    #[serde(default)]
    pub speed_mbps: Option<u64>,
    pub address: Option<String>,
}

/// 节点上已有的对象存储痕迹。发现既有部署时不应再次部署。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExistingStorage {
    #[serde(default)]
    pub containers: Vec<String>,
    #[serde(default)]
    pub systemd_units: Vec<String>,
    #[serde(default)]
    pub data_directories: Vec<String>,
}

impl ExistingStorage {
    pub fn found(&self) -> bool {
        !self.containers.is_empty()
            || !self.systemd_units.is_empty()
            || !self.data_directories.is_empty()
    }
}

/// 主机盘点结果。它是存储预检的唯一输入，因此必须如实反映采集到什么、没采集到什么。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostInventory {
    pub collected_at: i64,
    pub hostname: String,
    pub distro: String,
    pub kernel: String,
    pub arch: String,
    pub cpu_model: String,
    pub cpu_cores: u64,
    pub memory_total: u64,
    pub uptime: u64,
    #[serde(default)]
    pub devices: Vec<BlockDevice>,
    #[serde(default)]
    pub risks: Vec<DriveRisk>,
    #[serde(default)]
    pub interfaces: Vec<HostInterface>,
    #[serde(default)]
    pub existing: ExistingStorage,
    /// SMART 健康摘要。`None` 表示**未采集**，绝不能当作健康。
    #[serde(default)]
    pub smart: Option<String>,
    /// 采集过程中失败的部分。非空时预检必须把它当作信息不全，而不是"没问题"。
    #[serde(default)]
    pub gaps: Vec<String>,
}

/// 存储集群中单块磁盘的目标。
///
/// `expected_size` 是计划生成时记录的大小。应用前 Agent 会**重新读取实际大小**并比对，
/// 防止计划与执行之间设备被替换或改动——这是格式化前最后一道校验。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageDeviceTarget {
    pub path: String,
    pub expected_size: u64,
    /// 挂载点，例如 `/data/disk1`。
    pub mount: String,
}

/// 集群健康与容量。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StorageHealth {
    /// `online` / `degraded` / `offline` / `unknown`
    pub state: String,
    pub raw_capacity: u64,
    pub usable_capacity: u64,
    pub used_capacity: u64,
    pub drives_total: usize,
    pub drives_offline: usize,
    #[serde(default)]
    pub buckets: usize,
    /// 采集不到时的说明。此时上面各值都不可信。
    #[serde(default)]
    pub error: Option<String>,
}

/// 只读巡检结果的一个分区。
///
/// **「未知」是一等状态**：连不上、没权限、没配置都归到这里，并带上原因。
/// 它绝不能被折叠成 0（"队列长度 0"）或健康（"没有告警"）——
/// 运维视图里最危险的假象就是"采集失败看起来很平静"。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbSection<T> {
    #[serde(default)]
    pub data: Option<T>,
    /// 采集失败或未配置时的具体原因，界面直接展示。
    #[serde(default)]
    pub reason: Option<String>,
}

impl<T> DbSection<T> {
    pub fn unknown(reason: impl Into<String>) -> Self {
        Self {
            data: None,
            reason: Some(reason.into()),
        }
    }
    pub fn ok(data: T) -> Self {
        Self {
            data: Some(data),
            reason: None,
        }
    }
    /// 是否采集到了数据。界面据此决定显示数值还是显示「未知」。
    pub fn known(&self) -> bool {
        self.data.is_some()
    }
}

/// Galera / wsrep 状态。字段名与 `information_schema.GLOBAL_STATUS` 对齐，便于核对。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GaleraState {
    /// `Primary` / `non-Primary` / `Disconnected`。
    pub cluster_status: String,
    /// 本节点看到的集群成员数。
    #[serde(default)]
    pub cluster_size: Option<u64>,
    /// 期望的成员数，来自节点本机巡检配置（默认 5，与告警规则一致）。
    ///
    /// 由 Agent 原样带上来而不是就地判断：判断集中在控制台完成，
    /// 因此"为什么说它不正常"可以在界面上被复核。
    #[serde(default)]
    pub expected_cluster_size: u64,
    /// wsrep 本地状态码，4 = Synced。
    #[serde(default)]
    pub local_state: Option<u64>,
    /// `Synced` / `Donor/Desynced` / `Joining` 等。
    pub local_state_comment: String,
    #[serde(default)]
    pub ready: Option<bool>,
    #[serde(default)]
    pub connected: Option<bool>,
    #[serde(default)]
    pub desync: Option<bool>,
    /// 流控暂停比例（0..1）。持续大于 0.01 即视为异常。
    #[serde(default)]
    pub flow_control_paused: Option<f64>,
    #[serde(default)]
    pub flow_control_paused_ns: Option<u64>,
    #[serde(default)]
    pub last_committed: Option<i64>,
    #[serde(default)]
    pub recv_queue: Option<u64>,
    #[serde(default)]
    pub send_queue: Option<u64>,
    #[serde(default)]
    pub cert_failures: Option<u64>,
    #[serde(default)]
    pub bf_aborts: Option<u64>,
    #[serde(default)]
    pub cluster_state_uuid: Option<String>,
    /// 本节点 UUID。
    #[serde(default)]
    pub node_uuid: Option<String>,
    #[serde(default)]
    pub sst_donor: Option<String>,
    #[serde(default)]
    pub provider_version: Option<String>,
    /// `wsrep_incoming_addresses` 拆开后的成员地址。
    #[serde(default)]
    pub incoming: Vec<String>,
}

/// 数据库主机侧的事实。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DbHostState {
    pub version: String,
    #[serde(default)]
    pub version_comment: String,
    #[serde(default)]
    pub hostname: Option<String>,
    #[serde(default)]
    pub server_id: Option<u64>,
    #[serde(default)]
    pub uptime: Option<u64>,
    #[serde(default)]
    pub threads_connected: Option<u64>,
    #[serde(default)]
    pub threads_running: Option<u64>,
    #[serde(default)]
    pub read_only: Option<bool>,
    /// binlog 位点。未开启 binlog 时为 None，这也是"没有"，不是错误。
    #[serde(default)]
    pub binlog_file: Option<String>,
    #[serde(default)]
    pub binlog_position: Option<u64>,
}

/// ProxySQL 的一个后端节点。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProxySqlServer {
    pub hostgroup_id: i64,
    pub hostname: String,
    pub port: u16,
    /// `ONLINE` / `SHUNNED` / `OFFLINE_SOFT` 等。
    pub status: String,
    pub weight: u64,
    #[serde(default)]
    pub max_connections: u64,
    /// 来自 `stats_mysql_connection_pool.Latency_us`；没有该行时为 None，不补 0。
    #[serde(default)]
    pub latency_us: Option<u64>,
    #[serde(default)]
    pub queries: Option<u64>,
    #[serde(default)]
    pub conn_used: Option<u64>,
    #[serde(default)]
    pub conn_free: Option<u64>,
}

/// ProxySQL 连接池计数。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProxySqlPool {
    pub hostgroup: i64,
    pub srv_host: String,
    pub srv_port: u16,
    #[serde(default)]
    pub conn_used: Option<u64>,
    #[serde(default)]
    pub conn_free: Option<u64>,
    #[serde(default)]
    pub conn_ok: Option<u64>,
    #[serde(default)]
    pub conn_err: Option<u64>,
    #[serde(default)]
    pub queries: Option<u64>,
    #[serde(default)]
    pub bytes_sent: Option<u64>,
    #[serde(default)]
    pub bytes_recv: Option<u64>,
    #[serde(default)]
    pub latency_us: Option<u64>,
}

/// 一条查询摘要统计。
///
/// **只带摘要哈希与计数，绝不带 `digest_text`**：那里面是真实 SQL，
/// 可能包含用户名、邮箱、令牌等值。运维视图只需要知道"哪类语句最重"。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProxySqlDigest {
    /// 摘要哈希，可作为跨节点比对的稳定标识。
    pub digest: String,
    pub hostgroup: i64,
    #[serde(default)]
    pub schemaname: String,
    pub count_star: u64,
    pub sum_time: u64,
    pub max_time: u64,
}

/// ProxySQL 只读视图。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProxySqlState {
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub uptime: Option<u64>,
    /// writer 组（hostgroup 10）在线的后端数。
    #[serde(default)]
    pub writer_online: Option<u64>,
    /// 备用 writer 组（hostgroup 20）在线的后端数。
    #[serde(default)]
    pub backup_writer_online: Option<u64>,
    #[serde(default)]
    pub servers: Vec<ProxySqlServer>,
    #[serde(default)]
    pub pools: Vec<ProxySqlPool>,
    /// 按总耗时排序的前若干条摘要，条数有上限。
    #[serde(default)]
    pub digests: Vec<ProxySqlDigest>,
}

/// 单个备份层级的现状。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BackupTier {
    /// `hourly` / `daily` / `weekly`。
    pub tier: String,
    /// 最近一次成功的时刻（取自备份脚本写的 success 标记文件）。
    #[serde(default)]
    pub last_success: Option<i64>,
    /// 最近一次成功距今多少秒。界面据此判断"新鲜度"。
    #[serde(default)]
    pub age_seconds: Option<i64>,
    /// 最近一次成功的那份备份的体积。
    #[serde(default)]
    pub size_bytes: Option<u64>,
    pub directory: String,
    /// 该目录里归档文件的个数与校验清单是否存在。
    #[serde(default)]
    pub files: usize,
    #[serde(default)]
    pub has_checksums: bool,
    /// 归档是否为 age 格式。**只检查文件头**，见下面的说明。
    #[serde(default)]
    pub age_header_ok: Option<bool>,
}

/// 备份链路现状。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BackupState {
    pub root: String,
    #[serde(default)]
    pub tiers: Vec<BackupTier>,
    /// systemd 定时器的下次触发时刻。没有定时器时为 None。
    #[serde(default)]
    pub next_run: Option<i64>,
    #[serde(default)]
    pub last_trigger: Option<i64>,
}

/// 一次只读巡检的完整结果。
///
/// 四个分区**各自独立**：ProxySQL 连不上时，Galera 部分仍然有效。
/// 这种"部分未知"比整体失败更有用，也正是运维视图需要如实表达的状态。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbInspectReport {
    pub collected_at: i64,
    /// Agent 本地是否配置了只读巡检账号。未配置时其余分区都会给出原因。
    pub configured: bool,
    pub galera: DbSection<GaleraState>,
    pub host: DbSection<DbHostState>,
    pub proxysql: DbSection<ProxySqlState>,
    pub backup: DbSection<BackupState>,
    /// 巡检过程中被跳过的部分与原因，便于一次性看到全部问题。
    #[serde(default)]
    pub gaps: Vec<String>,
}

pub fn validate_public_address(value: &str) -> anyhow::Result<std::net::IpAddr> {
    use std::net::IpAddr;
    let ip: IpAddr = value.parse()?;
    let valid = match ip {
        IpAddr::V4(v) => {
            let b = v.octets();
            !v.is_private()
                && !v.is_loopback()
                && !v.is_link_local()
                && !v.is_multicast()
                && !v.is_broadcast()
                && b[0] != 0
                && b[0] < 240
                && !(b[0] == 100 && (64..=127).contains(&b[1]))
                && !v.is_documentation()
                && !(b[0] == 198 && (b[1] == 18 || b[1] == 19))
        }
        IpAddr::V6(v) => {
            let s = v.segments();
            (s[0] & 0xe000) == 0x2000 && !(s[0] == 0x2001 && s[1] == 0x0db8)
        }
    };
    anyhow::ensure!(valid, "地址必须是可路由的公网地址");
    Ok(ip)
}
