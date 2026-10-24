mod bootstrap;
mod node;
use anyhow::{Context as _, Result, bail, ensure};
use opsd_plugin_sdk::{self as sdk, Context, Request, Response};
use serde_json::{Value, json};
use std::sync::OnceLock;

struct Session {
    url: String,
    username: String,
    password: String,
    client: reqwest::Client,
}
static SESSION: OnceLock<tokio::sync::Mutex<Option<Session>>> = OnceLock::new();
static CONTEXT: OnceLock<tokio::sync::Mutex<Option<Context>>> = OnceLock::new();
static PAUSED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
type Cache = std::collections::HashMap<String, (tokio::time::Instant, Value)>;
static CACHE: OnceLock<tokio::sync::Mutex<Cache>> = OnceLock::new();

async fn client(context: &Context, renew: bool) -> Result<(String, reqwest::Client)> {
    let url = context.config["url"]
        .as_str()
        .context("请先配置 ET Web 地址")?
        .trim_end_matches('/');
    let parsed = reqwest::Url::parse(url)?;
    ensure!(
        ["http", "https"].contains(&parsed.scheme())
            && parsed.host_str().is_some()
            && parsed.username().is_empty()
            && parsed.query().is_none(),
        "ET Web 地址无效"
    );
    let username = context.config["username"]
        .as_str()
        .context("缺少 ET Web 用户名")?;
    let password = sdk::secret(context, "password")?;
    let mut cached = SESSION
        .get_or_init(|| tokio::sync::Mutex::new(None))
        .lock()
        .await;
    if !renew
        && let Some(session) = cached.as_ref()
        && session.url == url
        && session.username == username
        && session.password == password
    {
        return Ok((url.into(), session.client.clone()));
    }
    let client = reqwest::Client::builder()
        .cookie_store(true)
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(30))
        .build()?;
    let response=client.post(format!("{url}/api/v1/auth/login")).json(&json!({"username":username,"password":format!("{:x}",md5::compute(password.as_bytes()))})).send().await?;
    ensure!(
        response.status().is_success(),
        "ET Web 登录失败：{}",
        response.status()
    );
    *cached = Some(Session {
        url: url.into(),
        username: username.into(),
        password: password.into(),
        client: client.clone(),
    });
    Ok((url.into(), client))
}

async fn api(
    context: &Context,
    method: reqwest::Method,
    path: &str,
    body: Option<Value>,
) -> Result<Value> {
    let cache_key = format!("{}:{path}", context.config["url"]);
    if method == reqwest::Method::GET
        && let Some((at, value)) = CACHE
            .get_or_init(Default::default)
            .lock()
            .await
            .get(&cache_key)
        && at.elapsed() < std::time::Duration::from_secs(10)
    {
        return Ok(value.clone());
    }
    for attempt in 0..2 {
        let (url, client) = client(context, attempt == 1).await?;
        let mut request = client.request(method.clone(), format!("{url}/api/v1{path}"));
        if let Some(body) = &body {
            request = request.json(body);
        }
        let response = match request.send().await {
            Ok(response) => response,
            Err(error) if method != reqwest::Method::GET => {
                return Err(sdk::uncertain(format!(
                    "ET 写入响应中断，需读取实际结果：{error}"
                )));
            }
            Err(error) => return Err(error.into()),
        };
        if response.status() == 401 && attempt == 0 {
            continue;
        }
        ensure!(
            response.status().is_success(),
            "ET Web 请求失败：{}",
            response.status()
        );
        let bytes = response.bytes().await?;
        let value = if bytes.is_empty() {
            json!({})
        } else {
            serde_json::from_slice(&bytes)?
        };
        let mut cache = CACHE.get_or_init(Default::default).lock().await;
        if method == reqwest::Method::GET {
            if cache.len() > 256 {
                cache.clear();
            }
            cache.insert(cache_key, (tokio::time::Instant::now(), value.clone()));
        } else {
            cache.clear();
        }
        return Ok(value);
    }
    bail!("ET Web 会话不可用")
}
fn uuid(value: &Value) -> Result<String> {
    if let Some(text) = value.as_str() {
        return Ok(uuid::Uuid::parse_str(text)?.to_string());
    }
    let mut number = 0u128;
    for part in ["part1", "part2", "part3", "part4"] {
        number = (number << 32) | value[part].as_u64().context("ET 机器编号形状无效")? as u128;
    }
    Ok(uuid::Uuid::from_u128(number).to_string())
}
fn ids(body: &Value) -> Result<(String, String)> {
    Ok((uuid(&body["machine_id"])?, uuid(&body["instance_id"])?))
}

async fn member(
    context: &Context,
    machine: &str,
    instance: &str,
    node: &str,
) -> Result<(Value, Value)> {
    let base = format!("/machines/{machine}/networks");
    let config = api(
        context,
        reqwest::Method::GET,
        &format!("{base}/config/{instance}"),
        None,
    )
    .await?;
    let info = api(
        context,
        reqwest::Method::GET,
        &format!("{base}/info/{instance}"),
        None,
    )
    .await?;
    let runtime = &info["info"]["map"][instance];
    ensure!(runtime["running"] == true, "ET 实例当前未运行");
    let ipv4 = &runtime["my_node_info"]["virtual_ipv4"];
    let address = if let Some(number) = ipv4["address"]["addr"].as_u64() {
        std::net::Ipv4Addr::from(u32::try_from(number)?).to_string()
    } else {
        config["virtual_ipv4"]
            .as_str()
            .filter(|s| !s.is_empty())
            .context("ET 尚未分配地址")?
            .to_owned()
    };
    let prefix = ipv4["network_length"]
        .as_u64()
        .or_else(|| config["network_length"].as_u64())
        .context("ET 未提供网段前缀")?;
    let interface = runtime["dev_name"]
        .as_str()
        .filter(|s| !s.is_empty())
        .or_else(|| config["dev_name"].as_str())
        .context("ET 未提供实际接口")?;
    Ok((
        json!({"node_id":node,"address":address,"prefix":prefix,"interface":interface,"provider":"et","machine_id":machine,"instance_id":instance}),
        runtime.clone(),
    ))
}

async fn refresh(context: &Context) -> Result<()> {
    let mut directory = sdk::core("network.get", json!({})).await?;
    let members = directory["overlays"]
        .as_array()
        .context("核心网络目录结构无效")?
        .clone();
    for old in members {
        let node = old["node_id"].as_str().context("节点编号无效")?;
        let (Ok(machine), Ok(instance)) = (uuid(&old["machine_id"]), uuid(&old["instance_id"]))
        else {
            continue;
        };
        let Ok((current, runtime)) = member(context, &machine, &instance, node).await else {
            continue;
        };
        if current != old {
            let mut updated = directory.clone();
            if let Some(entries) = updated["overlays"].as_array_mut() {
                for item in entries.iter_mut().filter(|m| m["node_id"] == node) {
                    *item = current.clone();
                }
            }
            if let Ok(result) = sdk::core("network.put", updated).await {
                directory = result;
            }
        }
        let stun = &runtime["my_node_info"]["stun_info"];
        let _=sdk::core("network.observe",json!({"node_id":node,"machine_id":machine,"instance_id":instance,"addresses":stun["public_ip"].as_array().cloned().unwrap_or_default(),"source_at":stun["last_update_time"]})).await;
    }
    Ok(())
}

async fn handle(request: Request) -> Result<Response> {
    let context = &request.context;
    if request.method == "QUIESCE" {
        PAUSED.store(true, std::sync::atomic::Ordering::Relaxed);
        return Ok(Response::ok(json!({})));
    }
    *CONTEXT.get_or_init(Default::default).lock().await = Some(context.clone());
    if request.method == "INIT" {
        PAUSED.store(false, std::sync::atomic::Ordering::Relaxed);
        return Ok(Response::ok(json!({})));
    }
    if request.method == "EXECUTE" {
        if ["web.install", "network.create"].contains(&request.path.as_str()) {
            return Ok(Response::ok(bootstrap::execute(&request).await?));
        }
        let (machine, instance) = ids(&request.body)?;
        let base = format!("/machines/{machine}/networks");
        match request.path.as_str() {
            "config.update" => {
                let mut config = api(
                    context,
                    reqwest::Method::GET,
                    &format!("{base}/config/{instance}"),
                    None,
                )
                .await?;
                let patch = request.body["patch"].as_object().context("缺少配置变更")?;
                ensure!(patch.len() <= 64, "配置变更字段过多");
                let object = config.as_object_mut().context("ET Web 配置结构无效")?;
                for (key, value) in patch {
                    ensure!(
                        object.contains_key(key)
                            && !["instance_id", "network_secret", "credential_file"]
                                .contains(&key.as_str()),
                        "字段不能通过此操作修改：{key}"
                    );
                    object.insert(key.clone(), value.clone());
                }
                let validation = api(
                    context,
                    reqwest::Method::POST,
                    &format!("/machines/{machine}/validate-config"),
                    Some(json!({"config":config})),
                )
                .await?;
                ensure!(
                    validation.get("error").is_none_or(|e| e.is_null()),
                    "ET 配置校验失败"
                );
                api(
                    context,
                    reqwest::Method::PUT,
                    &format!("{base}/config/{instance}"),
                    Some(json!({"config":config})),
                )
                .await?;
                let actual = api(
                    context,
                    reqwest::Method::GET,
                    &format!("{base}/config/{instance}"),
                    None,
                )
                .await?;
                ensure!(
                    patch
                        .iter()
                        .all(|(key, value)| actual.get(key) == Some(value)),
                    "ET 保存后的配置与变更不一致"
                );
                Ok(Response::ok(
                    json!({"machine_id":machine,"instance_id":instance,"updated":true}),
                ))
            }
            "instance.state" => {
                let disabled = request.body["disabled"]
                    .as_bool()
                    .context("缺少实例启停状态")?;
                api(
                    context,
                    reqwest::Method::PUT,
                    &format!("{base}/{instance}"),
                    Some(json!({"disabled":disabled})),
                )
                .await?;
                let state = api(context, reqwest::Method::GET, &base, None).await?;
                let field = if disabled {
                    "disabled_inst_ids"
                } else {
                    "running_inst_ids"
                };
                ensure!(
                    state[field].as_array().is_some_and(|items| items
                        .iter()
                        .any(|v| uuid(v).ok().as_deref() == Some(&instance))),
                    "ET 实例没有达到目标状态"
                );
                Ok(Response::ok(json!({"disabled":disabled,"verified":true})))
            }
            "node.bind" => {
                let node = request.body["node_id"].as_str().context("缺少 OPSD 节点")?;
                let (member, _) = member(context, &machine, &instance, node).await?;
                let mut directory = sdk::core("network.get", json!({})).await?;
                let overlays = directory["overlays"]
                    .as_array_mut()
                    .context("核心网络目录结构无效")?;
                overlays.retain(|v| v["node_id"] != node);
                overlays.push(member);
                let directory = sdk::core("network.put", directory).await?;
                Ok(Response::ok(directory))
            }
            _ => bail!("未知 ET 任务操作"),
        }
    } else {
        match (request.method.as_str(), request.path.as_str()) {
            ("GET", "/setup") => Ok(Response::ok(bootstrap::setup(context).await?)),
            ("POST", "/web/install") => sdk::job(&request, "web.install").await,
            ("POST", "/network/create") => sdk::job(&request, "network.create").await,
            ("GET", "/machines") => {
                let mut data = api(context, reqwest::Method::GET, "/machines", None).await?;
                if let Some(machines) = data["machines"].as_array_mut() {
                    for machine in machines {
                        if let Ok(id) = uuid(&machine["info"]["machine_id"]) {
                            machine["machine_id"] = json!(id);
                        }
                    }
                }
                Ok(Response::ok(data))
            }
            ("POST", "/instances") => {
                let machine = uuid(&request.body["machine_id"])?;
                Ok(Response::ok(
                    api(
                        context,
                        reqwest::Method::GET,
                        &format!("/machines/{machine}/networks"),
                        None,
                    )
                    .await?,
                ))
            }
            ("POST", "/info") | ("POST", "/config") => {
                let (machine, instance) = ids(&request.body)?;
                let suffix = if request.path == "/info" {
                    "info"
                } else {
                    "config"
                };
                Ok(Response::ok(
                    api(
                        context,
                        reqwest::Method::GET,
                        &format!("/machines/{machine}/networks/{suffix}/{instance}"),
                        None,
                    )
                    .await?,
                ))
            }
            ("POST", "/config/update") => sdk::job(&request, "config.update").await,
            ("POST", "/instance/state") => sdk::job(&request, "instance.state").await,
            ("POST", "/bind") => sdk::job(&request, "node.bind").await,
            ("GET", "/console") => Ok(Response::ok(json!({"url":context.config["url"]}))),
            _ => bail!("未知 ET 接口"),
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    sdk::initialize();
    if std::env::args().nth(1).as_deref() == Some("node") {
        let request = sdk::node_input().await?;
        let response = match node::execute(request).await {
            Ok(body) => Response::ok(body),
            Err(error) => sdk::failure(error),
        };
        println!("{}", serde_json::to_string(&response)?);
        return Ok(());
    }
    tokio::spawn(async {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
        loop {
            interval.tick().await;
            if PAUSED.load(std::sync::atomic::Ordering::Relaxed) {
                continue;
            }
            let context = CONTEXT.get_or_init(Default::default).lock().await.clone();
            if let Some(context) = context {
                let _ = refresh(&context).await;
            }
        }
    });
    sdk::serve(handle).await
}
