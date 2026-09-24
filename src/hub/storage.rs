//! 存储预检分析。
//!
//! 预检**不在节点上做判断**：Agent 只带回事实，判断集中在这里完成。
//! 因此本模块是纯函数——输入各节点的主机盘点，输出结论式报告。这样分析逻辑
//! 可以被穷举测试，也不受节点离线、采集失败等时序因素干扰。
//!
//! 三条硬规则：
//!
//! 1. **不猜**。未采集到盘位的节点标为「待采集」，绝不判为「不适合」。
//! 2. **每个结论都要有原因**。界面直接展示原因，而不是只给一个分数。
//! 3. **不把信息不全当没问题**。采集缺口（`gaps`）会让结论降级为「待处理」。
use opsd::protocol::{Action, HostInventory, Node, TaskEnvelope, TaskStatus};
use serde::{Deserialize, Serialize};

/// 分布式对象存储要求的最小节点数。
pub const MIN_NODES: usize = 4;
/// 每个节点上期望的磁盘数量下限。
pub const MIN_DRIVES_PER_NODE: usize = 1;
/// 候选磁盘容量差异超过该比例即提示不一致。
pub const SIZE_TOLERANCE: f64 = 0.25;
/// 低于该链路速率时提示带宽风险（Mbps）。
pub const MIN_LINK_MBPS: u64 = 100;

/// 结论等级。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Suitability {
    /// 可以直接进入部署计划。
    Suitable,
    /// 有条件可用，需要先处理列出的问题。
    NeedsWork,
    /// 存在硬性阻断，不能部署。
    Unsuitable,
    /// 尚未采集到足够信息，**不代表不适合**。
    Pending,
}

/// 单块候选磁盘。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candidate {
    pub device: String,
    pub path: String,
    pub size: u64,
    pub rotational: bool,
    pub model: Option<String>,
}

/// 单个节点的结论。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeVerdict {
    pub node_id: String,
    pub name: String,
    pub online: bool,
    pub suitability: Suitability,
    /// 结论的原因，逐条给出。界面直接展示。
    pub reasons: Vec<String>,
    pub candidates: Vec<Candidate>,
    /// 被拒绝的盘及原因，便于运维核对。
    pub rejected: Vec<Rejected>,
    pub collected_at: Option<i64>,
    pub gaps: Vec<String>,
    /// 该节点最快的物理链路速率，用于带宽检查。
    pub link_mbps: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rejected {
    pub device: String,
    pub reason: String,
}

/// 一致性检查项。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Check {
    pub name: String,
    pub passed: bool,
    /// 不通过时的具体说明。
    pub detail: String,
}

/// 推荐拓扑。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Topology {
    pub nodes: usize,
    pub drives_per_node: usize,
    pub drive_size: u64,
    /// 原始裸容量。
    pub raw_capacity: u64,
    /// 纠删码校验盘数量。
    pub parity: usize,
    /// 扣除校验后的可用容量。
    pub usable_capacity: u64,
    pub parity_note: String,
}

/// 预检报告。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    pub generated_at: i64,
    pub nodes: Vec<NodeVerdict>,
    pub checks: Vec<Check>,
    pub topology: Option<Topology>,
    /// 总体是否可以直接进入部署计划。
    pub ready: bool,
    /// 面向运维的总结说明。
    pub summary: String,
}

/// 分析一次预检。
///
/// `inventories` 以节点编号为键：**没有条目就是没采集到**，与"采集到空列表"是两回事。
pub fn analyze(
    nodes: &[Node],
    online: &std::collections::HashSet<String>,
    inventories: &std::collections::HashMap<String, HostInventory>,
    at: i64,
) -> Report {
    let mut verdicts = Vec::new();
    for node in nodes.iter().filter(|n| !n.revoked) {
        verdicts.push(verdict_for(
            node,
            online.contains(&node.id),
            inventories.get(&node.id),
        ));
    }

    let ready_nodes: Vec<&NodeVerdict> = verdicts
        .iter()
        .filter(|v| v.suitability == Suitability::Suitable)
        .collect();
    let checks = consistency(&verdicts, &ready_nodes);
    let all_passed = checks.iter().all(|c| c.passed);
    let topology = all_passed
        .then(|| topology(&ready_nodes))
        .flatten();

    let pending = verdicts
        .iter()
        .filter(|v| v.suitability == Suitability::Pending)
        .count();
    let unsuitable = verdicts
        .iter()
        .filter(|v| v.suitability == Suitability::Unsuitable)
        .count();
    let needs_work = verdicts
        .iter()
        .filter(|v| v.suitability == Suitability::NeedsWork)
        .count();

    let ready = all_passed && topology.is_some();
    let summary = if ready {
        format!(
            "{} 个节点具备部署条件，推荐 {} 节点 × {} 块盘。",
            ready_nodes.len(),
            topology.as_ref().map(|t| t.nodes).unwrap_or(0),
            topology.as_ref().map(|t| t.drives_per_node).unwrap_or(0),
        )
    } else if pending > 0 {
        format!(
            "还有 {pending} 个节点尚未采集到盘位信息，暂不能给出结论；先触发一次主机盘点。"
        )
    } else {
        format!(
            "当前不满足部署条件：{unsuitable} 个节点存在硬性阻断，{needs_work} 个节点需要先处理。"
        )
    };

    Report {
        generated_at: at,
        nodes: verdicts,
        checks,
        topology,
        ready,
        summary,
    }
}

fn verdict_for(
    node: &Node,
    online: bool,
    inventory: Option<&HostInventory>,
) -> NodeVerdict {
    let base = NodeVerdict {
        node_id: node.id.clone(),
        name: node.name.clone(),
        online,
        suitability: Suitability::Pending,
        reasons: Vec::new(),
        candidates: Vec::new(),
        rejected: Vec::new(),
        collected_at: None,
        gaps: Vec::new(),
        link_mbps: None,
    };
    let Some(inventory) = inventory else {
        // 没有盘点结果：这是"还不知道"，不是"不适合"
        return NodeVerdict {
            reasons: vec![if online {
                "尚未采集：请在节点上触发一次主机盘点".into()
            } else {
                "节点离线，无法采集盘位信息".into()
            }],
            ..base
        };
    };

    let mut reasons = Vec::new();
    let mut candidates = Vec::new();
    let mut rejected = Vec::new();
    for device in inventory.devices.iter().filter(|d| d.kind == "disk") {
        let risk = inventory
            .risks
            .iter()
            .find(|r| r.device == device.name);
        let usable = risk.is_some_and(|r| r.usable());
        if usable && device.size > 0 {
            candidates.push(Candidate {
                device: device.name.clone(),
                path: device.path.clone(),
                size: device.size,
                rotational: device.rotational,
                model: device.model.clone(),
            });
        } else {
            rejected.push(Rejected {
                device: device.name.clone(),
                reason: risk
                    .map(|r| r.reasons.join("；"))
                    .unwrap_or_else(|| "未给出可用性判断".into()),
            });
        }
    }

    if inventory.existing.found() {
        reasons.push(format!(
            "节点上已存在对象存储痕迹（{}），需先确认是否接管而不是重复部署",
            [
                inventory.existing.containers.join("、"),
                inventory.existing.systemd_units.join("、"),
                inventory.existing.data_directories.join("、"),
            ]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join("、")
        ));
    }
    for gap in &inventory.gaps {
        reasons.push(format!("采集缺口：{gap}"));
    }
    if candidates.is_empty() {
        reasons.push("没有找到可用于存储集群的干净磁盘".into());
    } else {
        reasons.push(format!(
            "发现 {} 块可用磁盘，合计 {}",
            candidates.len(),
            human_bytes(candidates.iter().map(|c| c.size).sum())
        ));
    }
    if inventory.smart.is_none() {
        // 明确区分"未采集"与"健康"
        reasons.push("SMART 未采集，磁盘健康状态未知".into());
    }

    // 结论：有硬性阻断（已有存储但没盘、或完全没有盘）→ 不适合；
    // 有采集缺口或 SMART 未采集 → 需要先处理；否则可用。
    let suitability = if candidates.is_empty() {
        Suitability::Unsuitable
    } else if !inventory.gaps.is_empty() || inventory.smart.is_none() {
        Suitability::NeedsWork
    } else {
        Suitability::Suitable
    };

    NodeVerdict {
        suitability,
        reasons,
        candidates,
        rejected,
        collected_at: Some(inventory.collected_at),
        gaps: inventory.gaps.clone(),
        // 取最快的物理链路：只要有一条够快，节点间就有可用带宽
        link_mbps: inventory.interfaces.iter().filter_map(|i| i.speed_mbps).max(),
        ..base
    }
}

/// 一致性检查。全部通过才推荐拓扑。
fn consistency(all: &[NodeVerdict], ready: &[&NodeVerdict]) -> Vec<Check> {
    let mut checks = Vec::new();
    let pending = all
        .iter()
        .filter(|v| v.suitability == Suitability::Pending)
        .count();
    checks.push(Check {
        name: format!("候选节点不少于 {MIN_NODES} 台"),
        passed: ready.len() >= MIN_NODES,
        detail: if ready.len() >= MIN_NODES {
            format!("{} 台已确认可用", ready.len())
        } else if pending > 0 {
            // 未采集的节点不能算作不合格，只能说明结论还不完整
            format!(
                "已确认 {} 台，另有 {pending} 台尚未采集，结论暂不完整",
                ready.len()
            )
        } else {
            format!("仅 {} 台可用，分布式部署至少需要 {MIN_NODES} 台", ready.len())
        },
    });

    // 每节点磁盘数量一致
    let counts: Vec<usize> = ready.iter().map(|v| v.candidates.len()).collect();
    let uniform_count = !counts.is_empty() && counts.windows(2).all(|w| w[0] == w[1]);
    checks.push(Check {
        name: "各节点磁盘数量一致".into(),
        passed: uniform_count,
        detail: if counts.is_empty() {
            "还没有可用节点".into()
        } else if uniform_count {
            format!("每节点 {} 块", counts[0])
        } else {
            format!("各节点数量不一致：{counts:?}；建议先补齐再部署")
        },
    });

    // 容量一致性：纠删码按最小盘计容，差异过大会浪费大容量盘
    let sizes: Vec<u64> = ready
        .iter()
        .flat_map(|v| v.candidates.iter().map(|c| c.size))
        .collect();
    let (min, max) = sizes.iter().fold((u64::MAX, 0u64), |(lo, hi), size| {
        (lo.min(*size), hi.max(*size))
    });
    let size_ok = sizes.is_empty() || (max as f64) <= (min as f64) * (1.0 + SIZE_TOLERANCE);
    checks.push(Check {
        name: "磁盘容量基本一致".into(),
        passed: size_ok,
        detail: if sizes.is_empty() {
            "还没有候选磁盘".into()
        } else if size_ok {
            format!("最小 {}，最大 {}", human_bytes(min), human_bytes(max))
        } else {
            format!(
                "最小 {}，最大 {}，差异超过 {:.0}%；纠删码会按最小盘计容，大盘会被浪费",
                human_bytes(min),
                human_bytes(max),
                SIZE_TOLERANCE * 100.0
            )
        },
    });

    // 介质类型一致：机械与固态混用会让整体性能取决于最慢的一块
    let rotational: Vec<bool> = ready
        .iter()
        .flat_map(|v| v.candidates.iter().map(|c| c.rotational))
        .collect();
    let mixed = rotational.iter().any(|r| *r) && rotational.iter().any(|r| !*r);
    checks.push(Check {
        name: "介质类型一致".into(),
        passed: !mixed,
        detail: if rotational.is_empty() {
            "还没有候选磁盘".into()
        } else if mixed {
            "同时存在机械盘与固态盘，整体性能会受最慢的一块限制".into()
        } else if rotational[0] {
            "全部为机械盘".into()
        } else {
            "全部为固态盘".into()
        },
    });

    // 链路速率：存储集群的吞吐受节点间链路限制
    let slow: Vec<String> = all
        .iter()
        .filter(|v| {
            v.suitability == Suitability::Suitable || v.suitability == Suitability::NeedsWork
        })
        .filter(|v| !v.candidates.is_empty())
        .filter(|v| v.link_mbps.is_some_and(|speed| speed < MIN_LINK_MBPS))
        .map(|v| format!("{}（{} Mbps）", v.name, v.link_mbps.unwrap_or(0)))
        .collect();
    checks.push(Check {
        name: format!("节点链路不低于 {MIN_LINK_MBPS} Mbps"),
        passed: slow.is_empty(),
        detail: if slow.is_empty() {
            "链路速率满足要求，或未采集到速率信息".into()
        } else {
            format!("以下节点链路偏低：{}", slow.join("、"))
        },
    });

    // 采集完整性：任何缺口都意味着结论可能不完整
    let gapped: Vec<String> = all
        .iter()
        .filter(|v| !v.gaps.is_empty())
        .map(|v| format!("{}（{} 项）", v.name, v.gaps.len()))
        .collect();
    checks.push(Check {
        name: "主机信息采集完整".into(),
        passed: gapped.is_empty(),
        detail: if gapped.is_empty() {
            "未发现采集缺口".into()
        } else {
            format!("以下节点存在采集缺口：{}", gapped.join("、"))
        },
    });

    checks
}

/// 依据候选盘推荐拓扑。
///
/// 纠删码按最小盘计容，因此统一取最小容量；校验盘数量沿用 MinIO 的默认取值
/// （4–7 块校验 2，8 块及以上校验 4），并把这条规则写进报告，避免运维猜。
pub fn topology(ready: &[&NodeVerdict]) -> Option<Topology> {
    if ready.len() < MIN_NODES || ready.iter().any(|v| v.candidates.len() < MIN_DRIVES_PER_NODE) {
        return None;
    }
    let drives_per_node = ready.iter().map(|v| v.candidates.len()).min()?;
    let drive_size = ready
        .iter()
        .flat_map(|v| v.candidates.iter().map(|c| c.size))
        .min()?;
    let nodes = ready.len();
    let total_drives = nodes * drives_per_node;
    let raw = drive_size * total_drives as u64;
    let parity = if total_drives >= 8 { 4 } else { 2 };
    let usable = raw / total_drives as u64 * (total_drives - parity) as u64;
    Some(Topology {
        nodes,
        drives_per_node,
        drive_size,
        raw_capacity: raw,
        parity,
        usable_capacity: usable,
        parity_note: format!(
            "共 {total_drives} 块盘，校验 {parity} 块；容量按最小盘 {} 计算",
            human_bytes(drive_size)
        ),
    })
}

/// 人类可读的容量。预检报告面向运维，字节数没有可读性。
pub fn human_bytes(value: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    if value == 0 {
        return "0 B".into();
    }
    let mut size = value as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if size >= 100.0 || unit == 0 {
        format!("{size:.0} {}", UNITS[unit])
    } else {
        format!("{size:.1} {}", UNITS[unit])
    }
}

/* ------------------------------------------------------------------ *
 * 处理函数
 * ------------------------------------------------------------------ */

use super::*;
use axum::{Json, extract::State as S};

/// 各节点最近一次主机盘点结果。盘点结果不大且变化慢，因此直接留在内存里。
pub type SharedInventories = std::sync::Arc<std::sync::RwLock<std::collections::HashMap<String, HostInventory>>>;

/// 存储预检报告。
///
/// 报告是**结论式**的：逐节点给出适合与否以及具体原因，再给一致性检查与推荐拓扑。
/// 未采集的节点标为「待采集」，不会被当作不适合。
pub async fn readiness(S(s): S<State>) -> ApiResult<Json<Report>> {
    let nodes = s.db.list::<Node>("nodes").await?;
    let online: std::collections::HashSet<String> =
        s.channels.read().await.keys().cloned().collect();
    let inventories = s
        .host_inventories
        .read()
        .map_err(|_| ApiError(axum::http::StatusCode::INTERNAL_SERVER_ERROR, "盘点缓存不可用".into()))?
        .clone();
    Ok(Json(analyze(&nodes, &online, &inventories, now())))
}

/// 触发一次主机盘点。只读操作，但走任务机制以便看到执行结果。
pub async fn host_inspect(
    S(s): S<State>,
    Path(node_id): Path<String>,
    Json(input): Json<ActionRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let task = TaskEnvelope::new(
        node_id.clone(),
        input.idempotency_key,
        Action::HostInspect {},
    );
    let task = s.db.accept_task(&task).await?;
    if task.status == TaskStatus::Pending {
        if let Some(tx) = s.channels.read().await.get(&node_id).map(|c| c.1.clone()) {
            let _ = tx.send(Frame::Task { task: task.clone() }).await;
        }
        let _ = s.events.send(serde_json::json!({"type":"tasks"}));
    }
    Ok(Json(serde_json::json!({ "task_id": task.id })))
}

#[derive(Deserialize)]
pub struct ActionRequest {
    pub idempotency_key: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use opsd::protocol::{DriveRisk, ExistingStorage};

    fn node(id: &str) -> Node {
        Node {
            id: id.into(),
            name: format!("{id} · 测试"),
            public_addresses: vec![],
            overlay_address: None,
            ssh_port: 22,
            revoked: false,
            last_seen: 0,
            capabilities: None,
            inventory: None,
            address_version: 0,
            region: None,
            group: None,
            tags: vec![],
            weight: 0,
            hidden: false,
            public_remark: None,
        }
    }

    fn inventory(disks: &[(&str, u64, bool)], smart: Option<&str>, gaps: Vec<String>) -> HostInventory {
        HostInventory {
            collected_at: 1000,
            hostname: "n".into(),
            distro: "Debian 13".into(),
            kernel: "6.6".into(),
            arch: "x86_64".into(),
            cpu_model: "x".into(),
            cpu_cores: 4,
            memory_total: 1,
            uptime: 1,
            devices: disks
                .iter()
                .map(|(name, size, rotational)| BlockDevice {
                    name: (*name).into(),
                    path: format!("/dev/{name}"),
                    kind: "disk".into(),
                    size: *size,
                    rotational: *rotational,
                    ..Default::default()
                })
                .collect(),
            risks: disks
                .iter()
                .map(|(name, _, _)| DriveRisk {
                    device: (*name).into(),
                    reasons: vec!["未挂载、无文件系统、非系统盘，可用于存储集群".into()],
                    ..Default::default()
                })
                .collect(),
            interfaces: vec![],
            existing: ExistingStorage::default(),
            smart: smart.map(str::to_owned),
            gaps,
        }
    }

    fn online(ids: &[&str]) -> std::collections::HashSet<String> {
        ids.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn 未采集节点判为待采集而不是不适合() {
        let nodes: Vec<Node> = (1..=4).map(|i| node(&format!("C00{i}"))).collect();
        let report = analyze(&nodes, &online(&["C001", "C002", "C003", "C004"]), &Default::default(), 1);
        assert!(report.nodes.iter().all(|v| v.suitability == Suitability::Pending));
        assert!(!report.ready);
        assert!(report.summary.contains("尚未采集"), "总结应指出结论还不完整");
        // 离线节点另有说明
        let offline = analyze(&nodes, &online(&[]), &Default::default(), 1);
        assert!(offline.nodes[0].reasons[0].contains("离线"));
    }

    #[test]
    fn 四台干净节点给出拓扑与可用容量() {
        let nodes: Vec<Node> = (1..=4).map(|i| node(&format!("C00{i}"))).collect();
        let inventories: std::collections::HashMap<String, HostInventory> = (1..=4)
            .map(|i| {
                (
                    format!("C00{i}"),
                    inventory(&[("sdb", 1000 * 1024u64.pow(3), false)], Some("健康"), vec![]),
                )
            })
            .collect();
        let report = analyze(
            &nodes,
            &online(&["C001", "C002", "C003", "C004"]),
            &inventories,
            1,
        );
        assert!(report.ready, "{:?}", report.checks);
        let topology = report.topology.unwrap();
        assert_eq!(topology.nodes, 4);
        assert_eq!(topology.drives_per_node, 1);
        assert_eq!(topology.raw_capacity, 4 * 1000 * 1024u64.pow(3));
        // 4 块盘按默认校验 2 块，可用容量为一半
        assert_eq!(topology.parity, 2);
        assert_eq!(topology.usable_capacity, topology.raw_capacity / 2);
        assert!(report.summary.contains("4 个节点"));
    }

    #[test]
    fn 不足四台时明确指出缺口() {
        let nodes: Vec<Node> = (1..=3).map(|i| node(&format!("C00{i}"))).collect();
        let inventories: std::collections::HashMap<String, HostInventory> = (1..=3)
            .map(|i| {
                (
                    format!("C00{i}"),
                    inventory(&[("sdb", 1024u64.pow(3), false)], Some("健康"), vec![]),
                )
            })
            .collect();
        let report = analyze(&nodes, &online(&["C001", "C002", "C003"]), &inventories, 1);
        assert!(!report.ready);
        assert!(report.topology.is_none());
        let check = report
            .checks
            .iter()
            .find(|c| c.name.contains("候选节点"))
            .unwrap();
        assert!(!check.passed);
        assert!(check.detail.contains("至少需要 4 台"));
    }

    #[test]
    fn 采集缺口会让结论降级为待处理() {
        let nodes: Vec<Node> = (1..=4).map(|i| node(&format!("C00{i}"))).collect();
        let inventories: std::collections::HashMap<String, HostInventory> = (1..=4)
            .map(|i| {
                let gaps = if i == 2 {
                    vec!["SMART 未采集：smartctl 不存在".to_owned()]
                } else {
                    vec![]
                };
                (
                    format!("C00{i}"),
                    inventory(&[("sdb", 1024u64.pow(3), false)], Some("健康"), gaps),
                )
            })
            .collect();
        let report = analyze(&nodes, &online(&["C001", "C002", "C003", "C004"]), &inventories, 1);
        let second = report.nodes.iter().find(|v| v.node_id == "C002").unwrap();
        assert_eq!(second.suitability, Suitability::NeedsWork);
        assert!(second.reasons.iter().any(|r| r.contains("采集缺口")));
        assert!(!report.ready, "有采集缺口时不应直接进入部署");
    }

    #[test]
    fn smart_未采集不算健康() {
        let nodes = vec![node("C001")];
        let inventories = std::collections::HashMap::from([(
            "C001".to_string(),
            inventory(&[("sdb", 1024u64.pow(3), false)], None, vec![]),
        )]);
        let report = analyze(&nodes, &online(&["C001"]), &inventories, 1);
        assert_eq!(report.nodes[0].suitability, Suitability::NeedsWork);
        assert!(
            report.nodes[0]
                .reasons
                .iter()
                .any(|r| r.contains("SMART 未采集"))
        );
    }

    #[test]
    fn 没有干净磁盘的节点判为不适合() {
        let nodes = vec![node("C001")];
        let mut inv = inventory(&[], Some("健康"), vec![]);
        inv.risks.push(DriveRisk {
            device: "sda".into(),
            system_disk: true,
            reasons: vec!["承载系统挂载点（/），不可格式化".into()],
            ..Default::default()
        });
        inv.devices.push(BlockDevice {
            name: "sda".into(),
            path: "/dev/sda".into(),
            kind: "disk".into(),
            size: 100,
            ..Default::default()
        });
        let inventories = std::collections::HashMap::from([("C001".to_string(), inv)]);
        let report = analyze(&nodes, &online(&["C001"]), &inventories, 1);
        assert_eq!(report.nodes[0].suitability, Suitability::Unsuitable);
        assert_eq!(report.nodes[0].rejected.len(), 1);
        assert!(report.nodes[0].rejected[0].reason.contains("系统挂载点"));
    }

    #[test]
    fn 已有存储部署会被点名() {
        let nodes = vec![node("C001")];
        let mut inv = inventory(&[("sdb", 1024u64.pow(3), false)], Some("健康"), vec![]);
        inv.existing.containers.push("minio\tminio/minio".into());
        let inventories = std::collections::HashMap::from([("C001".to_string(), inv)]);
        let report = analyze(&nodes, &online(&["C001"]), &inventories, 1);
        assert!(
            report.nodes[0]
                .reasons
                .iter()
                .any(|r| r.contains("已存在对象存储痕迹")),
            "必须提示可能与既有部署冲突"
        );
    }

    #[test]
    fn 容量差异过大时提示会浪费大盘() {
        let nodes: Vec<Node> = (1..=4).map(|i| node(&format!("C00{i}"))).collect();
        let inventories: std::collections::HashMap<String, HostInventory> = (1..=4)
            .map(|i| {
                let size = if i == 4 { 4000u64 } else { 1000 };
                (
                    format!("C00{i}"),
                    inventory(&[("sdb", size * 1024u64.pow(3), false)], Some("健康"), vec![]),
                )
            })
            .collect();
        let report = analyze(&nodes, &online(&["C001", "C002", "C003", "C004"]), &inventories, 1);
        let check = report
            .checks
            .iter()
            .find(|c| c.name.contains("容量"))
            .unwrap();
        assert!(!check.passed);
        assert!(check.detail.contains("浪费"));
        assert!(!report.ready);
    }

    #[test]
    fn 介质混用会被指出() {
        let nodes: Vec<Node> = (1..=4).map(|i| node(&format!("C00{i}"))).collect();
        let inventories: std::collections::HashMap<String, HostInventory> = (1..=4)
            .map(|i| {
                (
                    format!("C00{i}"),
                    inventory(
                        &[("sdb", 1024u64.pow(3), i == 2)],
                        Some("健康"),
                        vec![],
                    ),
                )
            })
            .collect();
        let report = analyze(&nodes, &online(&["C001", "C002", "C003", "C004"]), &inventories, 1);
        let check = report
            .checks
            .iter()
            .find(|c| c.name.contains("介质"))
            .unwrap();
        assert!(!check.passed);
        assert!(check.detail.contains("最慢"));
    }

    #[test]
    fn 磁盘数量不一致会被指出() {
        let nodes: Vec<Node> = (1..=4).map(|i| node(&format!("C00{i}"))).collect();
        let inventories: std::collections::HashMap<String, HostInventory> = (1..=4)
            .map(|i| {
                let disks: Vec<(&str, u64, bool)> = if i == 2 {
                    vec![("sdb", 1024u64.pow(3), false), ("sdc", 1024u64.pow(3), false)]
                } else {
                    vec![("sdb", 1024u64.pow(3), false)]
                };
                (
                    format!("C00{i}"),
                    inventory(&disks, Some("健康"), vec![]),
                )
            })
            .collect();
        let report = analyze(&nodes, &online(&["C001", "C002", "C003", "C004"]), &inventories, 1);
        let check = report
            .checks
            .iter()
            .find(|c| c.name.contains("磁盘数量"))
            .unwrap();
        assert!(!check.passed);
        assert!(check.detail.contains("不一致"));
    }

    #[test]
    fn 八块盘以上采用四块校验() {
        let nodes: Vec<Node> = (1..=4).map(|i| node(&format!("C00{i}"))).collect();
        let inventories: std::collections::HashMap<String, HostInventory> = (1..=4)
            .map(|i| {
                let disks: Vec<(&str, u64, bool)> = vec![
                    ("sdb", 1024u64.pow(3), false),
                    ("sdc", 1024u64.pow(3), false),
                    ("sdd", 1024u64.pow(3), false),
                ];
                (format!("C00{i}"), inventory(&disks, Some("健康"), vec![]))
            })
            .collect();
        let report = analyze(&nodes, &online(&["C001", "C002", "C003", "C004"]), &inventories, 1);
        let topology = report.topology.unwrap();
        assert_eq!(topology.nodes * topology.drives_per_node, 12);
        assert_eq!(topology.parity, 4, "12 块盘应按 4 块校验");
        assert_eq!(topology.usable_capacity, topology.raw_capacity / 12 * 8);
        assert!(topology.parity_note.contains("校验 4 块"));
    }

    #[test]
    fn 每个结论都必须有原因() {
        let nodes: Vec<Node> = (1..=5).map(|i| node(&format!("C00{i}"))).collect();
        let mut inventories = std::collections::HashMap::new();
        for i in 1..=5 {
            let mut inv = inventory(&[("sdb", 1024u64.pow(3), false)], Some("健康"), vec![]);
            if i == 3 {
                inv.existing.systemd_units.push("silo.service".into());
            }
            inventories.insert(format!("C00{i}"), inv);
        }
        let report = analyze(&nodes, &online(&["C001", "C002", "C003", "C004"]), &inventories, 1);
        for verdict in &report.nodes {
            assert!(
                !verdict.reasons.is_empty(),
                "{} 没有给出任何原因",
                verdict.node_id
            );
        }
    }

    #[test]
    fn 容量格式化可读() {
        assert_eq!(human_bytes(0), "0 B");
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(1024), "1.0 KiB");
        assert_eq!(human_bytes(1024u64.pow(4)), "1.0 TiB");
    }
}
