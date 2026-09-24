use super::*;
use std::{collections::BTreeMap, path::Path};
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub id: String,
    pub family: u8,
    pub direction: String,
    pub protocol: String,
    pub source: Option<String>,
    pub destination: Option<String>,
    pub ports: Vec<u16>,
    pub action: String,
    pub enabled: bool,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Policy {
    pub backend: String,
    pub rules: Vec<Rule>,
    pub peers: PeerAddressSet,
    pub input_policy: String,
    pub output_policy: String,
    pub forward_policy: String,
    pub adopted: bool,
}
impl Default for Policy {
    fn default() -> Self {
        Self {
            backend: String::new(),
            rules: vec![],
            peers: PeerAddressSet {
                version: 0,
                addresses: vec![],
            },
            input_policy: "accept".into(),
            output_policy: "accept".into(),
            forward_policy: "accept".into(),
            adopted: false,
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
struct Plan {
    id: String,
    node_id: String,
    base: String,
    expires: i64,
    before: Policy,
    after: Policy,
    operation: Value,
}
#[derive(Clone, Serialize, Deserialize)]
struct Recovery {
    id: String,
    before: Policy,
    after: Policy,
    state: String,
    deadline: i64,
}
pub(super) fn valid_rule(r: &Rule) -> Result<()> {
    anyhow::ensure!(
        !r.id.is_empty()
            && r.id.len() < 64
            && r.id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b)),
        "规则编号无效"
    );
    anyhow::ensure!(
        matches!(r.family, 4 | 6)
            && ["input", "output", "forward"].contains(&r.direction.as_str())
            && ["tcp", "udp", "icmp", "icmpv6", "any"].contains(&r.protocol.as_str())
            && ["accept", "drop", "reject"].contains(&r.action.as_str()),
        "规则类型无效"
    );
    anyhow::ensure!(
        r.ports.len() <= 15 && r.ports.iter().all(|p| *p > 0),
        "端口必须为 1–65535，最多十五个"
    );
    anyhow::ensure!(
        r.ports.is_empty() || matches!(r.protocol.as_str(), "tcp" | "udp"),
        "只有 TCP/UDP 支持端口"
    );
    anyhow::ensure!(
        !(r.family == 4 && r.protocol == "icmpv6" || r.family == 6 && r.protocol == "icmp"),
        "ICMP 地址族不一致"
    );
    for cidr in [&r.source, &r.destination].into_iter().flatten() {
        let ip: ipnet::IpNet = cidr.parse()?;
        anyhow::ensure!(ip.addr().is_ipv4() == (r.family == 4), "CIDR 地址族不一致");
    }
    Ok(())
}
pub(super) async fn binary(name: &str, args: &[&str]) -> Result<String> {
    let path = [
        format!("/usr/sbin/{name}"),
        format!("/usr/bin/{name}"),
        format!("/sbin/{name}"),
    ]
    .into_iter()
    .find(|p| Path::new(p).exists())
    .context(format!("未安装 {name}"))?;
    command(&path, args, 20).await
}
async fn detect() -> Result<(String, BTreeMap<String, String>)> {
    anyhow::ensure!(cfg!(target_os = "linux"), "防火墙仅支持 Linux");
    let mut raw = BTreeMap::new();
    let firewalld = binary("firewall-cmd", &["--state"])
        .await
        .is_ok_and(|s| s.trim() == "running");
    let ufw_text = binary("ufw", &["status", "verbose"]).await.ok();
    let ufw = ufw_text
        .as_ref()
        .is_some_and(|s| s.contains("Status: active"));
    anyhow::ensure!(!(firewalld && ufw), "UFW 与 firewalld 同时活动，禁止写入");
    if let Some(v) = ufw_text {
        raw.insert("ufw".into(), v);
    }
    if let Ok(v) = binary("nft", &["-j", "list", "ruleset"]).await {
        let _: Value = serde_json::from_str(&v)?;
        raw.insert("nft".into(), v);
    }
    if let Ok(v) = binary("iptables-save", &[]).await {
        raw.insert("iptables".into(), v);
    }
    if let Ok(v) = binary("ip6tables-save", &[]).await {
        raw.insert("ip6tables".into(), v);
    }
    let backend = if firewalld {
        raw.insert(
            "firewalld".into(),
            binary("firewall-cmd", &["--list-all-zones"]).await?,
        );
        "firewalld"
    } else if ufw {
        "ufw"
    } else if raw.get("iptables").is_some_and(|s| s.contains("-A ")) {
        "iptables_nft"
    } else if raw.contains_key("nft") {
        "nftables"
    } else {
        "unsupported"
    };
    if backend == "iptables_nft" {
        anyhow::ensure!(
            binary("iptables", &["--version"])
                .await?
                .contains("nf_tables"),
            "iptables-legacy 不在首版支持范围"
        );
    }
    Ok((backend.into(), raw))
}
fn fingerprint(raw: &BTreeMap<String, String>) -> String {
    let normalized: Vec<_> = raw
        .iter()
        .map(|(k, v)| {
            let v = if k == "nft" {
                let mut parsed: Value = serde_json::from_str(v).unwrap_or(Value::Null);
                strip_counters(&mut parsed);
                parsed.to_string()
            } else {
                v.lines()
                    .filter(|l| !l.starts_with('#'))
                    .map(|l| {
                        if l.starts_with(':') {
                            l.split(" [").next().unwrap_or(l)
                        } else {
                            l
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            (k, v)
        })
        .collect();
    digest(serde_json::to_vec(&normalized).unwrap())
}
fn strip_counters(v: &mut Value) {
    match v {
        Value::Object(m) => {
            m.remove("metainfo");
            if let Some(c) = m.get_mut("counter").and_then(Value::as_object_mut) {
                c.remove("packets");
                c.remove("bytes");
            }
            for v in m.values_mut() {
                strip_counters(v)
            }
        }
        Value::Array(a) => {
            for v in a {
                strip_counters(v)
            }
        }
        _ => {}
    }
}
pub async fn inspect(ctx: &ContextState) -> Result<Value> {
    let (backend, raw) = detect().await?;
    let policy = ctx
        .db
        .get::<Policy>("firewall", "policy")
        .await?
        .unwrap_or_default();
    let listeners = binary("ss", &["-H", "-lntup"])
        .await
        .context("监听端口读取失败")?;
    let services = command(
        "/usr/bin/systemctl",
        &[
            "list-unit-files",
            "--type=service",
            "--no-pager",
            "--no-legend",
        ],
        10,
    )
    .await
    .context("持久化维护服务读取失败")?
    .lines()
    .filter(|l| {
        l.contains("firewall")
            || l.contains("netfilter")
            || l.contains("nftables")
            || l.contains("ufw")
            || l.contains("1panel")
    })
    .map(str::to_owned)
    .collect::<Vec<_>>();
    Ok(
        json!({"backend":backend,"fingerprint":fingerprint(&raw),"policy":policy,"external_rules":raw,"listeners":listeners,"maintenance_services":services,"docker_forwarding":"需检查 DOCKER-USER 或 nftables 转发链"}),
    )
}
pub async fn plan(ctx: &ContextState, operation: &Value) -> Result<Value> {
    let (backend, raw) = detect().await?;
    anyhow::ensure!(backend != "unsupported", "没有受支持的防火墙管理器");
    let before = ctx
        .db
        .get::<Policy>("firewall", "policy")
        .await?
        .unwrap_or_default();
    let mut after = before.clone();
    after.backend = backend;
    match operation["type"].as_str().context("缺少操作类型")? {
        "adopt" => {
            anyhow::ensure!(!before.adopted, "已完成受管区域接管");
            anyhow::ensure!(
                !raw.values()
                    .any(|s| s.contains("OPSD_INPUT") || s.contains("\"name\":\"opsd\"")),
                "发现未登记的 opsd 规则对象，禁止覆盖"
            );
            after.adopted = true;
            if let Some(set) = ctx
                .db
                .get::<PeerAddressSet>("firewall", "desired_peers")
                .await?
            {
                after.peers = set;
            }
        }
        "rule_put" => {
            anyhow::ensure!(before.adopted, "请先接管受管区域");
            let rule: Rule = serde_json::from_value(operation["rule"].clone())?;
            valid_rule(&rule)?;
            if let Some(old) = after.rules.iter_mut().find(|r| r.id == rule.id) {
                *old = rule
            } else {
                after.rules.push(rule)
            }
        }
        "rule_delete" => {
            let id = operation["id"].as_str().context("缺少规则编号")?;
            anyhow::ensure!(after.rules.iter().any(|r| r.id == id), "只能删除受管规则");
            after.rules.retain(|r| r.id != id);
        }
        "rule_order" => {
            let ids: Vec<String> = serde_json::from_value(operation["ids"].clone())?;
            anyhow::ensure!(ids.len() == after.rules.len(), "排序必须包含全部受管规则");
            let mut rules = Vec::new();
            for id in ids {
                let index = after
                    .rules
                    .iter()
                    .position(|r| r.id == id)
                    .context("排序包含未知或重复编号")?;
                rules.push(after.rules.remove(index));
            }
            after.rules = rules;
        }
        "default_policy" => {
            anyhow::ensure!(before.adopted, "请先接管");
            let action = operation["action"].as_str().context("缺少默认动作")?;
            anyhow::ensure!(["accept", "drop"].contains(&action), "默认动作无效");
            match operation["direction"].as_str() {
                Some("input") => after.input_policy = action.into(),
                Some("output") => after.output_policy = action.into(),
                Some("forward") => after.forward_policy = action.into(),
                _ => anyhow::bail!("方向无效"),
            }
        }
        _ => anyhow::bail!("首版不接受原始规则文本或未知操作"),
    }
    validate_backend(&after, &raw)?;
    let p = Plan {
        id: id(),
        node_id: ctx.config.node_id.clone(),
        base: fingerprint(&raw),
        expires: now() + 300,
        before,
        after,
        operation: operation.clone(),
    };
    ctx.db.insert("firewall_plans", &p.id, &p).await?;
    Ok(
        json!({"plan_id":p.id,"expires":p.expires,"before":p.before,"after":p.after,"requires_witness":true,"rollback_seconds":120,"external_rules_preserved":true}),
    )
}
fn validate_backend(p: &Policy, raw: &BTreeMap<String, String>) -> Result<()> {
    anyhow::ensure!(p.rules.len() <= 512, "受管规则超过首版上限");
    if p.backend == "nftables" {
        let parsed: Value = serde_json::from_str(raw.get("nft").context("缺少 nftables 快照")?)?;
        let chains = parsed["nftables"].as_array().context("nftables 格式无效")?;
        // 其他 base chain 的拒绝不能被本表的 accept 覆盖，必须逐条接管后再开放。
        let external = chains
            .iter()
            .filter_map(|v| v.get("chain"))
            .any(|c| c["hook"].is_string() && c["table"] != "opsd" && c["policy"] == "drop");
        anyhow::ensure!(
            !external || p.peers.addresses.is_empty(),
            "外部 nftables base chain 存在拒绝策略，节点互访需要先接管该链"
        );
    }
    Ok(())
}
pub async fn peers(ctx: &ContextState, set: &PeerAddressSet) -> Result<Value> {
    for ip in &set.addresses {
        validate_public_address(ip)?;
    }
    ctx.db.put("firewall", "desired_peers", set).await?;
    let before = ctx
        .db
        .get::<Policy>("firewall", "policy")
        .await?
        .unwrap_or_default();
    anyhow::ensure!(before.adopted, "节点地址已登记，防火墙尚未接管，等待应用");
    if set.version <= before.peers.version {
        return Ok(json!({"version":before.peers.version}));
    }
    let (backend, raw) = detect().await?;
    anyhow::ensure!(backend == before.backend, "防火墙管理器已变化");
    let mut after = before.clone();
    after.peers = set.clone();
    validate_backend(&after, &raw)?;
    let p = Plan {
        id: id(),
        node_id: ctx.config.node_id.clone(),
        base: fingerprint(&raw),
        expires: now() + 300,
        before,
        after,
        operation: json!({"type":"peer_sync"}),
    };
    ctx.db.insert("firewall_plans", &p.id, &p).await?;
    apply(ctx, &p.id, None).await
}
pub async fn apply(ctx: &ContextState, plan_id: &str, witness: Option<&str>) -> Result<Value> {
    anyhow::ensure!(
        cfg!(feature = "experimental-firewall"),
        "防火墙生产写入尚未通过隔离故障矩阵；当前构建仅支持发现与计划"
    );
    let _process_lock = process_lock(ctx)?;
    let p = ctx
        .db
        .get::<Plan>("firewall_plans", plan_id)
        .await?
        .context("防火墙计划不存在")?;
    anyhow::ensure!(
        p.node_id == ctx.config.node_id && p.expires > now(),
        "计划目标错误或已过期"
    );
    let (_, raw) = detect().await?;
    anyhow::ensure!(p.base == fingerprint(&raw), "规则已变化，请重新生成计划");
    let requires_witness = p.after.input_policy == "drop"
        || p.after.output_policy == "drop"
        || p.after.forward_policy == "drop"
        || p.after
            .rules
            .iter()
            .any(|r| r.enabled && r.action != "accept")
        || p.before.rules.iter().any(|r| !p.after.rules.contains(r))
        || p.before
            .peers
            .addresses
            .iter()
            .any(|ip| !p.after.peers.addresses.contains(ip));
    anyhow::ensure!(
        Path::new("/run/systemd/system").exists(),
        "独立 systemd 回滚不可用"
    );
    let mut recovery = Recovery {
        id: p.id.clone(),
        before: p.before.clone(),
        after: p.after.clone(),
        state: "pending".into(),
        deadline: now() + 120,
    };
    ctx.db
        .insert("firewall_recovery", plan_id, &recovery)
        .await?;
    let exe = std::env::current_exe()?;
    let dir = ctx.dir.canonicalize()?;
    binary(
        "systemd-run",
        &[
            "--unit",
            &format!("opsd-rollback-{plan_id}"),
            "--on-active=120s",
            exe.to_str().context("程序路径无效")?,
            "--data-dir",
            dir.to_str().context("数据路径无效")?,
            "firewall-rollback",
            plan_id,
        ],
    )
    .await?;
    let result: Result<()> = tokio::time::timeout(std::time::Duration::from_secs(90), async {
        write_policy(ctx, &p.after, &p.before).await?;
        verify(ctx, witness, requires_witness).await?;
        for old in p
            .before
            .peers
            .addresses
            .iter()
            .filter(|a| !p.after.peers.addresses.contains(a))
        {
            // 精确清理旧地址；命令失败必须保留错误，不能报告撤销完成。
            let current = binary("conntrack", &["-L", "-s", old]).await?;
            if !current.trim().is_empty() {
                binary("conntrack", &["-D", "-s", old]).await?;
            }
        }
        Ok(())
    })
    .await
    .unwrap_or_else(|_| Err(anyhow::anyhow!("防火墙应用或验证超时")));
    if let Err(e) = result {
        rollback_inner(ctx, plan_id)
            .await
            .context("应用失败且自动回滚失败")?;
        anyhow::bail!("防火墙变更已回滚：{e}")
    }
    recovery.state = "committed".into();
    ctx.db
        .put_records(&[
            ("firewall", "policy", serde_json::to_string(&p.after)?),
            (
                "firewall_recovery",
                plan_id,
                serde_json::to_string(&recovery)?,
            ),
        ])
        .await?;
    binary(
        "systemctl",
        &["stop", &format!("opsd-rollback-{plan_id}.timer")],
    )
    .await?;
    Ok(json!({"operation_id":plan_id,"state":"succeeded","peer_version":p.after.peers.version}))
}
pub async fn rollback(ctx: &ContextState, id: &str) -> Result<()> {
    let _lock = process_lock(ctx)?;
    rollback_inner(ctx, id).await
}
fn process_lock(ctx: &ContextState) -> Result<std::fs::File> {
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(ctx.dir.join("firewall.lock"))?;
    fs2::FileExt::lock_exclusive(&file)?;
    Ok(file)
}
async fn rollback_inner(ctx: &ContextState, id: &str) -> Result<()> {
    let mut r = ctx
        .db
        .get::<Recovery>("firewall_recovery", id)
        .await?
        .context("回滚记录不存在")?;
    if r.state != "pending" {
        return Ok(());
    }
    write_policy(ctx, &r.before, &r.after).await?;
    r.state = "rolled_back".into();
    ctx.db
        .put_records(&[
            ("firewall", "policy", serde_json::to_string(&r.before)?),
            ("firewall_recovery", id, serde_json::to_string(&r)?),
        ])
        .await?;
    Ok(())
}
async fn verify(ctx: &ContextState, witness: Option<&str>, required: bool) -> Result<()> {
    let mut pem = std::fs::read(ctx.dir.join("client.pem"))?;
    pem.extend_from_slice(&std::fs::read(ctx.dir.join("client-key.pem"))?);
    let client = reqwest::Client::builder()
        .tls_built_in_root_certs(false)
        .add_root_certificate(reqwest::Certificate::from_pem(&std::fs::read(
            ctx.dir.join("ca.pem"),
        )?)?)
        .identity(reqwest::Identity::from_pem(&pem)?)
        .timeout(std::time::Duration::from_secs(20))
        .pool_max_idle_per_host(0)
        .build()?;
    let url = ctx
        .config
        .agent_url
        .replace("wss://", "https://")
        .trim_end_matches("/agent")
        .to_string()
        + "/verify";
    for _ in 0..3 {
        client
            .post(&url)
            .json(&json!({"witness":witness,"required":required}))
            .send()
            .await?
            .error_for_status()?;
    }
    Ok(())
}
fn nft_rules(p: &Policy) -> Result<String> {
    let mut s = String::from("table inet opsd {\n");
    for (direction, policy) in [
        ("input", &p.input_policy),
        ("output", &p.output_policy),
        ("forward", &p.forward_policy),
    ] {
        s += &format!(
            "chain {direction} {{ type filter hook {direction} priority -10; policy accept;\n"
        );
        s += if direction == "output" {
            "oifname lo accept\n"
        } else {
            "iifname lo accept\n"
        };
        s += "ct state established,related accept\nmeta l4proto ipv6-icmp icmpv6 type { nd-neighbor-solicit, nd-neighbor-advert, nd-router-solicit, nd-router-advert } accept\n";
        for ip in &p.peers.addresses {
            let addr = validate_public_address(ip)?;
            s += &format!(
                "{} {} {addr} accept comment \"opsd:peer\"\n",
                if addr.is_ipv4() { "ip" } else { "ip6" },
                if direction == "output" {
                    "daddr"
                } else {
                    "saddr"
                }
            );
        }
        for r in p
            .rules
            .iter()
            .filter(|r| r.enabled && r.direction == direction)
        {
            valid_rule(r)?;
            s += &format!(
                "meta nfproto {} ",
                if r.family == 4 { "ipv4" } else { "ipv6" }
            );
            let family = if r.family == 4 { "ip" } else { "ip6" };
            if let Some(c) = &r.source {
                s += &format!("{family} saddr {c} ")
            }
            if let Some(c) = &r.destination {
                s += &format!("{family} daddr {c} ")
            }
            if r.protocol != "any" {
                s += &format!(
                    "meta l4proto {} ",
                    if r.protocol == "icmpv6" {
                        "ipv6-icmp"
                    } else {
                        &r.protocol
                    }
                )
            }
            if !r.ports.is_empty() {
                s += &format!(
                    "{} dport {{ {} }} ",
                    r.protocol,
                    r.ports
                        .iter()
                        .map(u16::to_string)
                        .collect::<Vec<_>>()
                        .join(",")
                );
            }
            s += &format!("{} comment \"opsd:rule:{}\"\n", r.action, r.id);
        }
        if policy == "drop" {
            s += "drop\n";
        }
        s += "}\n";
    }
    s += "}\n";
    Ok(s)
}
async fn write_policy(ctx: &ContextState, p: &Policy, old: &Policy) -> Result<()> {
    let backend = if p.backend.is_empty() {
        &old.backend
    } else {
        &p.backend
    };
    match backend.as_str() {
        "nftables" => {
            let exists = binary("nft", &["list", "table", "inet", "opsd"])
                .await
                .is_ok();
            let mut text = if exists {
                "delete table inet opsd\n".to_string()
            } else {
                String::new()
            };
            if p.adopted {
                text += &nft_rules(p)?;
            }
            if !text.is_empty() {
                let file = ctx.dir.join(format!("firewall-{}.nft", id()));
                pki::private_write(&file, text)?;
                let path = file.to_str().context("规则文件路径无效")?;
                let result = async {
                    binary("nft", &["-c", "-f", path]).await?;
                    binary("nft", &["-f", path]).await?;
                    Ok::<_, anyhow::Error>(())
                }
                .await;
                std::fs::remove_file(file)?;
                result?;
            }
        }
        "iptables_nft" => super::firewall_backends::iptables_policy(ctx, p).await?,
        "ufw" => super::firewall_backends::ufw_policy(ctx, p).await?,
        "firewalld" => super::firewall_backends::firewalld_policy(p, old).await?,
        _ => anyhow::bail!("防火墙管理器不支持写入"),
    };
    Ok(())
}
