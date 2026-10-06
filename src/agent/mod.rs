mod docker;
mod database;
mod files;
mod firewall;
mod firewall_backends;
mod host;
mod storage;
mod metrics;
mod probe;
mod shell;
mod stack;
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use futures_util::{SinkExt, StreamExt};
use opsd::{pki, protocol::*, store::Store};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::HashMap, path::PathBuf, sync::Arc};
use tokio::sync::{Mutex, RwLock, mpsc};
use tokio_tungstenite::tungstenite::Message;

#[derive(Parser)]
struct Args {
    #[arg(long, env = "OPSD_AGENT_DIR", default_value = ".data/agent")]
    data_dir: PathBuf,
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    Enroll {
        #[arg(long)]
        hub: String,
        #[arg(long)]
        agent_url: String,
        #[arg(long)]
        ca: PathBuf,
        #[arg(long)]
        fingerprint: String,
        #[arg(long)]
        token_file: PathBuf,
    },
    Run,
    RenewCertificate,
    FirewallRollback {
        operation_id: String,
    },
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Config {
    pub node_id: String,
    pub hub: String,
    pub agent_url: String,
}
#[derive(Clone)]
pub struct ContextState {
    pub db: Store,
    pub dir: PathBuf,
    pub config: Config,
    pub outbound: Arc<RwLock<Option<mpsc::Sender<Frame>>>>,
    pub network: Arc<Mutex<()>>,
}
pub async fn run() -> Result<()> {
    let args = Args::parse();
    std::fs::create_dir_all(&args.data_dir)?;
    if let Command::Enroll {
        hub,
        agent_url,
        ca,
        fingerprint,
        token_file,
    } = args.command
    {
        anyhow::ensure!(
            hub.starts_with("https://") && agent_url.starts_with("wss://"),
            "注册与节点通道必须使用 TLS"
        );
        anyhow::ensure!(
            !args.data_dir.join("config.json").exists(),
            "节点已登记，拒绝覆盖身份"
        );
        let ca_pem = std::fs::read_to_string(ca)?;
        anyhow::ensure!(
            pki::cert_fingerprint(&ca_pem)? == fingerprint.to_lowercase(),
            "主控 CA 指纹不匹配"
        );
        let key = rcgen::KeyPair::generate()?;
        let csr = rcgen::CertificateParams::new(Vec::<String>::new())?
            .serialize_request(&key)?
            .pem()?;
        let client = reqwest::Client::builder()
            .tls_built_in_root_certs(false)
            .add_root_certificate(reqwest::Certificate::from_pem(ca_pem.as_bytes())?)
            .build()?;
        let response = client
            .post(format!("{}/api/v1/enroll", hub.trim_end_matches('/')))
            .json(&json!({"token":std::fs::read_to_string(token_file)?.trim(),"csr":csr}))
            .send()
            .await?
            .error_for_status()?
            .json::<Value>()
            .await?;
        let config = Config {
            node_id: response["node_id"].as_str().context("缺少节点 ID")?.into(),
            hub,
            agent_url,
        };
        pki::private_write(&args.data_dir.join("ca.pem"), ca_pem)?;
        pki::private_write(&args.data_dir.join("client-key.pem"), key.serialize_pem())?;
        pki::private_write(
            &args.data_dir.join("client.pem"),
            response["certificate"].as_str().context("缺少节点证书")?,
        )?;
        pki::private_write(
            &args.data_dir.join("config.json"),
            serde_json::to_vec(&config)?,
        )?;
        println!("节点注册完成：{}", config.node_id);
        return Ok(());
    }
    let db = Store::open(&format!(
        "sqlite://{}?mode=rwc",
        args.data_dir.join("node.db").display()
    ))
    .await?;
    let config: Config =
        serde_json::from_slice(&std::fs::read(args.data_dir.join("config.json"))?)?;
    let ctx = ContextState {
        db,
        dir: args.data_dir,
        config,
        outbound: Default::default(),
        network: Default::default(),
    };
    if let Command::FirewallRollback { operation_id } = args.command {
        return firewall::rollback(&ctx, &operation_id).await;
    }
    let agent_lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(ctx.dir.join("agent.lock"))?;
    fs2::FileExt::try_lock_exclusive(&agent_lock).context("该节点 Agent 已运行")?;
    if matches!(args.command, Command::RenewCertificate) {
        return renew_certificate(&ctx, true).await;
    }
    for mut t in ctx.db.tasks().await? {
        if matches!(t.status, TaskStatus::Running | TaskStatus::Validating) {
            t.status = TaskStatus::Uncertain;
            t.sequence += 1;
            t.error = Some("执行器重启，需核实实际结果；未自动重试".into());
            t.updated_at = now();
            ctx.db.save_task(&t).await?;
        }
    }
    let (worker_tx, mut worker_rx) = mpsc::channel::<String>(32);
    let worker_ctx = ctx.clone();
    tokio::spawn(async move {
        while let Some(task_id) = worker_rx.recv().await {
            if let Err(e) = execute(&worker_ctx, &task_id).await {
                tracing::error!(error=%e,"任务持久化失败");
            }
        }
    });
    for t in ctx.db.tasks().await? {
        if matches!(t.status, TaskStatus::Pending | TaskStatus::Accepted) {
            worker_tx.send(t.id).await?;
        }
    }
    let collector = ctx.clone();
    let renewer = ctx.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(3600));
        loop {
            tick.tick().await;
            if let Err(error) = renew_certificate(&renewer, false).await {
                tracing::warn!(%error, "节点证书续期检查失败，将在下一周期重试");
            }
        }
    });
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(30));
        loop {
            tick.tick().await;
            if collector.outbound.read().await.is_none() {
                continue;
            }
            if let Ok(value) = inspect(&collector).await
                && let Some(tx) = collector.outbound.read().await.as_ref()
            {
                let _ = tx.try_send(Frame::Inventory { value });
            }
        }
    });
    let mut backoff = 1;
    loop {
        let start = now();
        match connection(ctx.clone(), worker_tx.clone()).await {
            Ok(_) => {}
            Err(e) => tracing::warn!(error=%e,"主控连接断开"),
        };
        *ctx.outbound.write().await = None;
        if now() - start > 30 {
            backoff = 1
        }
        tokio::time::sleep(std::time::Duration::from_secs(backoff)).await;
        backoff = (backoff * 2).min(30);
    }
}
async fn renew_certificate(ctx: &ContextState, force: bool) -> Result<()> {
    let certificate = std::fs::read(ctx.dir.join("client.pem"))?;
    if !force && pki::certificate_expires(&certificate)? > now() + 30 * 86400 {
        return Ok(());
    }
    let key_pem = std::fs::read_to_string(ctx.dir.join("client-key.pem"))?;
    let key = rcgen::KeyPair::from_pem(&key_pem)?;
    let csr = rcgen::CertificateParams::new(Vec::<String>::new())?
        .serialize_request(&key)?
        .pem()?;
    let mut identity = certificate;
    identity.extend_from_slice(key_pem.as_bytes());
    let client = reqwest::Client::builder()
        .tls_built_in_root_certs(false)
        .add_root_certificate(reqwest::Certificate::from_pem(&std::fs::read(
            ctx.dir.join("ca.pem"),
        )?)?)
        .identity(reqwest::Identity::from_pem(&identity)?)
        .timeout(std::time::Duration::from_secs(30))
        .build()?;
    let endpoint = ctx
        .config
        .agent_url
        .replacen("wss://", "https://", 1)
        .trim_end_matches("/agent")
        .to_string()
        + "/renew";
    let response: Value = client
        .post(endpoint)
        .json(&json!({"csr":csr}))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let renewed = response["certificate"]
        .as_str()
        .context("续期响应缺少证书")?;
    anyhow::ensure!(
        pki::certificate_expires(renewed.as_bytes())? > now() + 30 * 86400,
        "续期证书有效期不足"
    );
    stack::atomic_replace(&ctx.dir.join("client.pem"), renewed.as_bytes())?;
    tracing::info!("节点证书已续期，后续连接使用新证书");
    Ok(())
}
async fn capabilities() -> NodeCapabilities {
    let container = container_mode();
    NodeCapabilities {
        os: std::env::consts::OS.into(),
        docker: std::path::Path::new("/var/run/docker.sock").exists(),
        compose: command("/usr/bin/docker", &["compose", "version"], 10)
            .await
            .is_ok(),
        systemd: !container && std::path::Path::new("/run/systemd/system").exists(),
        firewall: ["nft", "iptables", "ufw", "firewall-cmd"]
            .iter()
            .filter(|c| {
                std::path::Path::new(&format!("/usr/sbin/{c}")).exists()
                    || std::path::Path::new(&format!("/usr/bin/{c}")).exists()
            })
            .map(|c| c.to_string())
            .collect(),
        metrics: cfg!(target_os = "linux"),
        hostinfo: cfg!(target_os = "linux"),
        storage: !container && cfg!(target_os = "linux"),
        database: cfg!(target_os = "linux"),
        // 探测只用 ping 或 TCP 握手，不需要额外特权；`ping` 缺失时
        // 结果里会如实写出「本机没有 ping 命令」，而不是假装网络不通。
        probe: cfg!(target_os = "linux"),
        execution_mode: if container {
            AgentInstallMode::Docker
        } else {
            AgentInstallMode::Host
        },
    }
}
async fn connection(ctx: ContextState, worker: mpsc::Sender<String>) -> Result<()> {
    let config = Arc::new(pki::client_config(&ctx.dir)?);
    let (mut socket, _) = tokio_tungstenite::connect_async_tls_with_config(
        &ctx.config.agent_url,
        None,
        false,
        Some(tokio_tungstenite::Connector::Rustls(config)),
    )
    .await?;
    // 能力位只声明一次，采样分支依据它决定是否上报，避免出现
    // 「声称没有采样能力、却持续发送采集失败」这种自相矛盾的状态。
    let capabilities = capabilities().await;
    let can_sample = capabilities.metrics;
    let can_probe = capabilities.probe;
    socket
        .send(Message::Text(
            serde_json::to_string(&Frame::Hello {
                version: VERSION,
                node_id: ctx.config.node_id.clone(),
                capabilities,
            })?
            .into(),
        ))
        .await?;
    let (tx, mut rx) = mpsc::channel(64);
    let (data_tx, mut data_rx) = mpsc::channel(128);
    *ctx.outbound.write().await = Some(tx.clone());
    for task in ctx.db.tasks().await? {
        socket
            .send(Message::Text(
                serde_json::to_string(&Frame::Report { task })?.into(),
            ))
            .await?;
    }
    let mut streams: HashMap<String, (mpsc::Sender<String>, tokio::task::JoinHandle<()>)> =
        HashMap::new();
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
    // 指标每 15 秒采样一次。采集只读 /proc 与 statvfs，不做外部命令；
    // 失败时上报 error 而不是零值，主控据此显示「采集失败」。
    let mut sampler = metrics::Sampler::new();
    let mut sampling = tokio::time::interval(std::time::Duration::from_secs(15));
    sampling.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    // 对端延迟探测独立成任务：一轮要连十几个对端，串在采样里会把采样节奏带偏。
    // 采样只取"最近一轮已完成的结果"，因此不会被阻塞。
    let probe_results: Arc<RwLock<Vec<PeerLatency>>> = Default::default();
    let probe_task = {
        let db = ctx.db.clone();
        let results = probe_results.clone();
        tokio::spawn(async move {
            if can_probe {
                probe::probe_loop(db, results).await;
            }
        })
    };
    let result:Result<()>=async{loop{tokio::select!{
        biased;
        _=interval.tick()=>{socket.send(Message::Text(serde_json::to_string(&Frame::Heartbeat)?.into())).await?;},
        _=sampling.tick()=>{
            // 没有采样能力时不发送，避免与 Hello 里的能力位自相矛盾。
            if can_sample {
                let peers=probe_results.read().await.clone();
                let report=report_metrics(&mut sampler,peers);
                socket.send(Message::Text(serde_json::to_string(&Frame::Metrics{report})?.into())).await?;
            }
        },
        Some(frame)=rx.recv()=>{socket.send(Message::Text(serde_json::to_string(&frame)?.into())).await?;},
        message=socket.next()=>{
            let message=message.context("连接结束")??;
            match message{Message::Ping(v)=>{socket.send(Message::Pong(v)).await?;continue},Message::Close(_)=>break,Message::Text(_)=>{},_=>continue}
            let Message::Text(text)=message else{continue};let frame:Frame=serde_json::from_str(&text)?;
            match frame{
                Frame::Task{mut task}=>{
                    anyhow::ensure!(task.node_id==ctx.config.node_id,"拒绝其他节点的任务");
                    task.status=TaskStatus::Accepted;task.sequence=1;task.result=None;task.error=None;
                    let accepted=ctx.db.accept_task(&task).await?;
                    // 先排队再回执；断线重投可补排已落盘但未进入执行队列的任务。
                    if matches!(accepted.status,TaskStatus::Accepted|TaskStatus::Pending){let _=worker.try_send(accepted.id.clone());}
                    socket.send(Message::Text(serde_json::to_string(&Frame::Report{task:accepted.clone()})?.into())).await?;
                },
                Frame::StreamOpen{stream_id,kind,target,command}=>{
                    anyhow::ensure!(streams.len()<8,"并发数据流超过上限");
                    let(input,input_rx)=mpsc::channel(16);let out=data_tx.clone();let sid=stream_id.clone();
                    let jail=file_jail(&ctx);
                    let handle=tokio::spawn(async move{
                        // 用内层 async 块承接错误，使 ? 落在 result 上而不是整个任务上
                        let result:Result<()>=async{
                            match (kind,target){
                                (StreamKind::ContainerTerminal,StreamTarget::Container{id})=>
                                    docker::stream(&sid,&id,true,command,input_rx,out.clone()).await,
                                (StreamKind::HostShell,StreamTarget::Host{shell,workdir})=>
                                    shell::Session::spawn(&jail,&shell,workdir.as_deref()).await?
                                        .pump(input_rx,out.clone(),sid.clone()).await,
                                (StreamKind::FileList,StreamTarget::Dir{path})=>
                                    stream_file_list(&sid,&jail,&path,out.clone()).await,
                                (StreamKind::FileRead,StreamTarget::File{path,offset,limit})=>
                                    stream_file_read(&sid,&jail,&path,offset,limit,out.clone()).await,
                                (StreamKind::FileWrite,StreamTarget::File{path,offset,..})=>
                                    stream_file_write(&sid,&jail,&path,offset,input_rx,out.clone()).await,
                                _=>anyhow::bail!("流类型与目标不匹配"),
                            }
                        }.await;
                        let _=out.send(Frame::StreamClose{stream_id:sid,error:result.err().map(|e|e.to_string())}).await;
                    });
                    streams.insert(stream_id,(input,handle));
                },
                Frame::StreamData{stream_id,data}=>{if let Some((tx,_))=streams.get(&stream_id){let _=tx.try_send(data);}},
                Frame::StreamClose{stream_id,..}=>{if let Some((_,handle))=streams.remove(&stream_id){handle.abort();}},
                Frame::Probe{request_id,address,port}=>{let out=tx.clone();tokio::spawn(async move{let ok=tokio::time::timeout(std::time::Duration::from_secs(5),tokio::net::TcpStream::connect((address.as_str(),port))).await.is_ok_and(|r|r.is_ok());let _=out.send(Frame::ProbeResult{request_id,ok}).await;});},
                _=>anyhow::bail!("主控消息类型无效"),
            }
        },
        Some(frame)=data_rx.recv()=>{socket.send(Message::Text(serde_json::to_string(&frame)?.into())).await?;}
    }}Ok(())}.await;
    for (_, (_, handle)) in streams {
        handle.abort();
    }
    // 探测任务属于这一次连接：重连会重建，因此不能让它留到下一次连接里去
    probe_task.abort();
    result
}
/// 文件操作的路径闸门。除固定禁止的前缀外，还要挡住 Agent 自己的数据目录，
/// 那里存放节点身份密钥与本地任务库。
fn file_jail(ctx: &ContextState) -> files::Jail {
    files::Jail::new(vec![], vec![ctx.dir.clone()])
}

/// 流通道以 base64 承载二进制，界面侧按同样方式解码。
fn encode_bytes(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(data)
}
fn decode_bytes(data: &str) -> Result<Vec<u8>> {
    use base64::Engine;
    Ok(base64::engine::general_purpose::STANDARD.decode(data)?)
}

/// 列目录：只读，走流式请求-响应，**不进任务表**。
/// 结果作为一条 JSON 消息返回，随后流关闭。
async fn stream_file_list(
    stream_id: &str,
    jail: &files::Jail,
    path: &str,
    out: mpsc::Sender<Frame>,
) -> Result<()> {
    let entries = files::list(jail, path)?;
    let info = files::stat(jail, path)?;
    let payload = serde_json::to_vec(&json!({
        "path": info.path,
        "entries": entries,
    }))?;
    let _ = out
        .send(Frame::StreamData {
            stream_id: stream_id.to_owned(),
            data: encode_bytes(&payload),
        })
        .await;
    Ok(())
}

/// 只读文件：一次性把区间内容送出，不占用长连接。
async fn stream_file_read(
    stream_id: &str,
    jail: &files::Jail,
    path: &str,
    offset: u64,
    limit: u64,
    out: mpsc::Sender<Frame>,
) -> Result<()> {
    let data = files::read(jail, path, offset, limit)?;
    let _ = out
        .send(Frame::StreamData {
            stream_id: stream_id.to_owned(),
            data: encode_bytes(&data),
        })
        .await;
    Ok(())
}

/// 写入文件：把流上收到的分块写到**确定性的暂存路径**。
///
/// 暂存路径由目标路径推导，因此内容传完后 `FilePut` 任务不需要额外信息就能找到它。
/// 只有任务核对摘要与大小一致后才会原子改名覆盖目标——半截上传不会破坏原文件。
async fn stream_file_write(
    stream_id: &str,
    jail: &files::Jail,
    path: &str,
    offset: u64,
    mut input: mpsc::Receiver<String>,
    out: mpsc::Sender<Frame>,
) -> Result<()> {
    let staged = files::staging_path(jail, path)?;
    let mut written: u64 = 0;
    {
        use tokio::io::AsyncWriteExt;
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(offset == 0)
            .open(&staged)
            .await?;
        while let Some(chunk) = input.recv().await {
            let bytes = decode_bytes(&chunk)?;
            anyhow::ensure!(
                written + bytes.len() as u64 <= files::MAX_WRITE_BYTES,
                "上传超过单文件上限"
            );
            file.write_all(&bytes).await?;
            written += bytes.len() as u64;
        }
        file.sync_all().await?;
    }
    // 回执给出摘要，供主控在 FilePut 任务里原样带上
    let bytes = tokio::fs::read(&staged).await?;
    let receipt = serde_json::json!({
        "staged": staged.display().to_string(),
        "size": bytes.len(),
        "digest": protocol_digest(&bytes),
    });
    let _ = out
        .send(Frame::StreamData {
            stream_id: stream_id.to_owned(),
            data: encode_bytes(serde_json::to_string(&receipt)?.as_bytes()),
        })
        .await;
    Ok(())
}

/// 把暂存的上传内容按摘要确认后就位。校验逻辑在 `files::put_staged` 中，
/// 那里也是这四道防线测试覆盖的地方。
async fn file_put(jail: &files::Jail, path: &str, expected: &str, size: u64) -> Result<Value> {
    let written = files::put_staged(jail, path, expected, size)?;
    Ok(json!({ "path": path, "size": written }))
}

fn protocol_digest(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(bytes))
}

/// 采样一次并包装成上报。采样失败时给出 error 与空 sample，
/// 让主控能区分「采集失败」与「旧 Agent 未上报」。
fn report_metrics(sampler: &mut metrics::Sampler, peers: Vec<PeerLatency>) -> MetricsReport {
    match sampler.sample() {
        Ok(mut sample) => {
            // 只有真正测过的那一次采样才带延迟；没测到的对端不会出现在列表里，
            // 也绝不补 0 —— 0.0 毫秒是"同机"，与"没测到"是两回事。
            sample.peers = peers;
            MetricsReport {
                at: sample.at,
                sample: Some(sample),
                error: None,
            }
        }
        Err(error) => {
            tracing::warn!(%error, "主机指标采集失败");
            MetricsReport {
                at: now(),
                sample: None,
                error: Some(error.to_string()),
            }
        }
    }
}
pub async fn report(ctx: &ContextState, task: &mut TaskEnvelope) -> Result<()> {    task.sequence += 1;
    ctx.db.save_task(task).await?;
    if let Some(tx) = ctx.outbound.read().await.as_ref() {
        let _ = tx.try_send(Frame::Report { task: task.clone() });
    }
    Ok(())
}
fn container_mode() -> bool {
    std::env::var("OPSD_AGENT_MODE").as_deref() == Ok("container")
}
async fn execute(ctx: &ContextState, id: &str) -> Result<()> {
    let mut task = ctx.db.task(id).await?.context("任务不存在")?;
    if !matches!(task.status, TaskStatus::Pending | TaskStatus::Accepted) {
        return Ok(());
    }
    task.status = TaskStatus::Running;
    task.updated_at = now();
    report(ctx, &mut task).await?;
    let result: Result<Value> = match &task.action {
        Action::Inspect {} => inspect(ctx).await,
        // 主机盘点只读，结果进 inventory 供控制台做存储预检
        Action::HostInspect {} => {
            anyhow::ensure!(
                cfg!(target_os = "linux"),
                "主机盘点只在 Linux 节点上可用"
            );
            let inventory = host::inspect().await;
            Ok(serde_json::to_value(&inventory)?)
        }
        Action::DockerInspect { container } => docker::detail(container).await,
        Action::StackCreate { project, content } => stack::create(ctx, project, content).await,
        Action::Docker {
            container,
            operation,
        } => {
            let _lock = ctx.network.lock().await;
            docker::action(container, operation).await
        }
        Action::PullImage { reference } => docker::pull(reference).await,
        Action::StackImport {
            project,
            directory,
            files,
            env_files,
        } => stack::import(ctx, project, directory, files, env_files).await,
        Action::StackPlan { project, content } => stack::plan(ctx, project, content).await,
        Action::StackApply { plan_id } => {
            let _lock = ctx.network.lock().await;
            stack::apply(ctx, plan_id).await
        }
        Action::StackControl { project, operation } => {
            let _lock = ctx.network.lock().await;
            stack::control(ctx, project, operation).await
        }
        Action::FirewallPlan { operation } => firewall::plan(ctx, operation).await,
        Action::FirewallApply { plan_id, witness } => {
            anyhow::ensure!(!container_mode(), "Docker Agent 不支持防火墙写入");
            let _lock = ctx.network.lock().await;
            firewall::apply(ctx, plan_id, witness.as_deref()).await
        }
        Action::PeerSync { set } => {
            anyhow::ensure!(!container_mode(), "Docker Agent 不支持节点地址发布");
            let _lock = ctx.network.lock().await;
            firewall::peers(ctx, set).await
        }
        // 探测目标是配置而非变更：落盘即可，探测循环下一轮自己会读到。
        Action::PeerProbeTargets { set } => {
            probe::save(&ctx.db, set).await?;
            Ok(json!({ "version": set.version, "targets": set.targets.len() }))
        }
        // 以下为结构化文件变更。它们走任务机制而不是流通道：天然带幂等键与事件，
        // 并参与节点变更锁，不会与防火墙或存储任务同时改动同一台机器。
        Action::FilePut {
            path,
            digest: expected,
            size,
        } => {
            anyhow::ensure!(!container_mode(), "Docker Agent 不支持主机文件写入");
            let _lock = ctx.network.lock().await;
            file_put(&file_jail(ctx), path, expected, *size).await
        }
        Action::FileRemove {
            path,
            recursive,
            confirmed,
        } => {
            anyhow::ensure!(!container_mode(), "Docker Agent 不支持主机文件删除");
            let _lock = ctx.network.lock().await;
            let jail = file_jail(ctx);
            let removed = files::remove(&jail, path, *recursive, *confirmed)?;
            // 目标被删除时，它的未完成上传也没有保留价值
            let _ = files::discard_staged(&jail, path);
            Ok(json!({ "removed": removed }))
        }
        Action::FileRename { from, to } => {
            anyhow::ensure!(!container_mode(), "Docker Agent 不支持主机文件操作");
            let _lock = ctx.network.lock().await;
            files::rename(&file_jail(ctx), from, to)?;
            Ok(json!({ "from": from, "to": to }))
        }
        Action::FileMkdir { path } => {
            anyhow::ensure!(!container_mode(), "Docker Agent 不支持主机文件操作");
            let _lock = ctx.network.lock().await;
            files::mkdir(&file_jail(ctx), path)?;
            Ok(json!({ "path": path }))
        }
        Action::FileChmod { path, mode } => {
            anyhow::ensure!(!container_mode(), "Docker Agent 不支持主机文件操作");
            let _lock = ctx.network.lock().await;
            files::chmod(&file_jail(ctx), path, *mode)?;
            Ok(json!({ "path": path, "mode": mode }))
        }
        Action::FileChown { path, uid, gid } => {
            anyhow::ensure!(!container_mode(), "Docker Agent 不支持主机文件操作");
            let _lock = ctx.network.lock().await;
            files::chown(&file_jail(ctx), path, *uid, *gid)?;
            Ok(json!({ "path": path, "uid": uid, "gid": gid }))
        }
        // 存储集群：磁盘准备会销毁数据，因此与其它节点变更共用同一把锁，
        // 绝不会与文件、防火墙或容器任务同时改动同一台机器。
        Action::StoragePrepare {
            cluster,
            plan_id,
            devices,
            filesystem,
        } => {
            anyhow::ensure!(!container_mode(), "Docker Agent 不支持存储变更");
            let _lock = ctx.network.lock().await;
            anyhow::ensure!(
                cfg!(target_os = "linux"),
                "存储集群只在 Linux 节点上可用"
            );
            storage::prepare(cluster, plan_id, devices, filesystem).await
        }
        Action::StorageDeploy {
            cluster,
            plan_id,
            image,
            mounts,
            peers,
            access_key,
            secret_key,
        } => {
            anyhow::ensure!(!container_mode(), "Docker Agent 不支持存储变更");
            let _lock = ctx.network.lock().await;
            storage::deploy(
                &ctx.dir,
                cluster,
                plan_id,
                image,
                mounts,
                peers,
                access_key,
                secret_key,
            )
            .await
        }
        Action::StorageStatus { cluster } => storage::status(cluster).await,
        // 数据库只读巡检。语句固定在 agent::database 里，主控无法影响它们。
        Action::DbInspect {} => {
            anyhow::ensure!(
                cfg!(target_os = "linux"),
                "数据库只读巡检只在 Linux 节点上可用"
            );
            let report = database::inspect(&ctx.dir).await;
            Ok(serde_json::to_value(&report)?)
        }
        Action::StorageBucket {
            cluster,
            bucket,
            remove,
        } => {
            anyhow::ensure!(!container_mode(), "Docker Agent 不支持存储变更");
            let _lock = ctx.network.lock().await;
            storage::bucket(cluster, bucket, *remove).await
        }
        Action::StorageUser {
            cluster,
            user,
            secret,
            policy,
            remove,
        } => {
            anyhow::ensure!(!container_mode(), "Docker Agent 不支持存储变更");
            let _lock = ctx.network.lock().await;
            storage::user(cluster, user, secret, policy, *remove).await
        }
    };
    match result {
        Ok(v) => {
            task.status = TaskStatus::Succeeded;
            task.result = Some(v)
        }
        Err(e) => {
            task.status = TaskStatus::Failed;
            task.error = Some(e.to_string())
        }
    }
    task.updated_at = now();
    report(ctx, &mut task).await?;
    if let Ok(value) = inspect(ctx).await
        && let Some(tx) = ctx.outbound.read().await.as_ref()
    {
        let _ = tx.try_send(Frame::Inventory { value });
    }
    Ok(())
}
async fn inspect(ctx: &ContextState) -> Result<Value> {
    let docker = match docker::inventory().await {
        Ok(v) => json!({"state":"ok","data":v}),
        Err(e) => json!({"state":"error","error":e.to_string()}),
    };
    let firewall = match firewall::inspect(ctx).await {
        Ok(v) => json!({"state":"ok","data":v}),
        Err(e) => json!({"state":"error","error":e.to_string()}),
    };
    Ok(
        json!({"collected_at":now(),"docker":docker,"firewall":firewall,"stacks":ctx.db.list::<Value>("stacks").await?}),
    )
}
pub async fn command(executable: &str, args: &[&str], timeout: u64) -> Result<String> {
    command_env(executable, args, &[], timeout).await
}

/// 与 `command` 相同，但可以额外注入环境变量。
///
/// 只给数据库只读巡检用：密码经这里进入容器内客户端的 MYSQL_PWD，
/// 于是它既不出现在 **Agent 自己** 的命令行里，也不上传主控。
pub async fn command_env(
    executable: &str,
    args: &[&str],
    env: &[(&str, &str)],
    timeout: u64,
) -> Result<String> {
    use tokio::io::AsyncReadExt;
    async fn read_bounded(reader: impl tokio::io::AsyncRead + Unpin) -> Result<Vec<u8>> {
        let mut bytes = Vec::new();
        reader
            .take(4 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .await?;
        anyhow::ensure!(bytes.len() <= 4 * 1024 * 1024, "系统命令输出超过限制");
        Ok(bytes)
    }
    let mut child = tokio::process::Command::new(executable);
    child
        .args(args)
        .envs(env.iter().copied())
        .kill_on_drop(true)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut process = child.spawn()?;
    let stdout = process.stdout.take().context("缺少标准输出管道")?;
    let stderr = process.stderr.take().context("缺少错误输出管道")?;
    let execution = async {
        let (stdout, stderr, status) =
            tokio::try_join!(read_bounded(stdout), read_bounded(stderr), async {
                Ok::<_, anyhow::Error>(process.wait().await?)
            })?;
        Ok::<_, anyhow::Error>((stdout, stderr, status))
    };
    let result = tokio::time::timeout(std::time::Duration::from_secs(timeout), execution).await;
    let (stdout, stderr, status) = match result {
        Ok(Ok(output)) => output,
        error => {
            let _ = process.kill().await;
            return match error {
                Ok(Err(error)) => Err(error),
                _ => anyhow::bail!("系统命令超时"),
            };
        }
    };
    anyhow::ensure!(
        status.success(),
        "系统命令执行失败：{}",
        if args.first() == Some(&"compose") && args.contains(&"config") {
            "Compose 配置校验失败；为避免环境变量泄漏，未返回原始输出".to_string()
        } else {
            String::from_utf8_lossy(&stderr)
                .chars()
                .take(1000)
                .collect::<String>()
        }
    );
    Ok(String::from_utf8(stdout)?)
}
