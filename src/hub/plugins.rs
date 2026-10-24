use super::*;
use anyhow::Context as _;
use axum::{
    Json,
    body::Bytes,
    extract::{Path, State as S},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use opsd_plugin_sdk::{Context, Manifest, Request};
use serde_json::{Value, json};
use std::path::{Component, Path as FilePath};

#[derive(Clone, Serialize, Deserialize)]
pub struct Installed {
    pub manifest: Manifest,
    pub enabled: bool,
    pub installed_at: i64,
}

pub struct Runtime {
    pub child: tokio::process::Child,
    pub socket: PathBuf,
}
pub type Runtimes = Arc<Mutex<HashMap<String, Runtime>>>;
pub type Tokens = Arc<RwLock<HashMap<String, (String, String)>>>;

pub fn copy_state(source: &FilePath, destination: &FilePath) -> Result<()> {
    fn copy(source: &FilePath, destination: &FilePath) -> Result<()> {
        if !source.exists() {
            return Ok(());
        }
        std::fs::create_dir_all(destination)?;
        for entry in std::fs::read_dir(source)? {
            let entry = entry?;
            let kind = entry.file_type()?;
            anyhow::ensure!(!kind.is_symlink(), "插件备份目录不能包含符号链接");
            let target = destination.join(entry.file_name());
            if kind.is_dir() {
                copy(&entry.path(), &target)?;
            } else {
                pki::private_write(&target, std::fs::read(entry.path())?)?;
                std::fs::set_permissions(target, entry.metadata()?.permissions())?;
            }
        }
        Ok(())
    }
    for folder in ["packages", "secrets"] {
        copy(&source.join(folder), &destination.join(folder))?;
    }
    Ok(())
}

fn key(plugin: &str, version: &str) -> ApiResult<String> {
    check(
        opsd_plugin_sdk::identifier(plugin) && opsd_plugin_sdk::identifier(version),
        "插件编号或版本无效",
    )?;
    Ok(format!("{plugin}@{version}"))
}
fn root(s: &State, plugin: &str, version: &str) -> ApiResult<PathBuf> {
    key(plugin, version)?;
    Ok(s.dir.join("plugins/packages").join(plugin).join(version))
}
pub async fn installed(s: &State, plugin: &str, version: &str) -> ApiResult<Installed> {
    s.db.get("plugins", &key(plugin, version)?)
        .await?
        .ok_or_else(|| bad("插件版本尚未安装"))
}
pub async fn active(s: &State, plugin: &str) -> ApiResult<Installed> {
    s.db.list::<Installed>("plugins")
        .await?
        .into_iter()
        .find(|p| p.manifest.id == plugin && p.enabled)
        .ok_or_else(|| bad("插件尚未启用"))
}
pub async fn list(S(s): S<State>) -> ApiResult<Json<Value>> {
    Ok(Json(
        json!({"plugins":s.db.list::<Installed>("plugins").await?,"api_version":1,"platform":format!("linux-{}",arch())}),
    ))
}
fn arch() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "amd64"
    }
}

pub async fn install(S(s): S<State>, body: Bytes) -> ApiResult<Json<Installed>> {
    let _management = s.plugin_management.lock().await;
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(body))?;
    let manifest: Manifest = serde_json::from_reader(zip.by_name("plugin.json")?)?;
    manifest.validate()?;
    let destination = root(&s, &manifest.id, &manifest.version)?;
    check(!destination.exists(), "插件版本已存在，不能覆盖")?;
    let mut total = 0u64;
    for index in 0..zip.len() {
        let entry = zip.by_index(index)?;
        check(
            entry.enclosed_name().is_some() && !entry.name().contains('\\'),
            "插件包路径无效",
        )?;
        check(
            entry.unix_mode().is_none_or(|m| m & 0o170000 != 0o120000),
            "插件包不能包含符号链接",
        )?;
        total = total
            .checked_add(entry.size())
            .ok_or_else(|| bad("插件包过大"))?;
        check(total <= 512 * 1024 * 1024, "插件解压大小超过限制")?;
    }
    let binary = format!("linux-{}/plugin", arch());
    check(zip.by_name(&binary).is_ok(), "插件包没有当前平台的程序")?;
    check(
        zip.by_name("api.json").is_ok(),
        "插件包缺少独立业务接口定义",
    )?;
    if let Some(page) = &manifest.page {
        check(
            zip.by_name(&format!("web/{page}")).is_ok(),
            "插件包缺少声明的页面",
        )?;
    }
    let result: Result<()> = async {
        std::fs::create_dir_all(&destination)?;
        for index in 0..zip.len() {
            let mut entry = zip.by_index(index)?;
            let path = destination.join(entry.enclosed_name().unwrap());
            if entry.is_dir() {
                std::fs::create_dir_all(&path)?;
                continue;
            }
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut file = std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&path)?;
            std::io::copy(&mut entry, &mut file)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(
                    &path,
                    std::fs::Permissions::from_mode(
                        if path.file_name().is_some_and(|n| n == "plugin") {
                            0o500
                        } else {
                            0o600
                        },
                    ),
                )?;
            }
        }
        Ok(())
    }
    .await;
    if let Err(error) = result {
        let _ = std::fs::remove_dir_all(&destination);
        return Err(error.into());
    }
    let record = Installed {
        manifest,
        enabled: false,
        installed_at: now(),
    };
    s.db.insert(
        "plugins",
        &key(&record.manifest.id, &record.manifest.version)?,
        &record,
    )
    .await?;
    Ok(Json(record))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    pub version: String,
    pub enabled: bool,
}
pub async fn select(
    S(s): S<State>,
    Path(plugin): Path<String>,
    Json(input): Json<Selection>,
) -> ApiResult<Json<Installed>> {
    let _management = s.plugin_management.lock().await;
    let mut record = installed(&s, &plugin, &input.version).await?;
    let all = s.db.list::<Installed>("plugins").await?;
    let previous: Vec<_> = all
        .iter()
        .filter(|p| p.manifest.id == plugin && p.enabled)
        .cloned()
        .collect();
    if input.enabled {
        for dependency in &record.manifest.dependencies {
            check(
                all.iter()
                    .any(|p| p.enabled && &p.manifest.id == dependency),
                &format!("缺少已启用依赖：{dependency}"),
            )?;
        }
        let mut graph: HashMap<String, Vec<String>> = all
            .iter()
            .filter(|p| p.enabled)
            .map(|p| (p.manifest.id.clone(), p.manifest.dependencies.clone()))
            .collect();
        graph.insert(plugin.clone(), record.manifest.dependencies.clone());
        fn acyclic(id: &str, graph: &HashMap<String, Vec<String>>, path: &mut Vec<String>) -> bool {
            if path.iter().any(|p| p == id) {
                return false;
            }
            path.push(id.into());
            let ok = graph
                .get(id)
                .is_none_or(|dependencies| dependencies.iter().all(|d| acyclic(d, graph, path)));
            path.pop();
            ok
        }
        check(
            acyclic(&plugin, &graph, &mut Vec::new()),
            "插件依赖形成循环",
        )?;
    } else {
        check(
            !all.iter()
                .any(|p| p.enabled && p.manifest.dependencies.contains(&plugin)),
            "仍有已启用插件依赖当前插件",
        )?;
    }
    if !input.enabled {
        check(
            !s.db.tasks().await?.iter().any(|t| {
                !t.status.terminal()
                    && matches!(&t.action,Action::Plugin{plugin:p,..} if p==&plugin)
            }),
            "插件仍有未结束任务",
        )?;
    }
    if input.enabled {
        let initialized = invoke(
            &s,
            &plugin,
            &record.manifest.version,
            "INIT",
            "",
            Value::Null,
            None,
        )
        .await?;
        check(initialized.status < 400, "插件初始化失败，保留原启用版本")?;
    }
    let _guard = s.gate.lock().await;
    for mut previous in all
        .into_iter()
        .filter(|p| p.manifest.id == plugin && p.enabled)
    {
        previous.enabled = false;
        s.db.put(
            "plugins",
            &key(&plugin, &previous.manifest.version)?,
            &previous,
        )
        .await?;
    }
    record.enabled = input.enabled;
    s.db.put("plugins", &key(&plugin, &input.version)?, &record)
        .await?;
    drop(_guard);
    for old in previous {
        if record.enabled && old.manifest.version == record.manifest.version {
            continue;
        }
        let _ = invoke(
            &s,
            &plugin,
            &old.manifest.version,
            "QUIESCE",
            "",
            Value::Null,
            None,
        )
        .await;
        if !record.enabled || old.manifest.version != record.manifest.version {
            let busy=s.db.tasks().await?.iter().any(|t|!t.status.terminal()&&(t.plugin_versions.get(&plugin)==Some(&old.manifest.version)||matches!(&t.action,Action::Plugin{plugin:p,version:v,..} if p==&plugin&&v==&old.manifest.version)));
            if !busy
                && let Some(mut runtime) = s
                    .plugin_runtimes
                    .lock()
                    .await
                    .remove(&key(&plugin, &old.manifest.version)?)
            {
                let _ = runtime.child.kill().await;
            }
        }
    }
    let _ = s.events.send(json!({"type":"plugins"}));
    Ok(Json(record))
}

pub async fn uninstall(
    S(s): S<State>,
    Path((plugin, version)): Path<(String, String)>,
) -> ApiResult<Json<Value>> {
    let _management = s.plugin_management.lock().await;
    let _guard = s.gate.lock().await;
    let record = installed(&s, &plugin, &version).await?;
    check(!record.enabled, "请先禁用插件再卸载")?;
    check(!s.db.tasks().await?.iter().any(|t|!t.status.terminal()&&(t.plugin_versions.get(&plugin)==Some(&version)||matches!(&t.action,Action::Plugin{plugin:p,version:v,..} if p==&plugin&&v==&version))),"插件版本仍有未结束任务或被活动任务依赖")?;
    if let Some(mut runtime) = s
        .plugin_runtimes
        .lock()
        .await
        .remove(&key(&plugin, &version)?)
    {
        let _ = runtime.child.kill().await;
    }
    s.plugin_tokens
        .write()
        .await
        .retain(|_, (p, v)| p != &plugin || v != &version);
    std::fs::remove_dir_all(root(&s, &plugin, &version)?)?;
    s.db.delete("plugins", &key(&plugin, &version)?).await?;
    Ok(Json(json!({"uninstalled":true,"data_retained":true})))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Configuration {
    pub config: Value,
    #[serde(default)]
    pub secrets: HashMap<String, String>,
}
fn secret_path(s: &State, plugin: &str) -> ApiResult<PathBuf> {
    check(opsd_plugin_sdk::identifier(plugin), "插件编号无效")?;
    Ok(s.dir.join("plugins/secrets").join(format!("{plugin}.json")))
}
fn secrets(s: &State, plugin: &str) -> ApiResult<HashMap<String, String>> {
    let path = secret_path(s, plugin)?;
    if !path.exists() {
        return Ok(HashMap::new());
    }
    Ok(serde_json::from_slice(&std::fs::read(path)?)?)
}
pub async fn configuration(S(s): S<State>, Path(plugin): Path<String>) -> ApiResult<Json<Value>> {
    Ok(Json(
        json!({"config":s.db.get::<Value>("plugin_config",&plugin).await?.unwrap_or(json!({})),"secret_names":secrets(&s,&plugin)?.keys().collect::<Vec<_>>()}),
    ))
}
pub async fn configure(
    S(s): S<State>,
    Path(plugin): Path<String>,
    Json(input): Json<Configuration>,
) -> ApiResult<Json<Value>> {
    let _management = s.plugin_management.lock().await;
    let record = active(&s, &plugin).await?;
    check(
        input.config.as_object().is_some_and(|config| {
            config
                .keys()
                .all(|name| record.manifest.config_keys.contains(name))
        }),
        "普通配置含未声明字段；凭据请写入秘密存储",
    )?;
    let _guard = s.gate.lock().await;
    let mut values = secrets(&s, &plugin)?;
    for (name, value) in input.secrets {
        check(opsd_plugin_sdk::identifier(&name), "秘密名称无效")?;
        if value.is_empty() {
            values.remove(&name);
        } else {
            values.insert(name, value);
        }
    }
    let path = secret_path(&s, &plugin)?;
    std::fs::create_dir_all(path.parent().unwrap())?;
    let temporary = path.with_extension(format!("{}.pending", id()));
    pki::private_write(&temporary, serde_json::to_vec(&values)?)?;
    std::fs::rename(temporary, path)?;
    s.db.put("plugin_config", &plugin, &input.config).await?;
    drop(_guard);
    let _ = invoke(
        &s,
        &plugin,
        &record.manifest.version,
        "INIT",
        "",
        Value::Null,
        None,
    )
    .await?;
    configuration(S(s.clone()), Path(plugin)).await
}

async fn runtime(s: &State, record: &Installed) -> ApiResult<PathBuf> {
    let runtime_key = key(&record.manifest.id, &record.manifest.version)?;
    let mut runtimes = s.plugin_runtimes.lock().await;
    if let Some(runtime) = runtimes.get_mut(&runtime_key)
        && runtime.child.try_wait()?.is_none()
    {
        return Ok(runtime.socket.clone());
    }
    let run = s.plugin_socket_dir.path();
    std::fs::create_dir_all(&run)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&run, std::fs::Permissions::from_mode(0o700))?;
    }
    if let Some(old) = runtimes.remove(&runtime_key) {
        let _ = std::fs::remove_file(old.socket);
    }
    let socket = run.join(format!("{}.sock", id()));
    let token = format!("{}{}", id(), id());
    s.plugin_tokens
        .write()
        .await
        .retain(|_, (plugin, version)| {
            plugin != &record.manifest.id || version != &record.manifest.version
        });
    s.plugin_tokens.write().await.insert(
        token.clone(),
        (record.manifest.id.clone(), record.manifest.version.clone()),
    );
    let child = opsd_plugin_sdk::process(
        root(s, &record.manifest.id, &record.manifest.version)?
            .join(format!("linux-{}/plugin", arch())),
    )
    .env_clear()
    .env(
        "PATH",
        "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
    )
    .env("OPSD_PLUGIN_SOCKET", &socket)
    .env(
        "OPSD_PLUGIN_CORE_SOCKET",
        s.plugin_socket_dir.path().join("core.sock"),
    )
    .env("OPSD_PLUGIN_CORE_TOKEN", token)
    .stdin(std::process::Stdio::null())
    .stdout(std::process::Stdio::null())
    .stderr(std::process::Stdio::null())
    .kill_on_drop(true)
    .spawn()?;
    runtimes.insert(
        runtime_key.clone(),
        Runtime {
            child,
            socket: socket.clone(),
        },
    );
    for _ in 0..50 {
        if socket.exists() {
            return Ok(socket);
        }
        if runtimes
            .get_mut(&runtime_key)
            .unwrap()
            .child
            .try_wait()?
            .is_some()
        {
            return Err(bad("插件进程启动失败"));
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    Err(bad("等待插件监听超时"))
}

pub async fn invoke(
    s: &State,
    plugin: &str,
    version: &str,
    method: &str,
    path: &str,
    body: Value,
    task_id: Option<String>,
) -> ApiResult<opsd_plugin_sdk::Response> {
    let record = installed(s, plugin, version).await?;
    let socket = runtime(s, &record).await?;
    let credentials = secrets(s, plugin)?;
    let mut config: Value =
        s.db.get("plugin_config", plugin)
            .await?
            .unwrap_or(json!({}));
    config["__ca_pem"] = json!(std::fs::read_to_string(s.dir.join("pki/ca.pem"))?);
    let dependencies = if let Some(id) = &task_id {
        s.db.task(id)
            .await?
            .ok_or_else(|| bad("任务不存在"))?
            .plugin_versions
    } else {
        HashMap::new()
    };
    let request = Request {
        context: Context {
            plugin: plugin.into(),
            version: version.into(),
            config,
            secrets: credentials.clone(),
            task_id,
            dependencies,
        },
        method: method.into(),
        path: path.into(),
        body,
    };
    let response =
        opsd_plugin_sdk::unix_request(&socket, "/invoke", None, &serde_json::to_value(request)?)
            .await?;
    check(response.status == 200, "插件进程通信失败")?;
    let mut result: opsd_plugin_sdk::Response = serde_json::from_value(response.body)?;
    opsd_plugin_sdk::scrub(&mut result.body, &credentials);
    Ok(result)
}

pub async fn gateway(
    S(s): S<State>,
    Path((plugin, path)): Path<(String, String)>,
    axum::Extension(peer): axum::Extension<transport::Peer>,
    method: axum::http::Method,
    body: Bytes,
) -> ApiResult<Response> {
    check(
        ["GET", "POST", "PUT", "DELETE"].contains(&method.as_str()),
        "插件请求方法无效",
    )?;
    let record = active(&s, &plugin).await?;
    let input = if body.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&body).map_err(|_| bad("插件请求必须是 JSON"))?
    };
    let response = invoke(
        &s,
        &plugin,
        &record.manifest.version,
        method.as_str(),
        &format!("/{path}"),
        input,
        None,
    )
    .await?;
    if response.status == 202 {
        let _ = audit::record(
            &s.db,
            audit::AuditEntry {
                actor: "管理员".into(),
                node_id: "@hub".into(),
                category: "插件".into(),
                target: format!("{plugin}@{} /{path}", record.manifest.version),
                result: "已受理".into(),
                detail: response.body["task_id"].as_str().unwrap_or_default().into(),
                source: peer.address.ip().to_string(),
                bytes: 0,
                duration: 0,
            },
        )
        .await;
        let _ = s.events.send(json!({"type":"task"}));
    }
    Ok((
        StatusCode::from_u16(response.status).map_err(|_| bad("插件响应状态无效"))?,
        Json(response.body),
    )
        .into_response())
}

#[derive(Deserialize)]
pub struct StreamQuery {
    csrf: String,
}
pub async fn stream(
    S(s): S<State>,
    Path((plugin, path)): Path<(String, String)>,
    axum::extract::Query(query): axum::extract::Query<StreamQuery>,
    axum::Extension(session): axum::Extension<auth::Session>,
    axum::Extension(peer): axum::Extension<transport::Peer>,
    headers: HeaderMap,
    ws: axum::extract::WebSocketUpgrade,
) -> ApiResult<Response> {
    check(
        query.csrf == session.csrf
            && headers.get("origin").and_then(|h| h.to_str().ok()) == Some(s.origin.as_str()),
        "插件流来源或 CSRF 无效",
    )?;
    let record = active(&s, &plugin).await?;
    let socket = runtime(&s, &record).await?;
    let mut config: Value =
        s.db.get("plugin_config", &plugin)
            .await?
            .unwrap_or(json!({}));
    config["__ca_pem"] = json!(std::fs::read_to_string(s.dir.join("pki/ca.pem"))?);
    let context = Context {
        plugin: plugin.clone(),
        version: record.manifest.version,
        config,
        secrets: secrets(&s, &plugin)?,
        task_id: None,
        dependencies: Default::default(),
    };
    let audit = audit::AuditEntry {
        actor: "管理员".into(),
        node_id: "@hub".into(),
        category: "插件会话".into(),
        target: format!("{plugin}/{path}"),
        result: "已开始".into(),
        detail: String::new(),
        source: peer.address.ip().to_string(),
        bytes: 0,
        duration: 0,
    };
    let _ = audit::record(&s.db, audit.clone()).await;
    Ok(ws.max_message_size(1024*1024).on_upgrade(move|mut browser|async move{
        use futures_util::{SinkExt,StreamExt};
        use axum::extract::ws::Message as Browser;
        use tokio_tungstenite::tungstenite::Message as Plugin;
        let started=now();
        let flow:Result<()>=async {
            #[cfg(unix)] let connection=tokio::net::UnixStream::connect(socket).await?;
            #[cfg(not(unix))] let connection=tokio::net::TcpStream::connect("127.0.0.1:0").await?;
            let (mut process,_)=tokio_tungstenite::client_async("ws://localhost/stream",connection).await?;
            let body=match tokio::time::timeout(std::time::Duration::from_secs(10),browser.recv()).await?.context("流初始化中断")?? {
                Browser::Text(text)=>serde_json::from_str(&text)?,_=>anyhow::bail!("缺少流初始化参数"),
            };
            process.send(Plugin::Text(serde_json::to_string(&Request{context,method:"STREAM".into(),path:format!("/{path}"),body})?.into())).await?;
            let expires=tokio::time::sleep(std::time::Duration::from_secs((session.expires-now()).max(0) as u64));tokio::pin!(expires);
            loop {tokio::select!{
                _=&mut expires=>break,
                message=browser.recv()=>match message{
                    Some(Ok(Browser::Text(text)))=>process.send(Plugin::Text(text.to_string().into())).await?,
                    Some(Ok(Browser::Binary(bytes)))=>process.send(Plugin::Binary(bytes)).await?,
                    Some(Ok(Browser::Ping(bytes)))=>browser.send(Browser::Pong(bytes)).await?,
                    Some(Ok(Browser::Pong(_)))=>{},_=>break,
                },
                message=process.next()=>match message{
                    Some(Ok(Plugin::Text(text)))=>browser.send(Browser::Text(text.to_string().into())).await?,
                    Some(Ok(Plugin::Binary(bytes)))=>browser.send(Browser::Binary(bytes)).await?,
                    Some(Ok(Plugin::Ping(bytes)))=>process.send(Plugin::Pong(bytes)).await?,
                    Some(Ok(Plugin::Pong(_)))=>{},_=>break,
                },
            }}
            let _=process.close(None).await;Ok(())
        }.await;
        if flow.is_err(){let _=browser.send(Browser::Text(json!({"error":"插件流已结束或不可用"}).to_string().into())).await;}
        let mut audit=audit;audit.result="已结束".into();audit.duration=now()-started;let _=audit::record(&s.db,audit).await;
        let _=browser.close().await;
    }).into_response())
}

pub async fn asset(
    S(s): S<State>,
    Path((plugin, path)): Path<(String, String)>,
) -> ApiResult<Response> {
    let record = active(&s, &plugin).await?;
    let path = if path.is_empty() { "index.html" } else { &path };
    check(
        !path.contains('\\')
            && FilePath::new(path)
                .components()
                .all(|c| matches!(c, Component::Normal(_))),
        "插件资源路径无效",
    )?;
    let bytes = std::fs::read(
        root(&s, &plugin, &record.manifest.version)?
            .join("web")
            .join(path),
    )?;
    let content_type = if path.ends_with(".html") {
        "text/html; charset=utf-8"
    } else if path.ends_with(".js") {
        "text/javascript; charset=utf-8"
    } else if path.ends_with(".css") {
        "text/css; charset=utf-8"
    } else {
        "application/octet-stream"
    };
    Ok((
        [
            ("content-type", content_type),
            ("cache-control", "no-store"),
        ],
        bytes,
    )
        .into_response())
}

pub async fn node_binary(
    S(s): S<State>,
    Path((plugin, version, arch)): Path<(String, String, String)>,
    axum::Extension(peer): axum::Extension<transport::Peer>,
) -> ApiResult<Response> {
    let binding = channel::identity(&s, &peer).await?;
    let record = installed(&s, &plugin, &version).await?;
    check(
        record.manifest.node && ["amd64", "arm64"].contains(&arch.as_str()),
        "插件不支持节点程序或架构",
    )?;
    check(s.db.tasks().await?.iter().any(|t|t.node_id==binding.node_id&&matches!(&t.action,Action::Plugin{plugin:p,version:v,..} if p==&plugin&&v==&version)),"节点没有对应插件任务")?;
    Ok((
        [
            ("content-type", "application/octet-stream"),
            ("cache-control", "no-store"),
        ],
        std::fs::read(root(&s, &plugin, &version)?.join(format!("linux-{arch}/plugin")))?,
    )
        .into_response())
}

pub async fn node_context(
    S(s): S<State>,
    Path(task_id): Path<String>,
    axum::Extension(peer): axum::Extension<transport::Peer>,
) -> ApiResult<Response> {
    let binding = channel::identity(&s, &peer).await?;
    let task =
        s.db.task(&task_id)
            .await?
            .ok_or_else(|| bad("任务不存在"))?;
    check(
        task.node_id == binding.node_id && !task.status.terminal(),
        "任务身份或执行状态不匹配",
    )?;
    let Action::Plugin {
        plugin, version, ..
    } = task.action
    else {
        return Err(bad("不是插件任务"));
    };
    let record = installed(&s, &plugin, &version).await?;
    let mut node_secrets = secrets(&s, &plugin)?;
    node_secrets.retain(|name, _| record.manifest.node_secrets.contains(name));
    let mut config: Value =
        s.db.get("plugin_config", &plugin)
            .await?
            .unwrap_or(json!({}));
    config["__ca_pem"] = json!(std::fs::read_to_string(s.dir.join("pki/ca.pem"))?);
    let context = Context {
        plugin: plugin.clone(),
        version,
        config,
        secrets: node_secrets,
        task_id: Some(task_id),
        dependencies: task.plugin_versions,
    };
    Ok(([("cache-control", "no-store")], Json(context)).into_response())
}

async fn submit(s: &State, caller: &str, version: &str, input: Value) -> ApiResult<Value> {
    let _guard = s.gate.lock().await;
    let plugin = input["plugin"]
        .as_str()
        .ok_or_else(|| bad("缺少插件编号"))?;
    let record = installed(s, caller, version).await?;
    check(
        plugin == caller || record.manifest.dependencies.iter().any(|d| d == plugin),
        "插件不能调用未声明的依赖",
    )?;
    let selected = active(s, plugin).await?;
    let selected_version = input["version"]
        .as_str()
        .unwrap_or(&selected.manifest.version);
    let selected = installed(s, plugin, selected_version).await?;
    let inherited = if let Some(id) = input["parent_task"].as_str() {
        let parent = s.db.task(id).await?.ok_or_else(|| bad("父任务不存在"))?;
        check(
            parent.status == TaskStatus::Running
                && matches!(&parent.action,Action::Plugin{plugin:p,version:v,..} if p==caller&&v==version),
            "父任务身份不匹配",
        )?;
        check(
            parent.plugin_versions.get(plugin) == Some(&selected.manifest.version),
            "子任务版本与父任务固定的依赖不一致",
        )?;
        Some(parent.plugin_versions)
    } else {
        check(record.enabled, "插件版本已停止受理新任务")?;
        None
    };
    let target = input["target"]
        .as_str()
        .ok_or_else(|| bad("缺少任务目标"))?;
    if target != "@hub" {
        let node =
            s.db.get::<Node>("nodes", target)
                .await?
                .filter(|n| !n.revoked)
                .ok_or_else(|| bad("节点不存在"))?;
        check(
            selected.manifest.node
                && node
                    .capabilities
                    .as_ref()
                    .is_some_and(|c| c.plugins && c.execution_mode == AgentInstallMode::Host),
            "节点不支持宿主机插件动作",
        )?;
    }
    let operation = input["operation"].as_str().ok_or_else(|| bad("缺少操作"))?;
    let task_key = input["key"].as_str().ok_or_else(|| bad("缺少幂等键"))?;
    check(!task_key.is_empty() && task_key.len() <= 128, "幂等键无效")?;
    let mut task = TaskEnvelope::new(
        target.into(),
        task_key.into(),
        Action::Plugin {
            plugin: plugin.into(),
            version: selected.manifest.version.clone(),
            operation: operation.into(),
            input: input["input"].clone(),
        },
    );
    task.plugin_versions = if let Some(versions) = inherited {
        versions
    } else {
        let mut versions = HashMap::new();
        let mut pending = vec![selected];
        while let Some(record) = pending.pop() {
            if versions.contains_key(&record.manifest.id) {
                continue;
            }
            versions.insert(record.manifest.id.clone(), record.manifest.version.clone());
            for dependency in record.manifest.dependencies {
                pending.push(active(s, &dependency).await?);
            }
        }
        versions
    };
    let task = s.db.accept_task(&task).await?;
    Ok(json!({"task_id":task.id,"status":task.status}))
}

#[derive(Deserialize)]
struct Call {
    operation: String,
    input: Value,
}
async fn core_call(
    S(s): S<State>,
    headers: HeaderMap,
    Json(call): Json<Call>,
) -> ApiResult<Json<Value>> {
    let token = headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .ok_or_else(|| bad("插件身份无效"))?;
    let (plugin, version) = s
        .plugin_tokens
        .read()
        .await
        .get(token)
        .cloned()
        .ok_or_else(|| bad("插件身份无效"))?;
    let record = installed(&s, &plugin, &version).await?;
    let result = match call.operation.as_str() {
        "task.submit" => submit(&s, &plugin, &version, call.input).await?,
        "task.get" => {
            let task =
                s.db.task(
                    call.input["id"]
                        .as_str()
                        .ok_or_else(|| bad("缺少任务编号"))?,
                )
                .await?
                .ok_or_else(|| bad("任务不存在"))?;
            check(
                matches!(&task.action,Action::Plugin{plugin:p,..} if p==&plugin||record.manifest.dependencies.contains(p)),
                "插件不能读取无关任务",
            )?;
            serde_json::to_value(task)?
        }
        "nodes" => serde_json::to_value(
            s.db.list::<Node>("nodes")
                .await?
                .into_iter()
                .filter(|n| !n.revoked)
                .collect::<Vec<_>>(),
        )?,
        "network.get" => serde_json::to_value(network::directory(&s).await?)?,
        "network.put" => {
            check(
                record
                    .manifest
                    .permissions
                    .iter()
                    .any(|p| p == "network.write"),
                "插件没有网络目录写权限",
            )?;
            serde_json::to_value(
                network::update(S(s.clone()), Json(serde_json::from_value(call.input)?))
                    .await?
                    .0,
            )?
        }
        "network.observe" => {
            check(
                record
                    .manifest
                    .permissions
                    .iter()
                    .any(|p| p == "network.write"),
                "插件没有地址观测写权限",
            )?;
            network::plugin_observation(&s, &plugin, call.input).await?
        }
        "certificate.sign" => {
            check(
                record
                    .manifest
                    .permissions
                    .iter()
                    .any(|p| p == "pki.server"),
                "插件没有服务证书权限",
            )?;
            let directory = network::directory(&s).await?;
            let node = call.input["node_id"]
                .as_str()
                .ok_or_else(|| bad("缺少证书目标节点"))?;
            let address = directory
                .overlays
                .iter()
                .find(|m| m.node_id == node)
                .ok_or_else(|| bad("节点未关联 ET 地址"))?
                .address
                .parse()
                .map_err(|_| bad("ET 地址无效"))?;
            let csr = call.input["csr"].as_str().ok_or_else(|| bad("缺少 CSR"))?;
            json!({"certificate":pki::sign_server_csr(&s.dir.join("pki"),csr,address)?,"ca":std::fs::read_to_string(s.dir.join("pki/ca.pem"))?})
        }
        "state.get" | "state.put" => {
            let name = call.input["key"]
                .as_str()
                .ok_or_else(|| bad("缺少状态名称"))?;
            check(name.len() <= 96, "状态名称过长")?;
            let bucket = format!("plugin:{plugin}");
            if call.operation == "state.put" {
                s.db.put(&bucket, name, &call.input["value"]).await?;
            }
            s.db.get::<Value>(&bucket, name)
                .await?
                .unwrap_or(Value::Null)
        }
        "plugin.call" => {
            let dependency = call.input["plugin"]
                .as_str()
                .ok_or_else(|| bad("缺少依赖插件"))?;
            check(
                record.manifest.dependencies.iter().any(|d| d == dependency),
                "插件没有声明该依赖",
            )?;
            if let Some(id) = call.input["task_id"].as_str() {
                let task = s.db.task(id).await?.ok_or_else(|| bad("调用任务不存在"))?;
                check(
                    task.status == TaskStatus::Running
                        && matches!(&task.action,Action::Plugin{plugin:p,version:v,..} if p==&plugin&&v==&version),
                    "跨插件写入的任务身份不匹配",
                )?;
            }
            let selected = if let Some(id) = call.input["task_id"].as_str() {
                let parent = s.db.task(id).await?.ok_or_else(|| bad("调用任务不存在"))?;
                installed(
                    &s,
                    dependency,
                    parent
                        .plugin_versions
                        .get(dependency)
                        .ok_or_else(|| bad("任务没有固定依赖版本"))?,
                )
                .await?
            } else {
                active(&s, dependency).await?
            };
            let response = Box::pin(invoke(
                &s,
                dependency,
                &selected.manifest.version,
                call.input["method"].as_str().unwrap_or("GET"),
                call.input["path"].as_str().unwrap_or("/"),
                call.input["body"].clone(),
                call.input["task_id"].as_str().map(str::to_owned),
            ))
            .await?;
            check(
                response.status < 400,
                response.body["error"]
                    .as_str()
                    .unwrap_or("依赖插件调用失败"),
            )?;
            response.body
        }
        _ => return Err(bad("未知的核心扩展操作")),
    };
    Ok(Json(result))
}

pub async fn start(s: State) -> Result<()> {
    for mut task in
        s.db.tasks()
            .await?
            .into_iter()
            .filter(|t| t.node_id == "@hub" && !t.status.terminal())
    {
        if matches!(task.status, TaskStatus::Running | TaskStatus::Validating) {
            task.status = TaskStatus::Uncertain;
            task.error = Some("主控在执行期间重启，请核实上游结果后恢复".into());
            task.sequence += 1;
            s.db.save_task(&task).await?;
        }
    }
    #[cfg(unix)]
    {
        let folder = s.dir.join("plugins");
        std::fs::create_dir_all(&folder)?;
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o700))?;
        let socket = s.plugin_socket_dir.path().join("core.sock");
        if socket.exists() {
            std::fs::remove_file(&socket)?;
        }
        let listener = tokio::net::UnixListener::bind(&socket)?;
        let rpc_state = s.clone();
        tokio::spawn(async move {
            let _ = axum::serve(
                listener,
                Router::new()
                    .route("/call", axum::routing::post(core_call))
                    .with_state(rpc_state),
            )
            .await;
        });
    }
    for record in
        s.db.list::<Installed>("plugins")
            .await?
            .into_iter()
            .filter(|p| p.enabled)
    {
        let state = s.clone();
        tokio::spawn(async move {
            let _ = invoke(
                &state,
                &record.manifest.id,
                &record.manifest.version,
                "INIT",
                "",
                Value::Null,
                None,
            )
            .await;
        });
    }
    tokio::spawn(async move {
        let mut running = std::collections::HashSet::new();
        let (done_tx, mut done_rx) = mpsc::channel::<String>(64);
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(1));
        loop {
            tokio::select! {
                Some(id)=done_rx.recv()=>{running.remove(&id);},
                _=tick.tick()=>{
                    let Ok(tasks)=s.db.tasks().await else{continue};
                    for mut task in tasks.into_iter().filter(|t|t.node_id=="@hub"&&matches!(t.status,TaskStatus::Pending|TaskStatus::Accepted)){
                        if running.len()>=3||!running.insert(task.id.clone()){continue}
                        task.status=TaskStatus::Running;task.sequence+=1;task.updated_at=now();
                        if s.db.save_task(&task).await.is_err(){running.remove(&task.id);continue}
                        let state=s.clone();let done=done_tx.clone();
                        tokio::spawn(async move{
                            let result=match &task.action {
                                Action::Plugin{plugin,version,operation,input}=>invoke(&state,plugin,version,"EXECUTE",operation,input.clone(),Some(task.id.clone())).await,
                                _=>Err(bad("主控只执行插件任务")),
                            };
                            match result {
                                Ok(response) if response.status<400=>{task.status=TaskStatus::Succeeded;task.result=Some(response.body);}
                                Ok(response)=>{task.status=if response.status==409{TaskStatus::Uncertain}else{TaskStatus::Failed};task.error=Some(response.body["error"].as_str().unwrap_or("插件执行失败").into());}
                                Err(error)=>{task.status=TaskStatus::Uncertain;task.error=Some(error.1);}
                            }
                            task.sequence+=1;task.updated_at=now();let _=state.db.save_task(&task).await;
                            if let Action::Plugin{plugin,version,operation,..}=&task.action{
                                let _=audit::record(&state.db,audit::AuditEntry{actor:"插件执行器".into(),node_id:"@hub".into(),category:"插件任务".into(),target:format!("{plugin}@{version} /{operation}"),result:format!("{:?}",task.status),detail:task.id.clone(),source:"本机插件进程".into(),bytes:0,duration:now()-task.created_at}).await;
                            }
                            let _=state.events.send(json!({"type":"task","task":task.redacted()}));let _=done.send(task.id).await;
                        });
                    }
                }
            }
        }
    });
    Ok(())
}
