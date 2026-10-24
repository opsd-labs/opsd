//! 网络模块仅引导和恢复自身容器；日常容器管理仍由 Docker 模块负责。
use anyhow::{Context as _, Result, bail, ensure};
use opsd_plugin_sdk::{self as sdk, Request};
use serde_json::{Value, json};
use std::path::Path;

const IMAGE: &str = "easytier/easytier:2.6.4";

async fn docker(args: &[String]) -> Result<String> {
    sdk::command("docker", args, &[], None, 600).await
}

fn text<'a>(body: &'a Value, key: &str) -> Result<&'a str> {
    body[key].as_str().filter(|v| !v.is_empty()).with_context(|| format!("缺少 {key}"))
}

fn port(body: &Value, key: &str) -> Result<u16> {
    let port = u16::try_from(body[key].as_u64().with_context(|| format!("缺少 {key}"))?)?;
    ensure!(port > 0, "端口不能为零");
    Ok(port)
}

async fn inspect(name: &str) -> Result<Option<Value>> {
    // Docker 故障不能当作容器不存在。
    docker(&sdk::strings(&["info"])).await?;
    let names = docker(&sdk::strings(&["ps", "-a", "--format", "{{.Names}}"])).await?;
    if !names.lines().any(|value| value == name) { return Ok(None); }
    let value: Value = serde_json::from_str(&docker(&sdk::strings(&["inspect", name])).await?)?;
    Ok(Some(value[0].clone()))
}

async fn ensure_container(name: &str, spec: &Value, mut args: Vec<String>, command: Vec<String>, data: &Path, client: bool) -> Result<Value> {
    let record = sdk::node_directory()?.join(format!("{name}.json"));
    let existing = inspect(name).await?;
    if record.exists() {
        let saved: Value = serde_json::from_slice(&std::fs::read(&record)?)?;
        ensure!(saved == *spec, "已有网络服务的部署参数不同，请核实后处理，不能覆盖");
    } else {
        ensure!(existing.is_none(), "同名容器没有本模块的部署记录，不能接管");
        ensure!(!data.exists() || std::fs::read_dir(data)?.next().is_none(), "数据目录不是空目录，不能覆盖已有网络数据");
        sdk::private_write(&record, &serde_json::to_vec(spec)?)?;
    }
    std::fs::create_dir_all(data)?;
    if let Some(existing) = existing {
        ensure!(existing["Config"]["Labels"]["io.opsd.plugin"] == "et"
            && existing["Config"]["Image"] == IMAGE, "已有容器归属或镜像不符");
        ensure!(existing["Config"]["Cmd"] == json!(command), "已有网络容器启动参数不符");
        ensure!(existing["Mounts"].as_array().is_some_and(|mounts| mounts.iter().any(|m| m["Source"] == data.display().to_string() && m["Destination"] == "/data" && m["RW"] == true)), "已有网络容器数据挂载不符");
        ensure!(existing["HostConfig"]["NetworkMode"] == if client { "host" } else { "default" }
            || (!client && existing["HostConfig"]["NetworkMode"] == "bridge"), "已有网络容器网络模式不符");
        if client {
            ensure!(existing["HostConfig"]["CapAdd"].as_array().is_some_and(|caps| caps.iter().any(|c| c == "NET_ADMIN")), "网络客户端缺少接口管理权限");
        } else {
            let bindings = &existing["HostConfig"]["PortBindings"];
            let bind = text(spec, "bind_address")?;
            for (key, protocol) in [("api_port", "tcp"), ("config_port", text(spec, "protocol")?)] {
                let port = port(spec, key)?;
                ensure!(bindings[format!("{port}/{protocol}")].as_array().is_some_and(|entries| entries.iter().any(|entry| entry["HostIp"] == bind && entry["HostPort"] == port.to_string())), "已有网络服务端口映射不符");
            }
        }
        if existing["State"]["Running"] != true { docker(&sdk::strings(&["start", name])).await?; }
    } else {
        docker(&sdk::strings(&["pull", IMAGE])).await?;
        args.extend(["--name".into(), name.into(), "--label".into(), "io.opsd.plugin=et".into(), "--restart".into(), "unless-stopped".into(), "--mount".into(), format!("type=bind,src={},dst=/data", data.display()), IMAGE.into()]);
        args.extend(command);
        docker(&args).await?;
    }
    let actual = inspect(name).await?.context("未找到已部署网络容器")?;
    ensure!(actual["State"]["Running"] == true, "网络容器未运行，请核实启动日志");
    Ok(json!({"container_id":actual["Id"],"running":true}))
}

pub async fn execute(request: Request) -> Result<Value> {
    let body = &request.body;
    match request.path.as_str() {
        "runtime.inspect" => {
            let installed = sdk::command("sh", &sdk::strings(&["-c", "command -v docker"]), &[], None, 10).await.is_ok();
            let running = installed && docker(&sdk::strings(&["info"])).await.is_ok();
            Ok(json!({"installed":installed,"running":running}))
        }
        "network.preflight" => {
            let selected: ipnet::Ipv4Net = text(body, "cidr")?.parse()?;
            let address = text(body, "address")?;
            let interfaces: Value = serde_json::from_str(&sdk::command("ip", &sdk::strings(&["-j", "-4", "address", "show"]), &[], None, 10).await?)?;
            for interface in interfaces.as_array().context("无法读取节点地址")? {
                for item in interface["addr_info"].as_array().into_iter().flatten() {
                    let local = item["local"].as_str().context("节点地址无效")?;
                    if interface["ifname"] == "opsd-et" && local == address { continue; }
                    let network: ipnet::Ipv4Net = format!("{local}/{}", item["prefixlen"]).parse()?;
                    ensure!(!selected.contains(&network.addr()) && !network.contains(&selected.addr()), "所选 ET 网段与节点现有接口重叠，请重新选择网段");
                }
            }
            Ok(json!({"available":true}))
        }
        "web.install" => {
            let bind = text(body, "bind_address")?;
            sdk::require_local_address(bind).await?;
            let api_port = port(body, "api_port")?;
            let config_port = port(body, "config_port")?;
            let protocol = text(body, "protocol")?;
            ensure!(["tcp", "udp"].contains(&protocol), "配置下发协议只支持 TCP 或 UDP");
            ensure!(api_port != config_port, "管理与配置下发端口不能相同");
            let data = Path::new(text(body, "data_dir")?);
            ensure!(data.is_absolute() && !data.to_string_lossy().contains([',', ':', '\n', '\r']), "数据目录必须为可挂载的绝对路径");
            let url = reqwest::Url::parse(text(body, "api_url")?)?;
            ensure!(["http", "https"].contains(&url.scheme()) && url.host_str().is_some() && url.username().is_empty() && url.password().is_none(), "管理地址无效");
            let host = if bind.contains(':') { format!("[{bind}]") } else { bind.into() };
            let args = sdk::strings(&["run", "-d", "--entrypoint", "/sbin/tini", "-p", &format!("{host}:{api_port}:{api_port}/tcp"), "-p", &format!("{host}:{config_port}:{config_port}/{protocol}")]);
            let command = sdk::strings(&["--", "easytier-web-embed", "--db", "/data/et.db", "--api-server-port", &api_port.to_string(), "--api-host", url.as_str(), "--config-server-port", &config_port.to_string(), "--config-server-protocol", protocol]);
            ensure_container("opsd-et-web", body, args, command, data, false).await
        }
        "client.install" => {
            let machine = uuid::Uuid::parse_str(text(body, "machine_id")?)?.to_string();
            let endpoint = reqwest::Url::parse(text(body, "config_endpoint")?)?;
            ensure!(["tcp", "udp", "ws", "wss"].contains(&endpoint.scheme()) && endpoint.host_str().is_some() && endpoint.username().is_empty() && endpoint.password().is_none(), "配置下发地址无效");
            ensure!(Path::new("/dev/net/tun").exists(), "节点缺少 TUN 设备，无法创建网络接口");
            // 一个受管客户端可承载多个实例，首次安装前不能接管已有未知进程。
            if inspect("opsd-et-client").await?.is_none() {
                for entry in std::fs::read_dir("/proc")? {
                    let entry = entry?;
                    let name = entry.file_name();
                    if !name.to_string_lossy().bytes().all(|c| c.is_ascii_digit()) { continue; }
                    let comm = std::fs::read_to_string(entry.path().join("comm")).unwrap_or_default();
                    ensure!(!comm.trim().starts_with("easytier-core"), "发现已有 ET 客户端，请通过现有服务关联，不自动覆盖");
                }
            }
            let data = sdk::node_directory()?.join("client");
            let args = sdk::strings(&["run", "-d", "--network", "host", "--cap-add", "NET_ADMIN", "--device", "/dev/net/tun", "--entrypoint", "/sbin/tini"]);
            let command = sdk::strings(&["--", "easytier-core", "--machine-id", &machine, "--config-dir", "/data", "-w", endpoint.as_str()]);
            ensure_container("opsd-et-client", body, args, command, &data, true).await
        }
        "network.verify" => {
            let address = text(body, "address")?;
            sdk::require_local_address(address).await?;
            for peer in body["peers"].as_array().context("缺少网络成员地址")? {
                let peer: std::net::Ipv4Addr = peer.as_str().context("成员地址无效")?.parse()?;
                sdk::command("ping", &sdk::strings(&["-I", address, "-c", "1", "-W", "3", &peer.to_string()]), &[], None, 10).await.with_context(|| format!("到 {peer} 的组网连通尚未确认"))?;
            }
            Ok(json!({"verified":true}))
        }
        _ => bail!("未知网络节点动作"),
    }
}
