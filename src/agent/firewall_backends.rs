use super::firewall::{Policy, binary, valid_rule};
use super::*;
use std::path::Path;

pub fn render_iptables(p: &Policy, raw: &str, v4: bool) -> Result<String> {
    let mut out = String::from("*filter\n");
    let mut deletes = Vec::new();
    for line in raw.lines() {
        if line.starts_with("-A ") && line.contains("opsd:hook") {
            let parts: Vec<_> = line.split_whitespace().collect();
            anyhow::ensure!(
                parts
                    .last()
                    .is_some_and(|c| ["OPSD_INPUT", "OPSD_OUTPUT", "OPSD_FORWARD"].contains(c)),
                "发现无法归属的 opsd 跳转"
            );
            deletes.push(line.replacen("-A ", "-D ", 1));
        }
    }
    for (_, chain, _) in [
        ("INPUT", "OPSD_INPUT", &p.input_policy),
        ("OUTPUT", "OPSD_OUTPUT", &p.output_policy),
        ("FORWARD", "OPSD_FORWARD", &p.forward_policy),
    ] {
        let exists = raw.lines().any(|l| l.starts_with(&format!(":{chain} ")));
        if p.adopted {
            out += &format!(":{chain} - [0:0]\n-F {chain}\n");
        } else if exists {
            out += &format!("-F {chain}\n");
        }
    }
    for delete in deletes {
        out += &format!("{delete}\n");
    }
    for (direction, chain, default) in [
        ("INPUT", "OPSD_INPUT", &p.input_policy),
        ("OUTPUT", "OPSD_OUTPUT", &p.output_policy),
        ("FORWARD", "OPSD_FORWARD", &p.forward_policy),
    ] {
        if !p.adopted {
            if raw.lines().any(|l| l.starts_with(&format!(":{chain} "))) {
                out += &format!("-X {chain}\n");
            }
            continue;
        }
        for ip in &p.peers.addresses {
            if validate_public_address(ip)?.is_ipv4() == v4 {
                let selector = if direction == "OUTPUT" { "-d" } else { "-s" };
                out += &format!(
                    "-A {chain} {selector} {ip} -m comment --comment opsd:peer -j ACCEPT\n"
                );
            }
        }
        out += &format!("-A {chain} -m conntrack --ctstate ESTABLISHED,RELATED -j ACCEPT\n");
        if direction == "INPUT" {
            out += &format!("-A {chain} -i lo -j ACCEPT\n");
        } else if direction == "OUTPUT" {
            out += &format!("-A {chain} -o lo -j ACCEPT\n");
        }
        if !v4 {
            for kind in [133, 134, 135, 136] {
                out += &format!("-A {chain} -p ipv6-icmp --icmpv6-type {kind} -j ACCEPT\n");
            }
        }
        for r in p.rules.iter().filter(|r| {
            r.enabled && (r.family == 4) == v4 && r.direction.eq_ignore_ascii_case(direction)
        }) {
            valid_rule(r)?;
            out += &format!("-A {chain}");
            if let Some(c) = &r.source {
                out += &format!(" -s {c}")
            }
            if let Some(c) = &r.destination {
                out += &format!(" -d {c}")
            }
            if r.protocol != "any" {
                out += &format!(
                    " -p {}",
                    if r.protocol == "icmpv6" {
                        "ipv6-icmp"
                    } else {
                        &r.protocol
                    }
                )
            }
            if !r.ports.is_empty() {
                out += &format!(
                    " -m multiport --dports {}",
                    r.ports
                        .iter()
                        .map(u16::to_string)
                        .collect::<Vec<_>>()
                        .join(",")
                );
            }
            out += &format!(
                " -m comment --comment opsd:rule:{} -j {}\n",
                r.id,
                r.action.to_uppercase()
            );
        }
        out += &format!(
            "-A {chain} -j {}\n",
            if default == "drop" { "DROP" } else { "RETURN" }
        );
        out += &format!("-I {direction} 1 -m comment --comment opsd:hook -j {chain}\n");
        if direction == "FORWARD" && raw.lines().any(|l| l.starts_with(":DOCKER-USER ")) {
            out += &format!("-I DOCKER-USER 1 -m comment --comment opsd:hook -j {chain}\n");
        }
    }
    out += "COMMIT\n";
    Ok(out)
}

#[cfg(all(test, target_os = "linux"))]
#[tokio::test]
#[ignore = "需要专用 Linux 网络命名空间实验室"]
async fn isolated_tcp_network_policy() -> Result<()> {
    anyhow::ensure!(
        std::env::var("OPSD_LAB").as_deref() == Ok("1"),
        "必须指定实验室"
    );
    let dir = tempfile::tempdir()?;
    let policy = Policy {
        adopted: true,
        input_policy: "drop".into(),
        output_policy: "drop".into(),
        peers: PeerAddressSet {
            version: 1,
            addresses: vec!["11.0.0.2".into(), "2001:4860::2".into()],
        },
        ..Default::default()
    };
    for (file, v4) in [("4.rules", true), ("6.rules", false)] {
        std::fs::write(dir.path().join(file), render_iptables(&policy, "", v4)?)?;
    }
    command(
        "/usr/bin/python3",
        &[
            "tests/firewall-netns.py",
            dir.path().to_str().context("测试路径编码错误")?,
        ],
        60,
    )
    .await?;
    Ok(())
}
pub async fn iptables_policy(ctx: &ContextState, p: &Policy) -> Result<()> {
    let mut files = Vec::new();
    // 先检查两个地址族的完整事务，再分别提交，跨地址族失败由看门狗回滚。
    for v4 in [true, false] {
        let tool = if v4 {
            "iptables-restore"
        } else {
            "ip6tables-restore"
        };
        let save = if v4 {
            "iptables-save"
        } else {
            "ip6tables-save"
        };
        let raw = binary(save, &[]).await?;
        let rendered = render_iptables(p, &raw, v4)?;
        let file = ctx.dir.join(format!("rules-{}.restore", id()));
        pki::private_write(&file, &rendered)?;
        binary(
            tool,
            &[
                "--test",
                "--noflush",
                "--wait",
                "5",
                file.to_str().context("规则文件路径无效")?,
            ],
        )
        .await?;
        files.push((tool, file, rendered));
    }
    for (tool, file, _) in &files {
        binary(
            tool,
            &[
                "--noflush",
                "--wait",
                "5",
                file.to_str().context("规则文件路径无效")?,
            ],
        )
        .await?;
    }
    for (_, file, _) in files {
        std::fs::remove_file(file)?;
    }
    Ok(())
}

pub fn replace_block(original: &str, content: Option<&str>) -> Result<String> {
    const START: &str = "# BEGIN OPSD MANAGED\n";
    const END: &str = "# END OPSD MANAGED\n";
    let base = if let Some(start) = original.find(START) {
        let end = original[start..].find(END).context("受管配置块不完整")? + start + END.len();
        format!("{}{}", &original[..start], &original[end..])
    } else {
        anyhow::ensure!(!original.contains(END), "受管配置结束标记孤立");
        original.to_string()
    };
    if let Some(content) = content {
        let offset = base.find("*filter\n").context("UFW 缺少 filter 配置段")? + 8;
        Ok(format!(
            "{}{START}{content}{END}{}",
            &base[..offset],
            &base[offset..]
        ))
    } else {
        Ok(base)
    }
}
pub async fn ufw_policy(ctx: &ContextState, p: &Policy) -> Result<()> {
    for (v4, file) in [
        (true, "/etc/ufw/before.rules"),
        (false, "/etc/ufw/before6.rules"),
    ] {
        let path = Path::new(file);
        let original = std::fs::read_to_string(path)?;
        let mut persistent = p.clone();
        // UFW 的默认规则放在自身 before 链之后；受管链保留明确的默认行为。
        persistent.backend = "ufw".into();
        let rendered = render_iptables(&persistent, "", v4)?;
        let content = rendered
            .lines()
            .filter(|l| *l != "*filter" && *l != "COMMIT" && !l.starts_with("-F "))
            .map(|l| {
                if l.starts_with("-I INPUT ") {
                    "-A ufw-before-input -m comment --comment opsd:hook -j OPSD_INPUT".to_string()
                } else if l.starts_with("-I OUTPUT ") {
                    "-A ufw-before-output -m comment --comment opsd:hook -j OPSD_OUTPUT".to_string()
                } else if l.starts_with("-I FORWARD ") {
                    "-A ufw-before-forward -m comment --comment opsd:hook -j OPSD_FORWARD"
                        .to_string()
                } else {
                    l.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        let result = replace_block(&original, if p.adopted { Some(&content) } else { None })?;
        if original != result {
            super::stack::atomic_replace(path, result.as_bytes())?;
        }
    }
    iptables_policy(ctx, p).await
}

fn rich_rules(p: &Policy, direction: &str) -> Result<Vec<String>> {
    let mut rules = Vec::new();
    for ip in &p.peers.addresses {
        let addr = validate_public_address(ip)?;
        rules.push(format!(
            "rule family=\"ipv{}\" priority=\"-32768\" {} address=\"{addr}\" accept",
            if addr.is_ipv4() { 4 } else { 6 },
            if direction == "output" {
                "destination"
            } else {
                "source"
            }
        ));
    }
    for (index, r) in p
        .rules
        .iter()
        .filter(|r| r.enabled && r.direction == direction)
        .enumerate()
    {
        valid_rule(r)?;
        let mut base = format!(
            "rule family=\"ipv{}\" priority=\"{}\"",
            r.family,
            -30000 + index as i32
        );
        if let Some(c) = &r.source {
            base += &format!(" source address=\"{c}\"")
        }
        if let Some(c) = &r.destination {
            base += &format!(" destination address=\"{c}\"")
        }
        if r.ports.is_empty() {
            if r.protocol != "any" {
                base += &format!(
                    " protocol value=\"{}\"",
                    if r.protocol == "icmpv6" {
                        "ipv6-icmp"
                    } else {
                        &r.protocol
                    }
                )
            }
            rules.push(base + &format!(" {}", r.action));
        } else {
            for port in &r.ports {
                rules.push(format!(
                    "{base} port port=\"{port}\" protocol=\"{}\" {}",
                    r.protocol, r.action
                ));
            }
        }
    }
    Ok(rules)
}
async fn fw(args: Vec<String>) -> Result<String> {
    binary(
        "firewall-cmd",
        &args.iter().map(String::as_str).collect::<Vec<_>>(),
    )
    .await
}
pub async fn firewalld_policy(p: &Policy, old: &Policy) -> Result<()> {
    for permanent in [false, true] {
        let prefix = if permanent {
            vec!["--permanent".into()]
        } else {
            vec![]
        };
        let call = |extra: Vec<String>| {
            let mut args = prefix.clone();
            args.extend(extra);
            fw(args)
        };
        let policies = call(vec!["--get-policies".into()]).await?;
        for (direction, ingress, egress, target) in [
            ("input", "ANY", "HOST", &p.input_policy),
            ("output", "HOST", "ANY", &p.output_policy),
            ("forward", "ANY", "ANY", &p.forward_policy),
        ] {
            let name = format!("opsd-{direction}");
            let exists = policies.split_whitespace().any(|s| s == name);
            if !p.adopted {
                if exists && old.adopted {
                    call(vec![format!("--delete-policy={name}")]).await?;
                }
                continue;
            }
            anyhow::ensure!(!exists || old.adopted, "同名 firewalld policy 已被外部占用");
            if !exists {
                call(vec![format!("--new-policy={name}")]).await?;
                call(vec![
                    format!("--policy={name}"),
                    "--set-priority=-30000".into(),
                ])
                .await?;
            }
            let policy_arg = format!("--policy={name}");
            let previous = rich_rules(old, direction)?;
            let next = rich_rules(p, direction)?;
            // 先添加节点放行，再更新普通规则；只操作登记过的 rich rule。
            for r in next.iter().filter(|r| !previous.contains(r)) {
                call(vec![policy_arg.clone(), format!("--add-rich-rule={r}")]).await?;
            }
            for r in previous.iter().filter(|r| !next.contains(r)) {
                call(vec![policy_arg.clone(), format!("--remove-rich-rule={r}")]).await?;
            }
            call(vec![
                policy_arg.clone(),
                format!(
                    "--set-target={}",
                    if target == "drop" { "DROP" } else { "CONTINUE" }
                ),
            ])
            .await?;
            if !exists {
                call(vec![
                    policy_arg.clone(),
                    format!("--add-ingress-zone={ingress}"),
                ])
                .await?;
                call(vec![policy_arg, format!("--add-egress-zone={egress}")]).await?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn 事务只清空受管链且保留容器扩展入口() {
        let p = Policy {
            adopted: true,
            ..Default::default()
        };
        let raw = ":DOCKER-USER - [0:0]\n:EXTERNAL - [0:0]\n-A INPUT -j EXTERNAL\n";
        let text = render_iptables(&p, raw, true).unwrap();
        assert!(text.contains("-I DOCKER-USER 1"));
        assert!(!text.contains("-F INPUT"));
        assert!(!text.contains("-F DOCKER"));
        assert!(!text.contains("-F EXTERNAL"));
        assert!(text.ends_with("COMMIT\n"));
    }
    #[test]
    fn 配置块更新保留并发外部内容() {
        let source =
            "*filter\n# BEGIN OPSD MANAGED\n旧规则\n# END OPSD MANAGED\n外部新增规则\nCOMMIT\n";
        let result = replace_block(source, Some("新规则\n")).unwrap();
        assert!(result.contains("外部新增规则"));
        assert!(!result.contains("旧规则"));
        assert!(replace_block("# END OPSD MANAGED\n", None).is_err());
    }
}
