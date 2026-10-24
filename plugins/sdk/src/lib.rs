use anyhow::{Context as _, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::HashMap, future::Future, path::Path};

pub const API_VERSION: u32 = 1;

pub fn initialize() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub api_version: u32,
    pub id: String,
    pub version: String,
    pub name: String,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub node: bool,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub node_secrets: Vec<String>,
    #[serde(default)]
    pub config_keys: Vec<String>,
    #[serde(default)]
    pub configuration_help: String,
    #[serde(default)]
    pub page: Option<String>,
}

pub fn identifier(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 64
        && text.as_bytes()[0].is_ascii_alphanumeric()
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        && !text.contains("..")
}

impl Manifest {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.api_version == API_VERSION, "插件接口版本不兼容");
        ensure!(
            identifier(&self.id) && identifier(&self.version),
            "插件编号或版本无效"
        );
        ensure!(!self.dependencies.contains(&self.id), "插件不能依赖自身");
        ensure!(
            self.dependencies.iter().all(|d| identifier(d)),
            "插件依赖编号无效"
        );
        if let Some(page) = &self.page {
            ensure!(
                std::path::Path::new(page)
                    .components()
                    .all(|c| matches!(c, std::path::Component::Normal(_)))
                    && !page.contains('\\')
                    && !page.is_empty(),
                "插件页面路径无效"
            );
        }
        Ok(())
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Context {
    pub plugin: String,
    pub version: String,
    pub config: Value,
    pub secrets: HashMap<String, String>,
    pub task_id: Option<String>,
    #[serde(default)]
    pub dependencies: HashMap<String, String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Request {
    pub context: Context,
    pub method: String,
    pub path: String,
    pub body: Value,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Response {
    pub status: u16,
    pub body: Value,
}
impl Response {
    pub fn ok(body: Value) -> Self {
        Self { status: 200, body }
    }
    pub fn accepted(task: Value) -> Self {
        Self {
            status: 202,
            body: task,
        }
    }
}

#[derive(Debug)]
pub struct Uncertain(pub String);
impl std::fmt::Display for Uncertain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Uncertain {}
pub fn uncertain(message: impl Into<String>) -> anyhow::Error {
    Uncertain(message.into()).into()
}
pub fn failure(error: anyhow::Error) -> Response {
    Response {
        status: if error.is::<Uncertain>() { 409 } else { 400 },
        body: json!({"error":error.to_string()}),
    }
}

#[cfg(unix)]
pub async fn unix_request(
    socket: &Path,
    path: &str,
    token: Option<&str>,
    body: &Value,
) -> Result<Response> {
    use http_body_util::{BodyExt, Full};
    let stream = tokio::net::UnixStream::connect(socket).await?;
    let (mut sender, connection) =
        hyper::client::conn::http1::handshake(hyper_util::rt::TokioIo::new(stream)).await?;
    tokio::spawn(async move {
        let _ = connection.await;
    });
    let mut request = hyper::Request::builder()
        .method("POST")
        .uri(path)
        .header("Host", "localhost")
        .header("Content-Type", "application/json");
    if let Some(token) = token {
        request = request.header("Authorization", format!("Bearer {token}"));
    }
    let response = sender
        .send_request(
            request.body(Full::new(hyper::body::Bytes::from(serde_json::to_vec(
                body,
            )?)))?,
        )
        .await?;
    let status = response.status().as_u16();
    let bytes = response.into_body().collect().await?.to_bytes();
    ensure!(bytes.len() <= 16 * 1024 * 1024, "插件响应超过限制");
    let body = serde_json::from_slice(&bytes).context("插件没有返回 JSON")?;
    Ok(Response { status, body })
}

#[cfg(not(unix))]
pub async fn unix_request(
    _socket: &Path,
    _path: &str,
    _token: Option<&str>,
    _body: &Value,
) -> Result<Response> {
    bail!("插件运行时需要 Linux Unix socket")
}

pub async fn core(operation: &str, input: Value) -> Result<Value> {
    let socket = std::env::var("OPSD_PLUGIN_CORE_SOCKET").context("缺少核心通信地址")?;
    let token = std::env::var("OPSD_PLUGIN_CORE_TOKEN").context("缺少插件身份")?;
    let response = unix_request(
        Path::new(&socket),
        "/call",
        Some(&token),
        &json!({"operation":operation,"input":input}),
    )
    .await?;
    ensure!(
        response.status < 400,
        "{}",
        response.body["error"].as_str().unwrap_or("核心调用失败")
    );
    Ok(response.body)
}

pub async fn submit(
    context: &Context,
    target: &str,
    operation: &str,
    input: Value,
    key: &str,
) -> Result<Value> {
    core("task.submit",json!({"target":target,"plugin":context.plugin,"version":context.version,"operation":operation,"input":input,"key":key,"parent_task":context.task_id})).await
}

pub async fn wait_task(task_id: &str, timeout: u64) -> Result<Value> {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(timeout);
    loop {
        let task = core("task.get", json!({"id":task_id})).await?;
        match task["status"].as_str().unwrap_or("") {
            "succeeded" => return Ok(task["result"].clone()),
            "uncertain" => return Err(uncertain(format!("子任务结果待核实：{task_id}"))),
            "failed" | "blocked" | "cancelled" | "rolled_back" => bail!(
                "子任务 {}：{}",
                task_id,
                task["error"].as_str().unwrap_or("执行未成功")
            ),
            _ => {}
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(uncertain(format!(
                "等待子任务超时，结果需要核实：{task_id}"
            )));
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
}

pub async fn node_task(
    context: &Context,
    node: &str,
    operation: &str,
    input: Value,
    step: &str,
) -> Result<Value> {
    let key = format!(
        "{}:{step}",
        context.task_id.as_deref().context("节点变更必须属于任务")?
    );
    let task = submit(context, node, operation, input, &key).await?;
    wait_task(task["task_id"].as_str().context("缺少任务编号")?, 1800).await
}

pub async fn job(request: &Request, operation: &str) -> Result<Response> {
    let key = request.body["idempotency_key"]
        .as_str()
        .context("写操作缺少幂等键")?;
    let task = submit(
        &request.context,
        "@hub",
        operation,
        request.body.clone(),
        key,
    )
    .await?;
    Ok(Response::accepted(task))
}

pub async fn kv_get(key: &str) -> Result<Value> {
    core("state.get", json!({"key":key})).await
}
pub async fn kv_put(key: &str, value: Value) -> Result<Value> {
    core("state.put", json!({"key":key,"value":value})).await
}

pub fn secret<'a>(context: &'a Context, name: &str) -> Result<&'a str> {
    context
        .secrets
        .get(name)
        .map(String::as_str)
        .context(format!("尚未配置秘密：{name}"))
}

pub fn process(executable: impl AsRef<std::ffi::OsStr>) -> tokio::process::Command {
    let mut command = tokio::process::Command::new(executable);
    command.kill_on_drop(true);
    #[cfg(target_os = "linux")]
    unsafe {
        let parent = std::process::id() as libc::pid_t;
        command.pre_exec(move || {
            if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            if libc::getppid() != parent {
                return Err(std::io::Error::other("父进程已结束"));
            }
            Ok(())
        });
    }
    command
}

pub async fn command(
    executable: &str,
    args: &[String],
    env: &[(String, String)],
    input: Option<Vec<u8>>,
    timeout: u64,
) -> Result<String> {
    use std::process::Stdio;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut child = process(executable)
        .args(args)
        .envs(env.iter().cloned())
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .context(format!("无法启动 {executable}"))?;
    if let Some(input) = input {
        child
            .stdin
            .take()
            .context("缺少输入管道")?
            .write_all(&input)
            .await?;
    }
    let mut stdout = child
        .stdout
        .take()
        .context("缺少输出管道")?
        .take(4 * 1024 * 1024 + 1);
    let mut stderr = child.stderr.take().context("缺少错误管道")?.take(64 * 1024);
    let output = async {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let (_, _, status) = tokio::try_join!(
            stdout.read_to_end(&mut out),
            stderr.read_to_end(&mut err),
            child.wait()
        )?;
        ensure!(out.len() <= 4 * 1024 * 1024, "程序输出超过限制");
        // 外部命令可能回显环境或配置，错误仅报告退出状态。
        ensure!(status.success(), "{executable} 执行失败，退出状态 {status}");
        Ok::<_, anyhow::Error>(String::from_utf8(out)?)
    };
    match tokio::time::timeout(std::time::Duration::from_secs(timeout), output).await {
        Ok(result) => result,
        Err(_) => {
            let _ = child.kill().await;
            Err(uncertain(format!("{executable} 执行超时，需核实实际结果")))
        }
    }
}

pub fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(|s| s.to_string()).collect()
}

pub fn node_directory() -> Result<std::path::PathBuf> {
    Ok(std::env::var("OPSD_PLUGIN_NODE_DIR")
        .context("缺少节点插件数据目录")?
        .into())
}

pub async fn identity() -> Result<Value> {
    let directory = node_directory()?.join("tls");
    std::fs::create_dir_all(&directory)?;
    let key = directory.join("server-key.pem");
    let csr = directory.join("server.csr");
    if !key.exists() {
        command(
            "openssl",
            &[
                "req".into(),
                "-new".into(),
                "-newkey".into(),
                "rsa:2048".into(),
                "-nodes".into(),
                "-subj".into(),
                "/CN=opsd-plugin-service".into(),
                "-keyout".into(),
                key.display().to_string(),
                "-out".into(),
                csr.display().to_string(),
            ],
            &[],
            None,
            30,
        )
        .await?;
    }
    ensure!(
        csr.exists(),
        "节点服务私钥存在但 CSR 缺失，不能覆盖已有身份"
    );
    Ok(json!({"csr":std::fs::read_to_string(csr)?}))
}

pub async fn service_certificate(context: &Context, node: &str, step: &str) -> Result<Value> {
    let identity = node_task(
        context,
        node,
        "identity.prepare",
        json!({}),
        &format!("{step}:identity"),
    )
    .await?;
    core(
        "certificate.sign",
        json!({"node_id":node,"csr":identity["csr"]}),
    )
    .await
}

pub fn private_write(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    Ok(())
}

#[cfg(unix)]
pub async fn serve<F, Fut>(handler: F) -> Result<()>
where
    F: Fn(Request) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Result<Response>> + Send + 'static,
{
    serve_stream(handler, |_, _| async { bail!("该插件没有流接口") }).await
}

pub use axum::extract::ws::{Message, WebSocket};

#[cfg(unix)]
pub async fn serve_stream<F, Fut, G, Flow>(handler: F, stream: G) -> Result<()>
where
    F: Fn(Request) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Result<Response>> + Send + 'static,
    G: Fn(Request, WebSocket) -> Flow + Clone + Send + Sync + 'static,
    Flow: Future<Output = Result<()>> + Send + 'static,
{
    use axum::{Json, Router, routing::post};
    let socket = std::env::var("OPSD_PLUGIN_SOCKET").context("缺少插件监听地址")?;
    let listener = tokio::net::UnixListener::bind(&socket)?;
    let route = post(move |Json(request): Json<Request>| {
        let handler = handler.clone();
        async move {
            match handler(request).await {
                Ok(response) => Json(response),
                Err(error) => Json(failure(error)),
            }
        }
    });
    let flow = axum::routing::get(move |ws: axum::extract::WebSocketUpgrade| {
        let stream = stream.clone();
        async move {
            ws.max_message_size(1024 * 1024)
                .on_upgrade(move |mut socket| async move {
                    let request = match tokio::time::timeout(
                        std::time::Duration::from_secs(10),
                        socket.recv(),
                    )
                    .await
                    {
                        Ok(Some(Ok(Message::Text(text)))) => serde_json::from_str::<Request>(&text),
                        _ => return,
                    };
                    if let Ok(request) = request {
                        let _ = stream(request, socket).await;
                    }
                })
        }
    });
    axum::serve(
        listener,
        Router::new()
            .route("/invoke", route)
            .route("/stream", flow)
            .layer(axum::extract::DefaultBodyLimit::max(1024 * 1024)),
    )
    .await?;
    Ok(())
}

#[cfg(not(unix))]
pub async fn serve_stream<F, Fut, G, Flow>(_handler: F, _stream: G) -> Result<()>
where
    F: Fn(Request) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Result<Response>> + Send + 'static,
    G: Fn(Request, WebSocket) -> Flow + Clone + Send + Sync + 'static,
    Flow: Future<Output = Result<()>> + Send + 'static,
{
    bail!("插件运行时需要 Linux")
}

#[cfg(not(unix))]
pub async fn serve<F, Fut>(_handler: F) -> Result<()>
where
    F: Fn(Request) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Result<Response>> + Send + 'static,
{
    bail!("插件运行时需要 Linux")
}

pub async fn node_input() -> Result<Request> {
    use tokio::io::AsyncReadExt;
    let mut bytes = Vec::new();
    tokio::io::stdin()
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .await?;
    ensure!(bytes.len() <= 1024 * 1024, "节点动作输入超过限制");
    Ok(serde_json::from_slice(&bytes)?)
}

pub fn scrub(value: &mut Value, secrets: &HashMap<String, String>) {
    match value {
        Value::String(text) => {
            for secret in secrets.values().filter(|s| s.len() >= 4) {
                *text = text.replace(secret, "[秘密已隐藏]");
            }
        }
        Value::Array(items) => {
            for item in items {
                scrub(item, secrets);
            }
        }
        Value::Object(items) => {
            for (key, item) in items {
                if [
                    "password",
                    "network_secret",
                    "secret_key",
                    "access_token",
                    "user_token",
                    "credential_file",
                    "private_key",
                    "local_private_key",
                    "admin_password",
                    "agent_secret",
                    "root_password",
                    "Env",
                    "env",
                ]
                .contains(&key.as_str())
                {
                    *item = json!("[秘密已隐藏]");
                } else {
                    scrub(item, secrets);
                }
            }
        }
        _ => {}
    }
}
pub fn address_host(address: &str) -> Result<String> {
    let ip: std::net::IpAddr = address.parse()?;
    Ok(if ip.is_ipv6() {
        format!("[{ip}]")
    } else {
        ip.to_string()
    })
}
