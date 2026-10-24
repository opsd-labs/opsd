mod node;
use anyhow::{Context as _, Result, bail, ensure};
use opsd_plugin_sdk::{self as sdk, Context, Request, Response};
use serde_json::{Value, json};
use std::sync::OnceLock;

struct Session {
    url: String,
    credential: String,
    client: reqwest::Client,
    jwt: String,
}
static SESSION: OnceLock<tokio::sync::Mutex<Option<Session>>> = OnceLock::new();

async fn base(context: &Context) -> Result<String> {
    let directory = sdk::core("network.get", json!({})).await?;
    let controller = directory["controller_node_id"]
        .as_str()
        .context("请先手动接入 Agent 并指定 Hub 宿主节点")?;
    let address = directory["overlays"]
        .as_array()
        .context("缺少 ET 目录")?
        .iter()
        .find(|m| m["node_id"] == controller)
        .and_then(|m| m["address"].as_str())
        .context("控制机尚未关联 ET 地址")?;
    let _ = context;
    Ok(format!(
        "https://{}:9443",
        if address.contains(':') {
            format!("[{address}]")
        } else {
            address.into()
        }
    ))
}

async fn connection(
    context: &Context,
    renew: bool,
) -> Result<(String, reqwest::Client, String, bool)> {
    let url = base(context).await?;
    let key = context.secrets.get("api_key");
    let credential = if let Some(key) = key {
        key.clone()
    } else {
        sdk::secret(context, "admin_password")?.to_owned()
    };
    let mut cached = SESSION
        .get_or_init(|| tokio::sync::Mutex::new(None))
        .lock()
        .await;
    if !renew
        && let Some(session) = cached.as_ref()
        && session.url == url
        && session.credential == credential
    {
        return Ok((
            url,
            session.client.clone(),
            if key.is_some() {
                credential
            } else {
                session.jwt.clone()
            },
            key.is_some(),
        ));
    }
    let mut builder = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(120));
    let ca = context
        .secrets
        .get("ca_pem")
        .map(String::as_str)
        .or_else(|| context.config["__ca_pem"].as_str())
        .context("缺少 Portainer CA")?;
    builder = builder.add_root_certificate(reqwest::Certificate::from_pem(ca.as_bytes())?);
    let client = builder.build()?;
    let jwt = if key.is_none() {
        let response=client.post(format!("{url}/api/auth")).json(&json!({"Username":context.config["username"].as_str().unwrap_or("admin"),"Password":credential})).send().await?;
        ensure!(
            response.status().is_success(),
            "Portainer 登录失败：{}",
            response.status()
        );
        response.json::<Value>().await?["jwt"]
            .as_str()
            .context("缺少 Portainer 会话")?
            .into()
    } else {
        String::new()
    };
    *cached = Some(Session {
        url: url.clone(),
        credential: credential.clone(),
        client: client.clone(),
        jwt: jwt.clone(),
    });
    Ok((
        url,
        client,
        if key.is_some() { credential } else { jwt },
        key.is_some(),
    ))
}

async fn api(
    context: &Context,
    method: reqwest::Method,
    path: &str,
    body: Option<Value>,
) -> Result<Value> {
    for attempt in 0..2 {
        let (url, client, token, key) = connection(context, attempt == 1).await?;
        let mut request = client.request(method.clone(), format!("{url}/api{path}"));
        request = if key {
            request.header("X-API-Key", token)
        } else {
            request.bearer_auth(token)
        };
        if let Some(body) = &body {
            request = request.json(body);
        }
        let response = match request.send().await {
            Ok(response) => response,
            Err(error) if method != reqwest::Method::GET => {
                return Err(sdk::uncertain(format!(
                    "Portainer 写入响应中断，需查询实际结果：{error}"
                )));
            }
            Err(error) => return Err(error.into()),
        };
        if response.status() == 401 && attempt == 0 {
            continue;
        }
        if response.status().is_server_error() && method != reqwest::Method::GET {
            return Err(sdk::uncertain(format!(
                "Portainer 写入返回 {}，需查询实际结果",
                response.status()
            )));
        }
        ensure!(
            response.status().is_success(),
            "Portainer 请求失败：{}",
            response.status()
        );
        let bytes = response.bytes().await?;
        ensure!(bytes.len() <= 16 * 1024 * 1024, "Portainer 响应超过限制");
        return if bytes.is_empty() {
            Ok(json!({}))
        } else {
            match serde_json::from_slice(&bytes) {
                Ok(value) => Ok(value),
                Err(_) => Ok(json!({"text":stream_text(&bytes)})),
            }
        };
    }
    bail!("Portainer 会话不可用")
}

async fn endpoint(node: &str) -> Result<u64> {
    let bindings = sdk::kv_get("environments").await?;
    bindings
        .as_array()
        .context("请先在 Docker 插件接入节点")?
        .iter()
        .find(|b| b["node_id"] == node)
        .and_then(|b| b["endpoint_id"].as_u64())
        .context("节点尚未绑定 Portainer 环境")
}
fn component(text: &str) -> Result<&str> {
    ensure!(
        !text.is_empty()
            && text.len() <= 256
            && text
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b)),
        "资源编号无效"
    );
    Ok(text)
}
async fn engine(
    context: &Context,
    node: &str,
    method: reqwest::Method,
    path: &str,
    body: Option<Value>,
) -> Result<Value> {
    api(
        context,
        method,
        &format!("/endpoints/{}/docker{path}", endpoint(node).await?),
        body,
    )
    .await
}

fn stream_text(bytes: &[u8]) -> String {
    let mut offset = 0;
    let mut result = Vec::new();
    while offset + 8 <= bytes.len()
        && [1, 2].contains(&bytes[offset])
        && bytes[offset + 1..offset + 4] == [0, 0, 0]
    {
        let length = u32::from_be_bytes(bytes[offset + 4..offset + 8].try_into().unwrap()) as usize;
        if offset + 8 + length > bytes.len() {
            return String::from_utf8_lossy(bytes).into();
        }
        result.extend_from_slice(&bytes[offset + 8..offset + 8 + length]);
        offset += 8 + length;
    }
    if offset == bytes.len() {
        String::from_utf8_lossy(&result).into()
    } else {
        String::from_utf8_lossy(bytes).into()
    }
}

async fn terminal(request: Request, mut browser: sdk::WebSocket) -> Result<()> {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::{Message, client::IntoClientRequest};
    let flow:Result<()>=async {
        ensure!(request.path=="/terminal","未知流操作");
        let node=request.body["node_id"].as_str().context("缺少节点")?;
        let container=component(request.body["container"].as_str().context("缺少容器")?)?;
        let actual=engine(&request.context,node,reqwest::Method::GET,&format!("/containers/{container}/json"),None).await?;
        ensure!(actual["Config"]["Labels"]["io.opsd.plugin"].is_null(),"插件基础服务含受保护凭据，请通过对应插件维护");
        let command=request.body["command"].as_array().context("终端命令必须是参数数组")?;
        ensure!(!command.is_empty()&&command.len()<=32&&command.iter().all(Value::is_string),"终端命令无效");
        let exec=engine(&request.context,node,reqwest::Method::POST,&format!("/containers/{container}/exec"),Some(json!({"AttachStdin":true,"AttachStdout":true,"AttachStderr":true,"Tty":true,"Cmd":command}))).await?;
        let id=component(exec["Id"].as_str().context("Portainer 没有返回终端编号")?)?;
        let (base,_,token,key)=connection(&request.context,false).await?;
        let url=format!("{}/api/websocket/exec?endpointId={}&id={id}",base.replacen("https://","wss://",1),endpoint(node).await?);
        let mut upgrade=url.into_client_request()?;
        upgrade.headers_mut().insert(if key{"X-API-Key"}else{"Authorization"},if key{token}else{format!("Bearer {token}")}.parse()?);
        let ca=request.context.secrets.get("ca_pem").map(String::as_str).or_else(||request.context.config["__ca_pem"].as_str()).context("缺少 CA")?;
        let mut roots=rustls::RootCertStore::empty();
        for certificate in rustls_pemfile::certs(&mut std::io::Cursor::new(ca.as_bytes())){roots.add(certificate?)?;}
        let tls=rustls::ClientConfig::builder().with_root_certificates(roots).with_no_client_auth();
        let (mut upstream,_)=tokio_tungstenite::connect_async_tls_with_config(upgrade,None,false,Some(tokio_tungstenite::Connector::Rustls(std::sync::Arc::new(tls)))).await?;
        browser.send(sdk::Message::Text(json!({"type":"ready","exec_id":id}).to_string().into())).await?;
        loop{tokio::select!{
            message=browser.recv()=>match message{
                Some(Ok(sdk::Message::Text(text)))=>upstream.send(Message::Binary(text.as_bytes().to_vec().into())).await?,
                Some(Ok(sdk::Message::Binary(bytes)))=>upstream.send(Message::Binary(bytes)).await?,
                Some(Ok(sdk::Message::Ping(bytes)))=>browser.send(sdk::Message::Pong(bytes)).await?,
                Some(Ok(sdk::Message::Pong(_)))=>{},_=>break,
            },
            message=upstream.next()=>match message{
                Some(Ok(Message::Binary(bytes)))=>browser.send(sdk::Message::Binary(bytes)).await?,
                Some(Ok(Message::Text(text)))=>browser.send(sdk::Message::Text(text.to_string().into())).await?,
                Some(Ok(Message::Ping(bytes)))=>upstream.send(Message::Pong(bytes)).await?,
                Some(Ok(Message::Pong(_)))=>{},_=>break,
            },
        }}
        let _=upstream.close(None).await;Ok(())
    }.await;
    if let Err(error) = flow {
        browser
            .send(sdk::Message::Text(
                json!({"error":error.to_string()}).to_string().into(),
            ))
            .await?;
    }
    Ok(())
}

async fn register(context: &Context, node: &str, address: &str, local: bool) -> Result<Value> {
    let name = format!("opsd-{node}");
    let endpoints = api(context, reqwest::Method::GET, "/endpoints", None).await?;
    let url = if local {
        "unix:///var/run/docker.sock".to_owned()
    } else {
        format!("tcp://{}:9002", sdk::address_host(address)?)
    };
    let record = if let Some(existing) = endpoints
        .as_array()
        .and_then(|a| a.iter().find(|e| e["Name"] == name))
    {
        ensure!(
            existing["URL"] == url,
            "同名 Portainer 环境地址不同，不能自动覆盖"
        );
        existing.clone()
    } else {
        let (base, client, token, key) = connection(context, false).await?;
        let mut form = reqwest::multipart::Form::new()
            .text("Name", name)
            .text("EndpointCreationType", if local { "1" } else { "2" })
            .text("URL", url);
        if !local {
            // 传统 Portainer Agent 使用其原生自签名协议与 AGENT_SECRET，不改变主控或 Server 的证书校验。
            form = form
                .text("TLS", "true")
                .text("TLSSkipVerify", "true")
                .text("TLSSkipClientVerify", "true");
        }
        let request = client.post(format!("{base}/api/endpoints")).multipart(form);
        let response = if key {
            request.header("X-API-Key", token)
        } else {
            request.bearer_auth(token)
        }
        .send()
        .await?;
        ensure!(
            response.status().is_success(),
            "Portainer 环境登记失败：{}",
            response.status()
        );
        response.json::<Value>().await?
    };
    let endpoint_id = record["Id"].as_u64().context("Portainer 环境编号无效")?;
    let mut bindings = sdk::kv_get("environments")
        .await?
        .as_array()
        .cloned()
        .unwrap_or_default();
    bindings.retain(|b| b["node_id"] != node);
    bindings.push(json!({"node_id":node,"endpoint_id":endpoint_id,"address":address}));
    sdk::kv_put("environments", json!(bindings)).await?;
    Ok(record)
}

async fn install(request: &Request) -> Result<Value> {
    let context = &request.context;
    let directory = sdk::core("network.get", json!({})).await?;
    let controller = directory["controller_node_id"]
        .as_str()
        .context("尚未指定控制机节点")?;
    let members = directory["overlays"].as_array().context("ET 目录无效")?;
    let host = members
        .iter()
        .find(|m| m["node_id"] == controller)
        .context("控制机尚未关联 ET")?;
    ensure!(
        sdk::secret(context, "agent_secret")?.len() >= 16
            && sdk::secret(context, "admin_password")?.len() >= 12,
        "请先配置足够强度的管理密码与 Agent 密钥"
    );
    let tls = sdk::service_certificate(context, controller, "server").await?;
    sdk::node_task(context,controller,"server.install",json!({"address":host["address"],"certificate":tls["certificate"],
        "allow_engine_install":request.body["allow_engine_install"],"image":context.config["server_image"]}),"server:install").await?;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        match connection(context, true).await {
            Ok(_) => break,
            Err(error) => {
                ensure!(
                    tokio::time::Instant::now() < deadline,
                    "Portainer 安装后登录失败：{error}"
                );
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
        }
    }
    let address = host["address"].as_str().context("控制机 ET 地址无效")?;
    register(context, controller, address, true).await?;
    let mut added = vec![controller.to_owned()];
    for node in request.body["nodes"]
        .as_array()
        .context("缺少待接入节点列表")?
    {
        let node = node.as_str().context("节点编号无效")?;
        if node == controller {
            continue;
        }
        let member = members
            .iter()
            .find(|m| m["node_id"] == node)
            .context("节点尚未关联 ET")?;
        sdk::node_task(context,node,"agent.install",json!({"address":member["address"],"allow_engine_install":request.body["allow_engine_install"],"image":context.config["agent_image"]}),&format!("agent:{node}")).await?;
        register(
            context,
            node,
            member["address"].as_str().context("ET 地址无效")?,
            false,
        )
        .await?;
        added.push(node.into());
    }
    Ok(json!({"nodes":added,"url":base(context).await?}))
}

async fn mutate(request: &Request) -> Result<Value> {
    let c = &request.context;
    let b = &request.body;
    let node = b["node_id"].as_str().context("缺少节点")?;
    let resource = b["resource"].as_str().context("缺少资源类型")?;
    let operation = b["operation"].as_str().context("缺少操作")?;
    let id = b["id"].as_str().unwrap_or("");
    match (resource, operation) {
        ("container", "start" | "stop" | "restart") => {
            component(id)?;
            let before = engine(
                c,
                node,
                reqwest::Method::GET,
                &format!("/containers/{id}/json"),
                None,
            )
            .await?;
            ensure!(
                before["Config"]["Labels"]["io.opsd.plugin"].is_null(),
                "插件基础服务请通过对应插件维护"
            );
            engine(
                c,
                node,
                reqwest::Method::POST,
                &format!("/containers/{id}/{operation}?t=20"),
                Some(json!({})),
            )
            .await?;
            let after = engine(
                c,
                node,
                reqwest::Method::GET,
                &format!("/containers/{id}/json"),
                None,
            )
            .await?;
            ensure!(
                after["Id"] == before["Id"] && after["Mounts"] == before["Mounts"],
                "容器身份或挂载发生变化"
            );
            ensure!(
                after["State"]["Running"].as_bool() == Some(operation != "stop"),
                "容器没有达到目标状态"
            );
            Ok(json!({"id":after["Id"],"state":after["State"]}))
        }
        ("container", "create") => {
            let name = component(b["name"].as_str().context("缺少容器名称")?)?;
            let mut spec = b["spec"].clone();
            let labels = spec["Labels"].as_object().cloned().unwrap_or_default();
            let mut labels = labels;
            labels.insert("io.opsd.task".into(), json!(c.task_id));
            spec["Labels"] = json!(labels);
            let existing = engine(
                c,
                node,
                reqwest::Method::GET,
                "/containers/json?all=1",
                None,
            )
            .await?;
            if let Some(old) = existing.as_array().and_then(|a| {
                a.iter()
                    .find(|v| v["Labels"]["io.opsd.task"] == json!(c.task_id))
            }) {
                return Ok(json!({"Id":old["Id"],"reused":true}));
            }
            engine(
                c,
                node,
                reqwest::Method::POST,
                &format!("/containers/create?name={name}"),
                Some(spec),
            )
            .await
        }
        ("container", "remove") => {
            component(id)?;
            ensure!(b["confirmed"] == true, "删除容器需要明确确认");
            let before = engine(
                c,
                node,
                reqwest::Method::GET,
                &format!("/containers/{id}/json"),
                None,
            )
            .await?;
            ensure!(
                before["Config"]["Labels"]["io.opsd.plugin"].is_null(),
                "插件基础服务请通过对应插件维护"
            );
            ensure!(before["State"]["Running"] == false, "请先停止容器再删除");
            engine(
                c,
                node,
                reqwest::Method::DELETE,
                &format!("/containers/{id}?v=false&force=false"),
                None,
            )
            .await
        }
        ("image", "pull") => {
            let image = b["reference"].as_str().context("缺少镜像引用")?;
            let query =
                reqwest::Url::parse_with_params("http://localhost/", [("fromImage", image)])?
                    .query()
                    .unwrap()
                    .to_owned();
            let result = engine(
                c,
                node,
                reqwest::Method::POST,
                &format!("/images/create?{query}"),
                None,
            )
            .await?;
            if let Some(text) = result["text"].as_str() {
                for line in text.lines() {
                    if let Ok(row) = serde_json::from_str::<Value>(line) {
                        ensure!(row["error"].is_null(), "镜像拉取失败");
                    }
                }
            }
            Ok(json!({"reference":image,"pulled":true}))
        }
        ("network", "create") => {
            engine(
                c,
                node,
                reqwest::Method::POST,
                "/networks/create",
                Some(b["spec"].clone()),
            )
            .await
        }
        ("volume", "create") => {
            engine(
                c,
                node,
                reqwest::Method::POST,
                "/volumes/create",
                Some(b["spec"].clone()),
            )
            .await
        }
        ("image" | "network" | "volume", "remove") => {
            component(id)?;
            let path = match resource {
                "image" => "images",
                "network" => "networks",
                _ => "volumes",
            };
            engine(
                c,
                node,
                reqwest::Method::DELETE,
                &format!("/{path}/{id}"),
                None,
            )
            .await
        }
        ("stack", "create" | "update" | "start" | "stop" | "remove") => {
            let endpoint = endpoint(node).await?;
            let (method, path, body) = match operation {
                "create" => (
                    reqwest::Method::POST,
                    format!("/stacks/create/standalone/string?endpointId={endpoint}"),
                    Some(
                        json!({"Name":component(b["name"].as_str().context("缺少 Stack 名称")?)?,"StackFileContent":b["content"],"Env":b["env"].as_array().cloned().unwrap_or_default()}),
                    ),
                ),
                "update" => (
                    reqwest::Method::PUT,
                    format!(
                        "/stacks/{}?endpointId={endpoint}",
                        b["stack_id"].as_u64().context("Stack 编号无效")?
                    ),
                    Some(
                        json!({"StackFileContent":b["content"],"Env":b["env"].as_array().cloned().unwrap_or_default(),"Prune":false,"PullImage":false}),
                    ),
                ),
                "remove" => (
                    reqwest::Method::DELETE,
                    format!(
                        "/stacks/{}?endpointId={endpoint}",
                        b["stack_id"].as_u64().context("Stack 编号无效")?
                    ),
                    None,
                ),
                _ => (
                    reqwest::Method::POST,
                    format!(
                        "/stacks/{}/{operation}?endpointId={endpoint}",
                        b["stack_id"].as_u64().context("Stack 编号无效")?
                    ),
                    None,
                ),
            };
            api(c, method, &path, body).await
        }
        _ => bail!("不支持的 Docker 操作"),
    }
}

async fn handle(request: Request) -> Result<Response> {
    if ["INIT", "QUIESCE"].contains(&request.method.as_str()) {
        return Ok(Response::ok(json!({})));
    }
    if request.method == "EXECUTE" {
        return Ok(Response::ok(match request.path.as_str() {
            "engine.inspect" | "engine.install" => {
                let node = request.body["node_id"].as_str().context("缺少节点")?;
                let value = sdk::node_task(&request.context, node, &request.path, request.body.clone(), &request.path).await?;
                sdk::kv_put(&format!("engine:{node}"), value.clone()).await?;
                value
            }
            "install" => install(&request).await?,
            "mutate" => mutate(&request).await?,
            _ => bail!("未知 Docker 任务"),
        }));
    }
    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/setup") => {
            let mut engines = serde_json::Map::new();
            let nodes = sdk::core("nodes", json!({})).await?;
            for node in nodes.as_array().context("节点列表无效")? {
                if let Some(id) = node["id"].as_str() {
                    let value = sdk::kv_get(&format!("engine:{id}")).await?;
                    if !value.is_null() { engines.insert(id.into(), value); }
                }
            }
            let bindings = sdk::kv_get("environments").await?;
            let configured = request.context.secrets.contains_key("api_key") || request.context.secrets.contains_key("admin_password");
            let (state, message) = if !configured {
                ("needs_configuration", "请配置 Docker 管理服务的凭据".to_owned())
            } else if let Err(error) = base(&request.context).await {
                ("needs_network", error.to_string())
            } else if bindings.as_array().is_none_or(|items| items.is_empty()) {
                ("needs_installation", "基础环境可单独安装；网络就绪后接入管理服务".to_owned())
            } else {
                match api(&request.context, reqwest::Method::GET, "/endpoints", None).await {
                    Ok(_) => ("ready", "Docker 管理服务已连接".to_owned()),
                    Err(error) => ("unavailable", format!("已接入的管理服务暂不可用：{error}")),
                }
            };
            Ok(Response::ok(json!({"state":state,"message":message,"engines":engines,"bindings":bindings})))
        }
        ("POST", "/engine/inspect") => sdk::job(&request, "engine.inspect").await,
        ("POST", "/engine/install") => sdk::job(&request, "engine.install").await,
        ("POST", "/install") => sdk::job(&request, "install").await,
        ("POST", "/mutate") => sdk::job(&request, "mutate").await,
        ("GET", "/console") => Ok(Response::ok(json!({"url":base(&request.context).await?}))),
        ("GET", "/environments") => Ok(Response::ok(
            json!({"bindings":sdk::kv_get("environments").await?,"environments":api(&request.context,reqwest::Method::GET,"/endpoints",None).await?}),
        )),
        ("POST", "/logs") => {
            let node = request.body["node_id"].as_str().context("缺少节点")?;
            let id = component(request.body["container"].as_str().context("缺少容器")?)?;
            let output = engine(
                &request.context,
                node,
                reqwest::Method::GET,
                &format!("/containers/{id}/logs?stdout=1&stderr=1&tail=300&timestamps=1"),
                None,
            )
            .await?;
            Ok(Response::ok(
                json!({"text":output["text"].as_str().unwrap_or("")}),
            ))
        }
        ("POST", "/terminal/resize") => {
            let node = request.body["node_id"].as_str().context("缺少节点")?;
            let id = component(request.body["exec_id"].as_str().context("缺少终端编号")?)?;
            let height = request.body["rows"]
                .as_u64()
                .filter(|n| (1..=500).contains(n))
                .context("终端高度无效")?;
            let width = request.body["cols"]
                .as_u64()
                .filter(|n| (1..=500).contains(n))
                .context("终端宽度无效")?;
            Ok(Response::ok(
                engine(
                    &request.context,
                    node,
                    reqwest::Method::POST,
                    &format!("/exec/{id}/resize?h={height}&w={width}"),
                    None,
                )
                .await?,
            ))
        }
        ("POST", "/detail") => {
            let node = request.body["node_id"].as_str().context("缺少节点")?;
            let resource = request.body["resource"].as_str().context("缺少资源类型")?;
            let id = component(request.body["id"].as_str().context("缺少资源编号")?)?;
            let path = match resource {
                "container" => format!("/containers/{id}/json"),
                "stack" => format!("/stacks/{id}/file"),
                _ => bail!("未知详情类型"),
            };
            let output = if resource == "stack" {
                api(&request.context, reqwest::Method::GET, &path, None).await?
            } else {
                engine(&request.context, node, reqwest::Method::GET, &path, None).await?
            };
            Ok(Response::ok(output))
        }
        ("POST", "/inventory") => {
            let node = request.body["node_id"].as_str().context("缺少节点")?;
            let c = &request.context;
            let (containers, images, networks, volumes, stacks) = tokio::try_join!(
                engine(
                    c,
                    node,
                    reqwest::Method::GET,
                    "/containers/json?all=1",
                    None
                ),
                engine(c, node, reqwest::Method::GET, "/images/json", None),
                engine(c, node, reqwest::Method::GET, "/networks", None),
                engine(c, node, reqwest::Method::GET, "/volumes", None),
                api(c, reqwest::Method::GET, "/stacks", None)
            )?;
            let endpoint = endpoint(node).await?;
            let stacks: Vec<_> = stacks
                .as_array()
                .into_iter()
                .flatten()
                .filter(|s| s["EndpointId"] == endpoint)
                .cloned()
                .collect();
            Ok(Response::ok(
                json!({"containers":containers,"images":images,"networks":networks,"volumes":volumes,"stacks":stacks}),
            ))
        }
        ("INTERNAL", "/engine") => {
            // 仅供依赖插件使用固定 Docker API；浏览器也受同一管理员权限保护。
            let node = request.body["node_id"].as_str().context("缺少节点")?;
            let path = request.body["path"]
                .as_str()
                .context("缺少 Docker API 路径")?;
            ensure!(
                path.starts_with('/') && !path.contains("..") && !path.contains('#'),
                "Docker API 路径无效"
            );
            let method = reqwest::Method::from_bytes(
                request.body["method"].as_str().unwrap_or("GET").as_bytes(),
            )?;
            ensure!(
                method == reqwest::Method::GET || request.context.task_id.is_some(),
                "底层写操作必须属于持久化任务"
            );
            Ok(Response::ok(
                engine(
                    &request.context,
                    node,
                    method,
                    path,
                    request.body.get("body").cloned(),
                )
                .await?,
            ))
        }
        _ => bail!("未知 Docker 接口"),
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
    sdk::serve_stream(handle, terminal).await
}
