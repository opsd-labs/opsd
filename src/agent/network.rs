use super::*;
use opsd::network::*;
use std::net::IpAddr;

pub async fn facts() -> NetworkFacts {
    let collected_at = now();
    let result: Result<Vec<InterfaceAddress>> = async {
        let text = command("ip", &["-j", "address", "show"], 10).await?;
        let rows: Vec<Value> = serde_json::from_str(&text)?;
        let mut addresses = Vec::new();
        for row in rows {
            let Some(interface) = row["ifname"].as_str() else {
                continue;
            };
            for item in row["addr_info"].as_array().into_iter().flatten() {
                if let (Some(address), Some(prefix)) =
                    (item["local"].as_str(), item["prefixlen"].as_u64())
                {
                    addresses.push(InterfaceAddress {
                        interface: interface.into(),
                        address: address.into(),
                        prefix: prefix as u8,
                    });
                }
            }
        }
        Ok(addresses)
    }
    .await;
    NetworkFacts {
        hostname: std::fs::read_to_string("/etc/hostname")
            .unwrap_or_default()
            .trim()
            .into(),
        interfaces: result.as_ref().cloned().unwrap_or_default(),
        collected_at,
        error: result.err().map(|e| e.to_string()),
    }
}

pub struct Link {
    pub generation: String,
    pub tx: mpsc::Sender<Frame>,
    pub connected_at: i64,
}
fn selected(paths: &std::collections::BTreeMap<PathKind, Link>) -> Option<(PathKind, &Link)> {
    let public = paths.get(&PathKind::Public).filter(|l| !l.tx.is_closed());
    let overlay = paths.get(&PathKind::Overlay).filter(|l| !l.tx.is_closed());
    if let Some(link) = public
        && (overlay.is_none() || now() - link.connected_at >= 15)
    {
        return Some((PathKind::Public, link));
    }
    overlay
        .map(|l| (PathKind::Overlay, l))
        .or_else(|| public.map(|l| (PathKind::Public, l)))
}
pub async fn preferred(ctx: &ContextState) {
    *ctx.outbound.write().await = selected(&*ctx.paths.read().await).map(|(_, l)| l.tx.clone());
}
pub async fn active(ctx: &ContextState, path: PathKind, generation: &str) -> bool {
    selected(&*ctx.paths.read().await).is_some_and(|(p, l)| p == path && l.generation == generation)
}
pub async fn overlay_active(ctx: &ContextState) -> bool {
    selected(&*ctx.paths.read().await).is_some_and(|(p, _)| p == PathKind::Overlay)
}
pub async fn dial_loop(ctx: ContextState, worker: mpsc::Sender<String>, path: PathKind) {
    let mut backoff = 1;
    loop {
        let overlay = ctx
            .db
            .get::<DesiredNetwork>("network", "desired")
            .await
            .ok()
            .flatten()
            .and_then(|d| d.hub_overlay_address);
        if path == PathKind::Overlay && overlay.is_none() {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            continue;
        }
        let generation = id();
        let started = now();
        if let Err(error) =
            super::connection(ctx.clone(), worker.clone(), path, &generation, overlay).await
        {
            tracing::warn!(?path,%error,"控制线路断开");
        }
        {
            let mut paths = ctx.paths.write().await;
            if paths.get(&path).is_some_and(|l| l.generation == generation) {
                paths.remove(&path);
            }
        }
        preferred(&ctx).await;
        if now() - started > 30 {
            backoff = 1;
        }
        tokio::time::sleep(std::time::Duration::from_secs(backoff)).await;
        backoff = (backoff * 2).min(30);
    }
}

pub async fn receive(ctx: &ContextState, desired: DesiredNetwork) -> Result<()> {
    let _guard = ctx.desired_gate.lock().await;
    if ctx
        .db
        .get::<DesiredNetwork>("network", "desired")
        .await?
        .is_some_and(|old| old.revision >= desired.revision)
    {
        return Ok(());
    }
    probe::save(&ctx.db, &desired.probes).await?;
    ctx.db.put("network", "desired", &desired).await?;
    ctx.network_changed.notify_one();
    Ok(())
}

pub async fn reconcile_loop(ctx: ContextState) {
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(60));
    loop {
        tokio::select! {_=tick.tick()=>{},_=ctx.network_changed.notified()=>{}}
        if let Err(error) = reconcile(&ctx).await {
            tracing::warn!(%error,"网络期望状态应用失败，将追赶最新版本");
        }
    }
}
pub(super) async fn reconcile(ctx: &ContextState) -> Result<()> {
    let _guard = ctx.network.lock().await;
    let Some(desired) = ctx.db.get::<DesiredNetwork>("network", "desired").await? else {
        return Ok(());
    };
    let result = if super::container_mode() {
        Err(anyhow::anyhow!(
            "Docker Agent 不修改宿主机防火墙，请使用宿主机 Agent"
        ))
    } else {
        apply_allowlist(ctx, &desired.allowlist).await
    };
    let old = ctx
        .db
        .get::<AppliedNetwork>("network", "applied")
        .await?
        .unwrap_or_default();
    let applied = AppliedNetwork {
        desired_revision: desired.allowlist.revision,
        applied_revision: if result.is_ok() {
            desired.allowlist.revision
        } else {
            old.applied_revision
        },
        backend: result.as_ref().ok().cloned().or(old.backend),
        error: result.err().map(|e| e.to_string()),
        at: now(),
    };
    ctx.db.put("network", "applied", &applied).await?;
    if let Some(tx) = ctx.outbound.read().await.as_ref() {
        let _ = tx.try_send(Frame::NetworkApplied { applied });
    }
    Ok(())
}

fn valid_set(set: &Allowlist) -> Result<()> {
    for address in &set.public_addresses {
        anyhow::ensure!(public_ip(address.parse()?), "白名单含非公网地址");
    }
    if let Some(overlay) = &set.overlay {
        let _: ipnet::IpNet = overlay.network.parse()?;
        anyhow::ensure!(
            !overlay.interface.is_empty()
                && overlay.interface.len() <= 15
                && overlay
                    .interface
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_-.:".contains(&b)),
            "ET 接口名无效"
        );
        anyhow::ensure!(
            std::path::Path::new("/sys/class/net")
                .join(&overlay.interface)
                .exists(),
            "ET 接口尚未出现"
        );
    }
    Ok(())
}

async fn stdin_command(executable: &str, args: &[&str], input: &str) -> Result<()> {
    use tokio::io::AsyncWriteExt;
    let mut child = tokio::process::Command::new(executable)
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;
    child
        .stdin
        .take()
        .context("缺少规则输入管道")?
        .write_all(input.as_bytes())
        .await?;
    let output = tokio::time::timeout(std::time::Duration::from_secs(20), child.wait_with_output())
        .await??;
    anyhow::ensure!(
        output.status.success(),
        "白名单应用失败：{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

fn nft_identifier(name: &str) -> Result<&str> {
    anyhow::ensure!(
        !name.is_empty()
            && (name.as_bytes()[0].is_ascii_alphabetic() || name.starts_with('_'))
            && name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b)),
        "防火墙对象名称无法安全引用，停止写入"
    );
    Ok(name)
}

fn nft_rules(set: &Allowlist, family: &str, table: &str, direction: &str) -> Result<String> {
    let table = nft_identifier(table)?;
    let chain = format!("opsd_peers_{direction}");
    let mut text = String::new();
    let address_field = if direction == "output" {
        "daddr"
    } else {
        "saddr"
    };
    for v4 in [true, false] {
        if (family == "ip" && !v4) || (family == "ip6" && v4) {
            continue;
        }
        let addresses: Vec<_> = set
            .public_addresses
            .iter()
            .filter(|a| a.parse::<IpAddr>().is_ok_and(|ip| ip.is_ipv4() == v4))
            .collect();
        if !addresses.is_empty() {
            text += &format!(
                "add rule {family} {table} {chain} {} {address_field} {{ {} }} accept\n",
                if v4 { "ip" } else { "ip6" },
                addresses
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
    }
    if let Some(overlay) = &set.overlay {
        let net: ipnet::IpNet = overlay.network.parse()?;
        if family == "inet" || (family == "ip") == net.addr().is_ipv4() {
            text += &format!(
                "add rule {family} {table} {chain} {} {} {} {address_field} {} accept\n",
                if direction == "output" {
                    "oifname"
                } else {
                    "iifname"
                },
                serde_json::to_string(&overlay.interface)?,
                if net.addr().is_ipv4() { "ip" } else { "ip6" },
                net
            );
        }
    }
    Ok(text)
}

async fn apply_nft(
    ctx: &ContextState,
    set: &Allowlist,
    backend: &str,
    skip_compat: bool,
) -> Result<String> {
    let state: Value =
        serde_json::from_str(&command("nft", &["-j", "list", "ruleset"], 10).await?)?;
    let rows = state["nftables"].as_array().context("nftables 输出无效")?;
    let chains: Vec<_> = rows.iter().filter_map(|r| r.get("chain")).collect();
    let mut bases: Vec<(String, String, String, String)> = chains
        .iter()
        .filter_map(|c| {
            let family = c["family"].as_str()?;
            let hook = c["hook"].as_str()?;
            (["inet", "ip", "ip6"].contains(&family)
                && ["input", "output", "forward"].contains(&hook)
                && !(skip_compat && ["ip", "ip6"].contains(&family) && c["table"] == "filter")
                && c["type"] == "filter")
                .then(|| {
                    (
                        family.to_owned(),
                        c["table"].as_str().unwrap().to_owned(),
                        c["name"].as_str().unwrap().to_owned(),
                        hook.to_owned(),
                    )
                })
        })
        .collect();
    if skip_compat && bases.is_empty() {
        return Ok(backend.into());
    }
    let old = ctx.db.get::<Allowlist>("network", "installed_set").await?;
    let hooks: Vec<_> = rows
        .iter()
        .filter_map(|r| r.get("rule"))
        .filter(|r| {
            r["comment"] == "opsd:peer"
                && !(skip_compat
                    && ["ip", "ip6"].iter().any(|family| r["family"] == *family)
                    && r["table"] == "filter")
        })
        .collect();
    if old.as_ref() == Some(set)
        && !bases.is_empty()
        && bases.iter().all(|(f, t, c, hook)| {
            let owned = format!("opsd_peers_{hook}");
            let expected = nft_rules(set, f, t, hook)
                .map(|rules| rules.lines().count())
                .unwrap_or(usize::MAX);
            let actual = rows
                .iter()
                .filter_map(|r| r.get("rule"))
                .filter(|r| r["family"] == *f && r["table"] == *t && r["chain"] == owned)
                .count();
            actual == expected
                && rows
                    .iter()
                    .filter_map(|r| r.get("rule"))
                    .find(|r| r["family"] == *f && r["table"] == *t && r["chain"] == *c)
                    .is_some_and(|r| {
                        r["comment"] == "opsd:peer"
                            && r["expr"].as_array().is_some_and(|expr| {
                                expr.iter().any(|item| {
                                    item["jump"]["target"] == format!("opsd_peers_{hook}")
                                })
                            })
                    })
        })
    {
        return Ok(backend.into());
    }
    let mut text = String::new();
    if bases.is_empty() {
        if !rows
            .iter()
            .any(|r| r.pointer("/table/name").and_then(Value::as_str) == Some("opsd_peers"))
        {
            text += "add table inet opsd_peers\n";
        }
        for hook in ["input", "output", "forward"] {
            let name = format!("base_{hook}");
            if !chains
                .iter()
                .any(|c| c["table"] == "opsd_peers" && c["name"] == name)
            {
                text += &format!(
                    "add chain inet opsd_peers {name} {{ type filter hook {hook} priority -10; policy accept; }}\n"
                );
            }
            bases.push(("inet".into(), "opsd_peers".into(), name, hook.into()));
        }
    }
    for rule in hooks {
        text += &format!(
            "delete rule {} {} {} handle {}\n",
            rule["family"].as_str().context("缺少规则族")?,
            nft_identifier(rule["table"].as_str().context("缺少表")?)?,
            nft_identifier(rule["chain"].as_str().context("缺少链")?)?,
            rule["handle"].as_u64().context("缺少规则句柄")?
        );
    }
    let mut prepared = std::collections::HashSet::new();
    for (family, table, base, hook) in &bases {
        if prepared.insert((family, table, hook)) {
            let owned = format!("opsd_peers_{hook}");
            text += &format!(
                "{} chain {family} {} {owned}\n",
                if chains
                    .iter()
                    .any(|c| c["family"] == *family && c["table"] == *table && c["name"] == owned)
                {
                    "flush"
                } else {
                    "add"
                },
                nft_identifier(table)?
            );
            text += &nft_rules(set, family, table, hook)?;
        }
        text += &format!(
            "insert rule {family} {} {} jump opsd_peers_{hook} comment \"opsd:peer\"\n",
            nft_identifier(table)?,
            nft_identifier(base)?
        );
    }
    stdin_command("nft", &["-f", "-"], &text).await?;
    ctx.db.put("network", "installed_set", set).await?;
    Ok(backend.into())
}

async fn apply_iptables(ctx: &ContextState, set: &Allowlist, backend: &str) -> Result<String> {
    for v4 in [true, false] {
        let exe = if v4 { "iptables" } else { "ip6tables" };
        let save = if v4 {
            "iptables-save"
        } else {
            "ip6tables-save"
        };
        let restore = if v4 {
            "iptables-restore"
        } else {
            "ip6tables-restore"
        };
        let raw = command(save, &[], 10).await?;
        let mut script = String::from("*filter\n");
        let mut hooks = Vec::new();
        for direction in ["INPUT", "OUTPUT", "FORWARD"] {
            let chain = format!("OPSD_PEERS_{direction}");
            if !raw.lines().any(|l| l.starts_with(&format!(":{chain} "))) {
                script += &format!(":{chain} - [0:0]\n");
            } else {
                script += &format!("-F {chain}\n");
            }
            for address in &set.public_addresses {
                let ip: IpAddr = address.parse()?;
                if ip.is_ipv4() != v4 {
                    continue;
                }
                script += &format!(
                    "-A {chain} {} {ip} -j ACCEPT\n",
                    if direction == "OUTPUT" { "-d" } else { "-s" }
                );
            }
            if let Some(overlay) = &set.overlay {
                let net: ipnet::IpNet = overlay.network.parse()?;
                if net.addr().is_ipv4() == v4 {
                    script += &format!(
                        "-A {chain} {} {} {} {net} -j ACCEPT\n",
                        if direction == "OUTPUT" { "-o" } else { "-i" },
                        overlay.interface,
                        if direction == "OUTPUT" { "-d" } else { "-s" }
                    );
                }
            }
            script += &format!("-A {chain} -j RETURN\n");
            hooks.push((direction.to_owned(), chain.clone()));
            if direction == "FORWARD" && raw.lines().any(|l| l.starts_with(":DOCKER-USER ")) {
                hooks.push(("DOCKER-USER".into(), chain));
            }
        }
        let intact = hooks.iter().all(|(base, chain)| {
            raw.lines()
                .find(|line| line.starts_with(&format!("-A {base} ")))
                .is_some_and(|line| {
                    line.contains("opsd:peer") && line.ends_with(&format!("-j {chain}"))
                })
        });
        for (base, chain) in hooks {
            let rule = format!("-A {base} -m comment --comment opsd:peer -j {chain}");
            // 删除旧跳转后置顶，保留所有非 OPSD 规则。
            for line in raw
                .lines()
                .filter(|l| l.contains("opsd:peer") && l.starts_with(&format!("-A {base} ")))
            {
                script += &(line.replacen("-A ", "-D ", 1) + "\n");
            }
            script += &(rule.replacen("-A ", "-I ", 1).replacen(
                &format!("-I {base} "),
                &format!("-I {base} 1 "),
                1,
            ) + "\n");
        }
        script += "COMMIT\n";
        let old = ctx
            .db
            .get::<Allowlist>("network", &format!("installed_{exe}"))
            .await?;
        if old.as_ref() != Some(set) || !intact {
            stdin_command(restore, &["--noflush"], &script).await?;
            ctx.db
                .put("network", &format!("installed_{exe}"), set)
                .await?;
        }
    }
    Ok(backend.into())
}

pub async fn apply_allowlist(ctx: &ContextState, set: &Allowlist) -> Result<String> {
    valid_set(set)?;
    let firewalld = command("firewall-cmd", &["--state"], 5).await.is_ok();
    let ufw = command("ufw", &["status"], 5)
        .await
        .is_ok_and(|s| s.lines().any(|l| l == "Status: active"));
    anyhow::ensure!(
        !(firewalld && ufw),
        "多个防火墙管理器同时活动，停止白名单写入"
    );
    if firewalld {
        let rules: Value =
            serde_json::from_str(&command("nft", &["-j", "list", "ruleset"], 10).await?)?;
        anyhow::ensure!(
            !rules["nftables"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|r| r.get("table"))
                .any(|table| table["name"] == "firewalld"
                    && table["flags"]
                        .as_array()
                        .is_some_and(|flags| flags.iter().any(|flag| flag == "owner"))),
            "firewalld 独占规则表，当前不能在不重载现有防火墙的前提下安全接入；未修改其保护设置"
        );
    }
    let mut iptables = false;
    for save in ["iptables-save", "ip6tables-save"] {
        iptables |= command(save, &[], 5).await.is_ok_and(|s| {
            s.lines().any(|l| {
                l.starts_with("-A ")
                    || l.starts_with(":INPUT ")
                    || l.starts_with(":OUTPUT DROP")
                    || l.starts_with(":FORWARD DROP")
            })
        });
    }
    let backend = if ufw {
        "ufw"
    } else if firewalld {
        "firewalld"
    } else if iptables {
        "iptables"
    } else {
        "nftables"
    };
    if iptables || ufw {
        apply_iptables(ctx, set, backend).await?;
    }
    // 兼容表仍由 iptables 操作；独立 nft 表里的后续 DROP 也必须覆盖。
    apply_nft(ctx, set, backend, iptables || ufw).await
}
