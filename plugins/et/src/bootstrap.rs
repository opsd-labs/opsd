use super::{api, member, sdk, uuid};
use anyhow::{Context as _, Result, ensure};
use opsd_plugin_sdk::{Context, Request};
use serde_json::{Value, json};
use std::sync::OnceLock;

static EXECUTION: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();

fn specification(body: &Value) -> Value {
    let mut spec = body.clone();
    if let Some(object) = spec.as_object_mut() {
        object.remove("idempotency_key");
        object.remove("resume");
    }
    spec
}

async fn save(key: &str, state: &Value) -> Result<()> {
    sdk::kv_put(key, state.clone()).await?;
    Ok(())
}

async fn host_nodes(ids: &[String]) -> Result<()> {
    let nodes = sdk::core("nodes", json!({})).await?;
    for id in ids {
        let node = nodes.as_array().context("节点目录无效")?.iter().find(|node| node["id"] == *id).context("节点不存在")?;
        ensure!(node["capabilities"]["plugins"] == true && node["capabilities"]["execution_mode"] == "host", "初始化需要已升级且支持模块的宿主机 Agent");
    }
    Ok(())
}

pub async fn execute(request: &Request) -> Result<Value> {
    let _guard = EXECUTION.get_or_init(Default::default).lock().await;
    match request.path.as_str() {
        "web.install" => web(request).await,
        "network.create" => network(request).await,
        _ => anyhow::bail!("未知网络初始化操作"),
    }
}

async fn web(request: &Request) -> Result<Value> {
    let mut state = sdk::kv_get("web_deployment").await?;
    if state.is_null() {
        state = json!({"spec":specification(&request.body),"phase":"preparing"});
    } else {
        ensure!(request.body["resume"] == true, "已有部署记录，请核实后继续原部署");
    }
    let spec = state["spec"].clone();
    let node = spec["node_id"].as_str().context("请选择 ET Web 所在节点")?;
    host_nodes(&[node.to_owned()]).await?;
    state["task_id"] = json!(request.context.task_id);
    state["last_error"] = Value::Null;
    save("web_deployment", &state).await?;
    let result: Result<Value> = async {
        let runtime = sdk::node_task(&request.context, node, "runtime.inspect", json!({}), "runtime").await?;
        ensure!(runtime["installed"] == true, "请先到 Docker 页为该节点安装 Docker 基础环境");
        ensure!(runtime["running"] == true, "Docker 已安装但不可用，请先处理服务故障");
        state["phase"] = json!("deploying");
        save("web_deployment", &state).await?;
        let result = sdk::node_task(&request.context, node, "web.install", spec.clone(), "web").await?;
        // 容器运行不等于 Web API 可用；采用向导中明确填写的 Hub 可达地址。
        let base = reqwest::Url::parse(spec["api_url"].as_str().context("缺少管理访问地址")?)?;
        ensure!(["http", "https"].contains(&base.scheme()) && base.host_str().is_some(), "管理访问地址无效");
        let client = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).timeout(std::time::Duration::from_secs(5)).build()?;
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(60);
        loop {
            let response = client.get(format!("{}/api/v1/auth/captcha", base.as_str().trim_end_matches('/'))).send().await;
            if response.is_ok_and(|r| r.status().is_success()) { break; }
            ensure!(tokio::time::Instant::now() < deadline, "网络容器已启动，但 Hub 无法访问 ET Web，请核对地址、端口映射与访问策略后恢复");
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
        Ok(result)
    }.await;
    match result {
        Ok(result) => { state["phase"] = json!("awaiting_account"); state["container"] = result; }
        Err(error) => { state["phase"] = json!("interrupted"); state["last_error"] = json!(error.to_string()); save("web_deployment", &state).await?; return Err(error); }
    }
    save("web_deployment", &state).await?;
    Ok(state)
}

async fn network(request: &Request) -> Result<Value> {
    let c = &request.context;
    let key = "network_deployment";
    let mut state = sdk::kv_get(key).await?;
    if state.is_null() {
        let spec = specification(&request.body);
        let nodes = spec["nodes"].as_array().context("请选择网络成员")?;
        ensure!(!nodes.is_empty(), "至少选择一个网络成员");
        let ids = nodes.iter().map(|node| node.as_str().map(str::to_owned).context("节点编号无效")).collect::<Result<Vec<_>>>()?;
        ensure!(ids.iter().collect::<std::collections::HashSet<_>>().len() == ids.len(), "网络成员重复");
        host_nodes(&ids).await?;
        let cidr: ipnet::Ipv4Net = spec["cidr"].as_str().context("缺少网络网段")?.parse()?;
        ensure!((8..=30).contains(&cidr.prefix_len()) && cidr.addr() == cidr.network(), "请填写规范的 IPv4 网段，前缀范围为 8～30");
        ensure!(ids.len() as u64 + 2 <= 1u64 << (32 - cidr.prefix_len()), "网段没有足够的成员地址");
        let name = spec["name"].as_str().context("缺少网络名称")?;
        ensure!(!name.trim().is_empty() && name.len() <= 64, "网络名称长度无效");
        let endpoint = reqwest::Url::parse(spec["config_endpoint"].as_str().context("缺少客户端配置下发地址")?)?;
        ensure!(["tcp", "udp", "ws", "wss"].contains(&endpoint.scheme()) && endpoint.host_str().is_some(), "配置下发地址无效");
        let peers = spec["peer_urls"].as_array().context("缺少组网连接入口")?;
        ensure!(ids.len() == 1 || !peers.is_empty(), "多个节点需要明确填写相互可达的组网连接入口");
        for peer in peers { let url = reqwest::Url::parse(peer.as_str().context("组网入口无效")?)?; ensure!(["tcp", "udp", "ws", "wss"].contains(&url.scheme()) && url.host_str().is_some(), "组网入口无效"); }
        let listeners = spec["listener_urls"].as_array().context("缺少客户端监听入口")?;
        ensure!(!listeners.is_empty(), "至少填写一个客户端监听入口");
        for listener in listeners { let url = reqwest::Url::parse(listener.as_str().context("监听入口无效")?)?; ensure!(["tcp", "udp", "ws", "wss"].contains(&url.scheme()) && url.host_str().is_some(), "监听入口无效"); }
        let first = u32::from(cidr.network()) + 1;
        let members: Vec<Value> = ids.iter().enumerate().map(|(index, node)| json!({"node_id":node,"machine_id":uuid::Uuid::new_v4().to_string(),"instance_id":uuid::Uuid::new_v4().to_string(),"address":std::net::Ipv4Addr::from(first + index as u32).to_string(),"phase":"pending"})).collect();
        state = json!({"spec":spec,"members":members,"phase":"preparing"});
    } else {
        ensure!(request.body["resume"] == true, "首次组网已有记录，请核实后继续原任务");
    }
    let secret = sdk::secret(c, "network_secret")?;
    ensure!(secret.len() >= 16, "请先保存至少 16 位的组网密钥");
    state["task_id"] = json!(c.task_id);
    state["last_error"] = Value::Null;
    save(key, &state).await?;
    let result = run_network(c, key, &mut state, secret).await;
    match result {
        Ok(()) => state["phase"] = json!("ready"),
        Err(error) => { state["phase"] = json!("interrupted"); state["last_error"] = json!(error.to_string()); save(key, &state).await?; return Err(error); }
    }
    save(key, &state).await?;
    Ok(state)
}

async fn run_network(c: &Context, key: &str, state: &mut Value, secret: &str) -> Result<()> {
    let spec = state["spec"].clone();
    let cidr: ipnet::Ipv4Net = spec["cidr"].as_str().context("缺少网段")?.parse()?;
    let count = state["members"].as_array().context("成员记录无效")?.len();
    // 先验证账号及所有节点依赖，避免在最后一个节点才发现基础条件缺失。
    api(c, reqwest::Method::GET, "/machines", None).await?;
    for index in 0..count {
        let node = state["members"][index]["node_id"].as_str().context("成员编号无效")?;
        let runtime = sdk::node_task(c, node, "runtime.inspect", json!({}), &format!("runtime:{node}")).await?;
        ensure!(runtime["installed"] == true, "节点 {node} 缺少 Docker，请先到 Docker 页安装");
        ensure!(runtime["running"] == true, "节点 {node} 的 Docker 服务不可用");
        sdk::node_task(c, node, "network.preflight", json!({"cidr":spec["cidr"],"address":state["members"][index]["address"]}), &format!("preflight:{node}")).await?;
    }
    for index in 0..count {
        let member_state = state["members"][index].clone();
        let node = member_state["node_id"].as_str().context("缺少节点")?;
        let machine = member_state["machine_id"].as_str().context("缺少机器编号")?;
        let instance = member_state["instance_id"].as_str().context("缺少实例编号")?;
        sdk::node_task(c, node, "client.install", json!({"machine_id":machine,"config_endpoint":spec["config_endpoint"]}), &format!("client:{node}")).await?;
        state["members"][index]["phase"] = json!("client_ready");
        save(key, state).await?;
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(90);
        let directory = loop {
            match api(c, reqwest::Method::GET, &format!("/machines/{machine}/networks"), None).await {
                Ok(value) => break value,
                Err(error) => { ensure!(tokio::time::Instant::now() < deadline, "客户端没有连到所选 ET Web，请核对配置下发入口：{error}"); tokio::time::sleep(std::time::Duration::from_secs(2)).await; }
            }
        };
        let exists = ["running_inst_ids", "disabled_inst_ids"].iter().any(|field| directory[*field].as_array().is_some_and(|ids| ids.iter().any(|id| uuid(id).is_ok_and(|id| id == instance))));
        let config = json!({"instance_id":instance,"hostname":node,"network_name":spec["name"],"network_secret":secret,"dhcp":false,"virtual_ipv4":member_state["address"],"network_length":cidr.prefix_len(),"dev_name":"opsd-et","networking_method":1,"peer_urls":spec["peer_urls"],"listener_urls":spec["listener_urls"]});
        if exists {
            let actual = api(c, reqwest::Method::GET, &format!("/machines/{machine}/networks/config/{instance}"), None).await?;
            for field in ["instance_id", "network_name", "network_secret", "virtual_ipv4", "network_length", "peer_urls", "listener_urls", "dev_name"] {
                ensure!(actual[field] == config[field], "已存在的网络实例与部署记录不同，请核实，不自动覆盖");
            }
            // 用户主动停止的实例不在恢复时擅自重新启动。
        } else {
            let validation = api(c, reqwest::Method::POST, &format!("/machines/{machine}/validate-config"), Some(json!({"config":config}))).await?;
            ensure!(validation["error"].is_null(), "ET 拒绝所选网络配置");
            api(c, reqwest::Method::POST, &format!("/machines/{machine}/networks"), Some(json!({"config":config,"save":true}))).await?;
        }
        state["members"][index]["phase"] = json!("network_created");
        save(key, state).await?;
    }
    // 所有成员创建后再等实际接口、节点事实和网络连接收敛。
    for index in 0..count {
        let item = state["members"][index].clone();
        let node = item["node_id"].as_str().context("缺少节点")?;
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(90);
        loop {
            let result: Result<()> = async {
                let (overlay, _) = member(c, item["machine_id"].as_str().unwrap(), item["instance_id"].as_str().unwrap(), node).await?;
                ensure!(overlay["address"] == item["address"], "实际地址与部署计划不同");
                let mut directory = sdk::core("network.get", json!({})).await?;
                let overlays = directory["overlays"].as_array_mut().context("网络目录无效")?;
                ensure!(!overlays.iter().any(|old| old["node_id"] == node && old["instance_id"] != item["instance_id"]), "节点已关联其他网络实例，不能覆盖");
                overlays.retain(|old| old["node_id"] != node);
                overlays.push(overlay);
                sdk::core("network.put", directory).await?;
                Ok(())
            }.await;
            match result { Ok(()) => break, Err(error) => { ensure!(tokio::time::Instant::now() < deadline, "网络接口尚未完成实际关联：{error}"); tokio::time::sleep(std::time::Duration::from_secs(2)).await; } }
        }
        let peers: Vec<Value> = state["members"].as_array().unwrap().iter().filter(|peer| peer["node_id"] != node).map(|peer| peer["address"].clone()).collect();
        sdk::node_task(c, node, "network.verify", json!({"address":item["address"],"peers":peers}), &format!("verify:{node}")).await?;
        state["members"][index]["phase"] = json!("ready");
        save(key, state).await?;
    }
    Ok(())
}

pub async fn setup(context: &Context) -> Result<Value> {
    let web = sdk::kv_get("web_deployment").await?;
    let network = sdk::kv_get("network_deployment").await?;
    let configured = context.config["url"].as_str().is_some_and(|url| !url.is_empty())
        && context.config["username"].as_str().is_some_and(|name| !name.is_empty()) && context.secrets.contains_key("password");
    let (state, message) = if !configured {
        ("needs_configuration", "接入已有网络服务，或部署后完成账户设置并填写连接信息".to_owned())
    } else {
        match api(context, reqwest::Method::GET, "/machines", None).await {
            Ok(_) => ("ready", "网络管理服务已连接".to_owned()),
            Err(error) => ("unavailable", format!("已配置的网络服务暂不可用：{error}")),
        }
    };
    Ok(json!({"state":state,"message":message,"web":web,"network":network}))
}
