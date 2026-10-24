mod node;
mod readiness;
use anyhow::{Context as _, Result, bail, ensure};
use opsd_plugin_sdk::{self as sdk, Context, Request, Response};
use serde_json::{Value, json};

async fn docker(
    context: &Context,
    node: &str,
    method: &str,
    path: &str,
    body: Option<Value>,
) -> Result<Value> {
    let mut input = json!({"node_id":node,"method":method,"path":path});
    if let Some(body) = body {
        input["body"] = body;
    }
    sdk::core("plugin.call",json!({"plugin":"docker","method":"INTERNAL","path":"/engine","body":input,"task_id":context.task_id})).await
}

fn name(value: &Value) -> Result<&str> {
    let name = value.as_str().context("缺少名称")?;
    ensure!(sdk::identifier(name), "名称无效");
    Ok(name)
}
fn s3_name(value: &Value) -> Result<&str> {
    let value = value.as_str().context("缺少名称")?;
    ensure!(
        !value.is_empty()
            && value.len() <= 63
            && value
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"-.".contains(&b)),
        "桶或用户名称无效"
    );
    Ok(value)
}

async fn install(request: &Request) -> Result<Value> {
    let c = &request.context;
    let cluster = name(&request.body["name"])?;
    let state_key = format!("cluster:{cluster}");
    let previous = sdk::kv_get(&state_key).await?;
    ensure!(
        previous.is_null() || request.body["resume"] == true,
        "集群已登记，请使用恢复或接管流程"
    );
    let mut state = if previous.is_null() {
        json!({"name":cluster,"spec":request.body,"phase":"preparing","prepared":{},"containers":{}})
    } else {
        previous
    };
    let spec = state["spec"].clone();
    let members = spec["members"].as_array().context("缺少集群节点")?;
    ensure!(members.len() >= 4, "分布式 Silo 首批部署至少选择四个节点");
    let disk_count = members[0]["storage"]
        .as_array()
        .context("节点没有选择数据目录")?
        .len();
    let mut ids = std::collections::HashSet::new();
    for member in members {
        ensure!(ids.insert(name(&member["node_id"])?), "集群节点重复");
        ensure!(
            member["storage"]
                .as_array()
                .is_some_and(|v| !v.is_empty() && v.len() == disk_count),
            "各节点必须选择相同数量的数据目录"
        );
    }
    let image = spec["image"]
        .as_str()
        .unwrap_or("pgsty/silo:RELEASE.2026-10-18T00-00-00Z");
    ensure!(
        image.starts_with("pgsty/silo:RELEASE.")
            && image
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"/.:_-".contains(&b)),
        "Silo 必须使用官方固定发行镜像"
    );
    let directory = sdk::core("network.get", json!({})).await?;
    let overlays = directory["overlays"].as_array().context("ET 目录无效")?;
    let user = sdk::secret(c, "root_user")?;
    let password = sdk::secret(c, "root_password")?;
    ensure!(
        user.len() >= 3 && password.len() >= 16,
        "请先配置 Silo 管理用户与密码"
    );
    // 先让所有节点完成只读预检，再开始任何磁盘准备。
    let mut engines = std::collections::HashSet::new();
    for member in members {
        let node = name(&member["node_id"])?;
        let overlay = overlays
            .iter()
            .find(|m| m["node_id"] == node)
            .context("集群节点未关联 ET 地址")?;
        let engine = docker(c, node, "GET", "/info", None).await?;
        let engine_id = engine["ID"]
            .as_str()
            .context("没有取得 Docker Engine 身份")?;
        ensure!(
            !engine_id.is_empty() && engines.insert(engine_id.to_owned()),
            "集群节点必须使用不同的 Docker Engine"
        );
        sdk::node_task(
            c,
            node,
            "storage.validate",
            json!({"cluster":cluster,"selections":member["storage"],"address":overlay["address"]}),
            &format!("validate:{node}"),
        )
        .await?;
    }
    sdk::kv_put(&state_key, state.clone()).await?;
    for member in members {
        let node = name(&member["node_id"])?;
        ensure!(
            overlays.iter().any(|m| m["node_id"] == node),
            "集群节点未关联 ET 地址"
        );
        if state["prepared"].get(node).is_none() {
            let address = &overlays.iter().find(|m| m["node_id"] == node).unwrap()["address"];
            let prepared=sdk::node_task(c,node,"storage.prepare",json!({"cluster":cluster,"selections":member["storage"],"confirmed":spec["acknowledge_destructive"],"address":address}),&format!("prepare:{node}")).await?;
            state["prepared"][node] = prepared;
            sdk::kv_put(&state_key, state.clone()).await?;
        } else {
            sdk::node_task(
                c,
                node,
                "storage.verify",
                state["prepared"][node].clone(),
                &format!("verify:{node}"),
            )
            .await?;
        }
        let certificate = sdk::service_certificate(c, node, &format!("tls:{node}")).await?;
        let installed = sdk::node_task(
            c,
            node,
            "tls.install",
            certificate,
            &format!("tls-install:{node}"),
        )
        .await?;
        state["prepared"][node]["tls_directory"] = installed["directory"].clone();
        sdk::kv_put(&state_key, state.clone()).await?;
    }
    let mut endpoints = Vec::new();
    for member in members {
        let node = name(&member["node_id"])?;
        let address = overlays.iter().find(|m| m["node_id"] == node).unwrap()["address"]
            .as_str()
            .context("ET 地址无效")?;
        let mounts = state["prepared"][node]["mounts"]
            .as_array()
            .context("缺少已准备目录")?;
        for index in 0..mounts.len() {
            endpoints.push(format!(
                "https://{}:9000/data/disk{}",
                sdk::address_host(address)?,
                index + 1
            ));
        }
    }
    state["phase"] = json!("deploying");
    sdk::kv_put(&state_key, state.clone()).await?;
    for member in members {
        let node = name(&member["node_id"])?;
        let address = overlays.iter().find(|m| m["node_id"] == node).unwrap()["address"]
            .as_str()
            .unwrap();
        let container_name = format!("opsd-silo-{cluster}");
        let mut binds = Vec::new();
        for (index, mount) in state["prepared"][node]["mounts"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
        {
            binds.push(format!(
                "{}:/data/disk{}",
                mount.as_str().context("挂载路径无效")?,
                index + 1
            ));
        }
        binds.push(format!(
            "{}:/certs:ro",
            state["prepared"][node]["tls_directory"]
                .as_str()
                .context("证书目录无效")?
        ));
        let mut command = vec!["--certs-dir".to_owned(), "/certs".into(), "server".into()];
        command.extend(endpoints.clone());
        command.extend(["--console-address".into(), ":9001".into()]);
        let existing = docker(c, node, "GET", "/containers/json?all=1", None).await?;
        let container = if let Some(old) = existing.as_array().and_then(|a| {
            a.iter().find(|v| {
                v["Names"]
                    .as_array()
                    .is_some_and(|n| n.iter().any(|n| n == &json!(format!("/{container_name}"))))
            })
        }) {
            ensure!(
                old["Labels"]["io.opsd.plugin"] == "silo"
                    && old["Labels"]["io.opsd.cluster"] == cluster,
                "同名容器不属于本集群，不能接管"
            );
            let id = old["Id"].as_str().context("容器编号无效")?;
            let actual = docker(c, node, "GET", &format!("/containers/{id}/json"), None).await?;
            ensure!(
                actual["Config"]["Image"] == image
                    && actual["Config"]["Cmd"] == json!(command)
                    && actual["HostConfig"]["Binds"] == json!(binds),
                "已有集群容器的镜像、拓扑或挂载不同，停止恢复"
            );
            for (port, host) in [("9000/tcp", "9000"), ("9001/tcp", "9001")] {
                ensure!(
                    actual["HostConfig"]["PortBindings"][port]
                        == json!([{"HostIp":address,"HostPort":host}]),
                    "已有集群端口绑定不同，停止恢复"
                );
            }
            id.to_owned()
        } else {
            let body = json!({"Image":image,"Cmd":command,"Env":[format!("MINIO_ROOT_USER={user}"),format!("MINIO_ROOT_PASSWORD={password}")],
                "Labels":{"io.opsd.plugin":"silo","io.opsd.cluster":cluster,"io.opsd.node":node},
                "ExposedPorts":{"9000/tcp":{},"9001/tcp":{}},"HostConfig":{"Binds":binds,"RestartPolicy":{"Name":"unless-stopped"},
                    "PortBindings":{"9000/tcp":[{"HostIp":address,"HostPort":"9000"}],"9001/tcp":[{"HostIp":address,"HostPort":"9001"}]}}});
            // 拉取固定镜像；输出由 Docker 插件判断错误，不将提交当成完成。
            let query =
                reqwest::Url::parse_with_params("http://localhost/", [("fromImage", image)])?
                    .query()
                    .unwrap()
                    .to_owned();
            let pulled = docker(c, node, "POST", &format!("/images/create?{query}"), None).await?;
            if let Some(text) = pulled["text"].as_str() {
                for line in text.lines() {
                    if let Ok(value) = serde_json::from_str::<Value>(line) {
                        ensure!(value["error"].is_null(), "Silo 镜像拉取失败");
                    }
                }
            }
            let result = docker(
                c,
                node,
                "POST",
                &format!("/containers/create?name={container_name}"),
                Some(body),
            )
            .await?;
            result["Id"]
                .as_str()
                .context("未取得 Silo 容器编号")?
                .to_owned()
        };
        docker(
            c,
            node,
            "POST",
            &format!("/containers/{container}/start"),
            Some(json!({})),
        )
        .await?;
        state["containers"][node] = json!(container);
        sdk::kv_put(&state_key, state.clone()).await?;
    }
    state["phase"] = json!("converging");
    sdk::kv_put(&state_key, state.clone()).await?;
    let client = reqwest::Client::builder()
        .add_root_certificate(reqwest::Certificate::from_pem(
            c.config["__ca_pem"].as_str().context("缺少 CA")?.as_bytes(),
        )?)
        .timeout(std::time::Duration::from_secs(5))
        .build()?;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(180);
    loop {
        let mut ready = true;
        for member in members {
            let node = name(&member["node_id"])?;
            let address = overlays.iter().find(|m| m["node_id"] == node).unwrap()["address"]
                .as_str()
                .unwrap();
            ready &= client
                .get(format!(
                    "https://{}:9000/minio/health/cluster",
                    sdk::address_host(address)?
                ))
                .send()
                .await
                .is_ok_and(|r| r.status().is_success());
        }
        if ready {
            break;
        }
        ensure!(
            tokio::time::Instant::now() < deadline,
            "集群未在期限内收敛，已保留部署进度，请核实后恢复"
        );
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
    state["phase"] = json!("ready");
    let first = name(&members[0]["node_id"])?;
    let address = overlays.iter().find(|m| m["node_id"] == first).unwrap()["address"]
        .as_str()
        .unwrap();
    state["console_url"] = json!(format!("https://{}:9001", sdk::address_host(address)?));
    state["s3_url"] = json!(format!("https://{}:9000", sdk::address_host(address)?));
    sdk::kv_put(&state_key, state.clone()).await?;
    Ok(state)
}

async fn mcli(context: &Context, cluster: &Value, args: Vec<String>) -> Result<Value> {
    let node = cluster["spec"]["members"][0]["node_id"]
        .as_str()
        .context("缺少集群成员")?;
    let container = cluster["containers"][node]
        .as_str()
        .context("缺少集群容器")?;
    let directory = sdk::core("network.get", json!({})).await?;
    let address = directory["overlays"]
        .as_array()
        .and_then(|a| a.iter().find(|m| m["node_id"] == node))
        .and_then(|m| m["address"].as_str())
        .context("节点 ET 地址未知")?;
    let mut url = reqwest::Url::parse(&format!("https://{}:9000", sdk::address_host(address)?))?;
    url.set_username(sdk::secret(context, "root_user")?)
        .map_err(|_| anyhow::anyhow!("管理用户名无法编码"))?;
    url.set_password(Some(sdk::secret(context, "root_password")?))
        .map_err(|_| anyhow::anyhow!("管理密码无法编码"))?;
    let mut command = sdk::strings(&["mcli", "--certs-dir", "/certs", "--json"]);
    command.extend(args);
    let exec=docker(context,node,"POST",&format!("/containers/{container}/exec"),Some(json!({"AttachStdout":true,"AttachStderr":true,"Tty":true,"Cmd":command,"Env":[format!("MC_HOST_opsd={url}")]}))).await?;
    let id = exec["Id"].as_str().context("缺少命令编号")?;
    let output = docker(
        context,
        node,
        "POST",
        &format!("/exec/{id}/start"),
        Some(json!({"Detach":false,"Tty":true})),
    )
    .await?;
    let actual = docker(context, node, "GET", &format!("/exec/{id}/json"), None).await?;
    ensure!(
        actual["Running"] == false && actual["ExitCode"] == 0,
        "Silo 管理命令未成功执行"
    );
    let text = if let Some(text) = output["text"].as_str() {
        text.to_owned()
    } else {
        ensure!(output["status"] != "error", "Silo 管理程序报告失败");
        return Ok(output);
    };
    let rows: Vec<Value> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(serde_json::from_str)
        .collect::<std::result::Result<_, _>>()?;
    ensure!(
        !rows.iter().any(|r| r["status"] == "error"),
        "Silo 管理程序报告失败"
    );
    Ok(if rows.len() == 1 {
        rows[0].clone()
    } else {
        json!(rows)
    })
}

async fn handle(request: Request) -> Result<Response> {
    if ["INIT", "QUIESCE"].contains(&request.method.as_str()) {
        return Ok(Response::ok(json!({})));
    }
    let c = &request.context;
    if request.method == "EXECUTE" {
        if request.path == "install" {
            return match install(&request).await {
                Ok(state) => Ok(Response::ok(state)),
                Err(error) => {
                    if let Some(cluster) = request.body["name"].as_str() {
                        let key = format!("cluster:{cluster}");
                        if let Ok(mut state) = sdk::kv_get(&key).await
                            && state.is_object()
                        {
                            state["last_error"] = json!(error.to_string());
                            let _ = sdk::kv_put(&key, state).await;
                        }
                    }
                    Err(error)
                }
            };
        }
        if request.path == "inventory" {
            let node = name(&request.body["node_id"])?;
            let inventory = sdk::node_task(
                c,
                node,
                "inventory",
                json!({}),
                &format!("inventory:{node}"),
            )
            .await?;
            sdk::kv_put(&format!("inventory:{node}"), inventory.clone()).await?;
            return Ok(Response::ok(inventory));
        }

        let cluster = name(&request.body["cluster"])?;
        let state = sdk::kv_get(&format!("cluster:{cluster}")).await?;
        ensure!(state["phase"] == "ready", "集群尚未就绪");
        let operation = request.body["operation"].as_str().unwrap_or("");
        let result = match request.path.as_str() {
            "bucket" => {
                let target = if operation == "list" {
                    "opsd".into()
                } else {
                    format!("opsd/{}", s3_name(&request.body["bucket"])?)
                };
                let args = match operation {
                    "create" => vec!["mb".into(), target],
                    "remove" => vec!["rb".into(), target],
                    "list" => vec!["ls".into(), "opsd".into()],
                    _ => bail!("未知桶操作"),
                };
                mcli(c, &state, args).await?
            }
            "user" => {
                let user = if operation == "list" {
                    ""
                } else {
                    s3_name(&request.body["user"])?
                };
                match operation {
                    "create" => {
                        let secret = sdk::secret(c, name(&request.body["secret_name"])?)?;
                        ensure!(secret.len() >= 16, "用户密钥过短");
                        mcli(
                            c,
                            &state,
                            vec![
                                "admin".into(),
                                "user".into(),
                                "add".into(),
                                "opsd".into(),
                                user.into(),
                                secret.into(),
                            ],
                        )
                        .await?;
                        let policy = request.body["policy"].as_str().unwrap_or("readonly");
                        ensure!(
                            ["readonly", "readwrite", "writeonly", "diagnostics"].contains(&policy),
                            "策略无效"
                        );
                        mcli(
                            c,
                            &state,
                            sdk::strings(&[
                                "admin", "policy", "attach", "opsd", policy, "--user", user,
                            ]),
                        )
                        .await?
                    }
                    "remove" => {
                        mcli(
                            c,
                            &state,
                            sdk::strings(&["admin", "user", "remove", "opsd", user]),
                        )
                        .await?
                    }
                    "list" => {
                        mcli(c, &state, sdk::strings(&["admin", "user", "list", "opsd"])).await?
                    }
                    _ => bail!("未知用户操作"),
                }
            }
            "status" => mcli(c, &state, sdk::strings(&["admin", "info", "opsd"])).await?,
            _ => bail!("未知 Silo 任务"),
        };
        return Ok(Response::ok(result));
    }
    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/setup") => {
            let configured = c.secrets.contains_key("root_user") && c.secrets.contains_key("root_password");
            let directory = sdk::core("network.get", json!({})).await?;
            let (state, message) = if !configured {
                ("needs_configuration", "请先配置存储管理凭据".to_owned())
            } else if directory["overlays"].as_array().is_none_or(|items| items.is_empty()) {
                ("needs_network", "请先在网络页完成节点组网与关联".to_owned())
            } else {
                match sdk::core("plugin.call",json!({"plugin":"docker","method":"GET","path":"/setup","body":{}})).await {
                    Ok(value) if value["state"] == "ready" => ("ready", "可选择成员部署或管理现有集群".to_owned()),
                    Ok(value) => ("needs_docker", value["message"].as_str().unwrap_or("请先接入 Docker 管理环境").to_owned()),
                    Err(error) => ("unavailable", error.to_string()),
                }
            };
            Ok(Response::ok(json!({"state":state,"message":message})))
        }
        ("GET", "/readiness") => {
            let nodes: Vec<opsd::protocol::Node> =
                serde_json::from_value(sdk::core("nodes", json!({})).await?)?;
            let at = opsd::protocol::now();
            let online = nodes
                .iter()
                .filter(|node| node.last_seen > at - 45)
                .map(|node| node.id.clone())
                .collect();
            let mut inventories = std::collections::HashMap::new();
            for node in &nodes {
                let inventory = sdk::kv_get(&format!("inventory:{}", node.id)).await?;
                if !inventory.is_null() {
                    inventories.insert(node.id.clone(), serde_json::from_value(inventory)?);
                }
            }
            Ok(Response::ok(serde_json::to_value(readiness::analyze(
                &nodes,
                &online,
                &inventories,
                at,
            ))?))
        }
        ("GET", "/clusters") => {
            let index = sdk::kv_get("clusters")
                .await?
                .as_array()
                .cloned()
                .unwrap_or_default();
            let mut clusters = Vec::new();
            for name in index {
                if let Some(name) = name.as_str() {
                    clusters.push(sdk::kv_get(&format!("cluster:{name}")).await?);
                }
            }
            Ok(Response::ok(json!({"clusters":clusters})))
        }
        ("POST", "/install") => {
            let cluster = name(&request.body["name"])?;
            let mut names = sdk::kv_get("clusters")
                .await?
                .as_array()
                .cloned()
                .unwrap_or_default();
            if !names.iter().any(|n| n == cluster) {
                names.push(json!(cluster));
                sdk::kv_put("clusters", json!(names)).await?;
            }
            sdk::job(&request, "install").await
        }
        ("POST", "/inventory") => sdk::job(&request, "inventory").await,
        ("POST", "/bucket") => sdk::job(&request, "bucket").await,
        ("POST", "/user") => sdk::job(&request, "user").await,
        ("POST", "/status") => sdk::job(&request, "status").await,
        _ => bail!("未知 Silo 接口"),
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
    sdk::serve(handle).await
}
