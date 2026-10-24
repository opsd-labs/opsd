mod api;
mod audit;
mod auth;
mod channel;
mod database;
mod export;
mod host;
mod installation;
mod metrics;
mod network;
mod plugins;
mod repository;
mod theme;
use anyhow::Result;
use axum::{
    Router,
    extract::Path,
    middleware,
    routing::{delete, get, post, put},
};
use clap::{Parser, Subcommand};
use opsd::{entrance, pki, protocol::*, store::Store, transport};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf, sync::Arc};
use tokio::sync::{Mutex, RwLock, broadcast, mpsc};

/// 安全入口在控制库中的存放位置。
const ENTRANCE_BUCKET: &str = "settings";
const ENTRANCE_ID: &str = "entrance";

#[derive(Parser)]
pub struct Args {
    #[arg(long, env = "OPSD_DATA_DIR", default_value = ".data/hub")]
    data_dir: PathBuf,
    #[arg(long, env = "OPSD_DATABASE_URL")]
    database_url: Option<String>,
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    Init {
        #[arg(long)]
        password_file: PathBuf,
        #[arg(long, default_value = "localhost,127.0.0.1")]
        names: String,
    },
    Serve {
        #[arg(long, env = "OPSD_BIND", default_value = "127.0.0.1:65535")]
        listen: std::net::SocketAddr,
        #[arg(long, default_value = "0.0.0.0:8444")]
        agent_listen: std::net::SocketAddr,
        #[arg(long, env = "OPSD_HEALTH_BIND", default_value = "127.0.0.1:65534")]
        health_listen: std::net::SocketAddr,
        #[arg(long, env = "OPSD_ORIGIN", default_value = "https://localhost:65535")]
        origin: String,
        #[arg(long, default_value = "web/dist")]
        web: PathBuf,
    },
    Backup {
        #[arg(long)]
        destination: PathBuf,
    },
    Restore {
        #[arg(long)]
        source: PathBuf,
    },
    /// 容器健康检查：只访问回环健康监听，不经过安全入口，也不需要控制库与主控锁。
    Probe {
        #[arg(long, env = "OPSD_HEALTH_BIND", default_value = "127.0.0.1:65534")]
        health_listen: std::net::SocketAddr,
    },
}
/// 对回环健康监听发一次最简 HTTP 请求；仅用于容器健康检查。
fn probe(address: std::net::SocketAddr) -> Result<()> {
    use std::io::{Read, Write};
    let mut stream =
        std::net::TcpStream::connect_timeout(&address, std::time::Duration::from_secs(3))?;
    stream.set_read_timeout(Some(std::time::Duration::from_secs(3)))?;
    stream.write_all(
        format!("GET /api/v1/health HTTP/1.0\r\nHost: {address}\r\nConnection: close\r\n\r\n")
            .as_bytes(),
    )?;
    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    anyhow::ensure!(
        response.starts_with("HTTP/1.") && response.contains(" 200"),
        "健康检查未返回成功状态"
    );
    Ok(())
}
#[derive(Clone)]
pub struct State {
    pub db: Store,
    pub dir: PathBuf,
    pub origin: String,
    pub channels: Arc<RwLock<NodeChannels>>,
    pub events: broadcast::Sender<serde_json::Value>,
    pub streams: broadcast::Sender<(String, Frame)>,
    pub gate: Arc<Mutex<()>>,
    pub network_gate: Arc<Mutex<()>>,
    pub login_limits: Arc<Mutex<HashMap<String, (i64, u32)>>>,
    /// 运行期可改的安全入口，由门禁直接读取。
    pub entrance: entrance::Shared,
    /// 各节点最新一次指标，属于有界内存缓存；历史在控制库中分层保存。
    pub latest_metrics: Arc<RwLock<HashMap<String, metrics::NodeMetrics>>>,
    /// 控制台静态资源根目录。
    pub web: PathBuf,
    /// 各节点最近一次通过核心通道上报的主机事实。
    pub host_inventories: host::SharedInventories,
    /// 各节点最近一次数据库只读巡检结果。同样只留在内存里：
    /// 它体积不大但变化很快，落库只会得到一份过期的运维视图。
    pub db_reports: database::SharedDbReports,
    pub plugin_runtimes: plugins::Runtimes,
    pub plugin_tokens: plugins::Tokens,
    pub plugin_management: Arc<Mutex<()>>,
    pub plugin_socket_dir: Arc<tempfile::TempDir>,
}
type NodeChannels = network::NodeChannels;
#[derive(Clone, Serialize, Deserialize)]
pub struct CertificateBinding {
    pub node_id: String,
    pub expires: i64,
}
pub async fn run() -> Result<()> {
    let args = Args::parse();
    // 健康检查在主控运行时由容器调用，必须先于主控锁与控制库处理。
    if let Command::Probe { health_listen } = args.command {
        return probe(health_listen);
    }
    std::fs::create_dir_all(&args.data_dir)?;
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(args.data_dir.join("hub.lock"))?;
    fs2::FileExt::try_lock_exclusive(&lock)
        .map_err(|_| anyhow::anyhow!("主控正在运行；备份恢复须先停止主控，避免不一致快照"))?;
    let url = args.database_url.unwrap_or_else(|| {
        format!(
            "sqlite://{}?mode=rwc",
            args.data_dir.join("control.db").display()
        )
    });
    let db = Store::open(&url).await?;
    match args.command {
        Command::Init {
            password_file,
            names,
        } => {
            anyhow::ensure!(
                db.get::<String>("auth", "admin").await?.is_none(),
                "管理员已存在"
            );
            let password = std::fs::read_to_string(password_file)?;
            anyhow::ensure!(password.trim().len() >= 12, "管理员密码至少需要十二个字符");
            pki::initialize(
                &args.data_dir.join("pki"),
                names.split(',').map(str::to_owned).collect(),
            )?;
            db.insert("auth", "admin", &auth::hash_password(password.trim())?)
                .await?;
            // 安全入口在首次初始化时生成，并写入日志供管理员保存。
            let entrance = entrance::generate();
            entrance::validate(&entrance)?;
            db.insert(ENTRANCE_BUCKET, ENTRANCE_ID, &entrance).await?;
            println!(
                "初始化完成，CA 指纹：{}",
                pki::cert_fingerprint(&std::fs::read_to_string(args.data_dir.join("pki/ca.pem"))?)?
            );
            println!("控制台安全入口：{entrance}");
            println!(
                "访问地址形如 https://<主机>:65535/{entrance}/；不带安全入口的请求会被直接丢弃。"
            );
            println!("安全入口可在控制台「设置」中修改，请通过可信渠道保存。");
        }
        Command::Backup { destination } => {
            anyhow::ensure!(!destination.exists(), "备份目标已存在");
            std::fs::create_dir_all(&destination)?;
            let records = sqlx::query("SELECT bucket,id,value FROM records")
                .fetch_all(&db.pool)
                .await?;
            use sqlx::Row;
            let records:Vec<_>=records.into_iter().map(|r|serde_json::json!({"bucket":r.get::<String,_>("bucket"),"id":r.get::<String,_>("id"),"value":r.get::<String,_>("value")})).collect();
            pki::private_write(
                &destination.join("backup.json"),
                serde_json::to_vec(
                    &serde_json::json!({"version":1,"records":records,"tasks":db.tasks().await?}),
                )?,
            )?;
            std::fs::create_dir_all(destination.join("pki"))?;
            for entry in std::fs::read_dir(args.data_dir.join("pki"))? {
                let e = entry?;
                pki::private_write(
                    &destination.join("pki").join(e.file_name()),
                    std::fs::read(e.path())?,
                )?;
            }
            plugins::copy_state(&args.data_dir.join("plugins"), &destination.join("plugins"))?;
            println!(
                "一致性备份已保存：{}；备份包含节点身份密钥，请存放在受保护的异机位置",
                destination.display()
            );
        }
        Command::Restore { source } => {
            anyhow::ensure!(
                db.get::<String>("auth", "admin").await?.is_none(),
                "恢复只能写入全新控制库"
            );
            let backup: serde_json::Value =
                serde_json::from_slice(&std::fs::read(source.join("backup.json"))?)?;
            anyhow::ensure!(backup["version"] == 1, "备份版本不受支持");
            let records = backup["records"]
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("备份缺少记录"))?;
            let tasks: Vec<TaskEnvelope> = serde_json::from_value(backup["tasks"].clone())?;
            for name in ["ca.pem", "ca-key.pem", "server.pem", "server-key.pem"] {
                anyhow::ensure!(
                    source.join("pki").join(name).is_file(),
                    "备份缺少身份文件 {name}"
                );
            }
            let mut tx = db.pool.begin().await?;
            for record in records {
                let bucket = record["bucket"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("记录格式错误"))?;
                if matches!(bucket, "sessions" | "enrollment") {
                    continue;
                }
                let id = record["id"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("记录编号错误"))?;
                let value = record["value"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("记录内容错误"))?;
                sqlx::query("INSERT INTO records(bucket,id,value) VALUES(?,?,?)")
                    .bind(bucket)
                    .bind(id)
                    .bind(value)
                    .execute(&mut *tx)
                    .await?;
            }
            for task in tasks {
                sqlx::query(
                    "INSERT INTO tasks(id,node_id,task_key,digest,value) VALUES(?,?,?,?,?)",
                )
                .bind(&task.id)
                .bind(&task.node_id)
                .bind(&task.key)
                .bind(&task.digest)
                .bind(serde_json::to_string(&task)?)
                .execute(&mut *tx)
                .await?;
            }
            std::fs::create_dir_all(args.data_dir.join("pki"))?;
            for name in ["ca.pem", "ca-key.pem", "server.pem", "server-key.pem"] {
                pki::private_write(
                    &args.data_dir.join("pki").join(name),
                    std::fs::read(source.join("pki").join(name))?,
                )?;
            }
            plugins::copy_state(&source.join("plugins"), &args.data_dir.join("plugins"))?;
            tx.commit().await?;
            println!(
                "主控恢复完成；请保持原主控停用，并将原管理地址指向恢复设备。管理员需重新登录。"
            );
        }
        Command::Serve {
            listen,
            agent_listen,
            health_listen,
            origin,
            web,
        } => {
            anyhow::ensure!(
                db.get::<String>("auth", "admin").await?.is_some(),
                "请先初始化管理员"
            );
            let entrance_value = db
                .get::<String>(ENTRANCE_BUCKET, ENTRANCE_ID)
                .await?
                .ok_or_else(|| {
                    anyhow::anyhow!("控制库缺少安全入口；请在初始化时生成，或手工写入后再启动")
                })?;
            entrance::validate(&entrance_value)?;
            let entrance: entrance::Shared =
                Arc::new(std::sync::RwLock::new(entrance_value.clone()));
            let (events, _) = broadcast::channel(512);
            let (streams, _) = broadcast::channel(256);
            let state = State {
                db,
                dir: args.data_dir.clone(),
                origin,
                channels: Default::default(),
                events,
                streams,
                gate: Default::default(),
                network_gate: Default::default(),
                login_limits: Default::default(),
                entrance: entrance.clone(),
                latest_metrics: Default::default(),
                web: web.clone(),
                host_inventories: Default::default(),
                db_reports: Default::default(),
                plugin_runtimes: Default::default(),
                plugin_tokens: Default::default(),
                plugin_management: Default::default(),
                plugin_socket_dir: Arc::new(tempfile::Builder::new().prefix("opsd-").tempdir()?),
            };
            for mut task in state
                .db
                .tasks()
                .await?
                .into_iter()
                .filter(|t| !t.status.terminal() && t.action.legacy_business())
            {
                task.status = TaskStatus::Blocked;
                task.sequence += 1;
                task.updated_at = now();
                task.error = Some("业务已迁入插件，旧任务停止自动重放，请核实实际状态".into());
                state.db.save_task(&task).await?;
            }
            plugins::start(state.clone()).await?;
            let directory = state.clone();
            tokio::spawn(async move {
                let mut tick = tokio::time::interval(std::time::Duration::from_secs(60));
                loop {
                    tick.tick().await;
                    if let Err(error) = network::settle(&directory).await {
                        tracing::warn!(%error,"旧来源白名单收敛暂未完成");
                    }
                    let _lock = directory.gate.lock().await;
                    if let Err(error) = network::synchronize(&directory).await {
                        tracing::error!(%error,"网络状态同步失败");
                    }
                    if let Err(error) = api::synchronize_peers(&directory).await {
                        tracing::error!(%error, "节点地址任务对账失败，将重试");
                    }
                    // 探测目标是节点目录的另一个投影，同样要按周期对账
                    if let Err(error) = api::synchronize_probes(&directory).await {
                        tracing::error!(%error, "对端探测目标对账失败，将重试");
                    }
                }
            });
            // 保留策略独立于采样，按小时检查一次即可。
            let retention = state.clone();
            tokio::spawn(async move {
                let mut tick = tokio::time::interval(std::time::Duration::from_secs(3600));
                loop {
                    tick.tick().await;
                    if let Err(error) = metrics::prune(&retention.db).await {
                        tracing::warn!(%error, "指标保留策略执行失败");
                    }
                    if let Err(error) = audit::prune(&retention.db).await {
                        tracing::warn!(%error, "审计保留策略执行失败");
                    }
                }
            });
            let private = Router::new()
                .route("/api/v1/auth/me", get(auth::me))
                .route("/api/v1/auth/logout", post(auth::logout))
                .route("/api/v1/nodes", get(api::nodes))
                .route(
                    "/api/v1/network",
                    get(network::read).put(network::controller),
                )
                .route("/api/v1/plugins", get(plugins::list))
                .route("/api/v1/plugins/catalog", get(plugins::catalog))
                .route("/api/v1/plugins/{plugin}/install", post(plugins::install_official))
                .route(
                    "/api/v1/plugins/{plugin}/{version}",
                    delete(plugins::uninstall),
                )
                .route(
                    "/api/v1/plugins/{plugin}/stream/{*path}",
                    get(plugins::stream),
                )
                .route("/api/v1/plugins/{plugin}/selection", put(plugins::select))
                .route(
                    "/api/v1/plugins/{plugin}/configuration",
                    get(plugins::configuration).put(plugins::configure),
                )
                .route(
                    "/api/v1/plugins/{plugin}/api/{*path}",
                    axum::routing::any(plugins::gateway),
                )
                .route("/plugins/{plugin}/{*path}", get(plugins::asset))
                .route(
                    "/api/v1/nodes/{id}",
                    put(api::update_node).delete(api::revoke),
                )
                .route(
                    "/api/v1/enrollment-tokens",
                    get(api::enrollments).post(api::token),
                )
                .route(
                    "/api/v1/enrollment-tokens/{node_id}",
                    delete(api::revoke_enrollment),
                )
                .route("/api/v1/peer-addresses", get(api::peers))
                .route("/api/v1/nodes/{id}/actions", post(api::action))
                .route("/api/v1/nodes/{id}/stream", get(channel::browser_stream))
                .route("/api/v1/tasks", get(api::tasks))
                .route("/api/v1/tasks/{id}", get(api::task))
                .route("/api/v1/tasks/{id}/events", get(api::task_events))
                .route("/api/v1/events", get(api::events))
                .route("/api/v1/metrics/overview", get(channel::metrics_overview))
                .route("/api/v1/metrics/status", get(channel::metrics_status))
                .route("/api/v1/nodes/{id}/metrics", get(channel::metrics_history))
                .route(
                    "/api/v1/export/keys",
                    get(export::keys).post(export::create),
                )
                .route("/api/v1/export/keys/{id}", delete(export::revoke))
                .route("/api/v1/themes", get(theme::list_themes))
                .route(
                    "/api/v1/themes/active",
                    get(theme::read_active).put(theme::activate),
                )
                .route("/api/v1/themes/console", post(theme::install_console_theme))
                .route(
                    "/api/v1/themes/console/repository/resolve",
                    post(theme::resolve_repository),
                )
                .route(
                    "/api/v1/themes/console/repository/install",
                    post(theme::install_repository),
                )
                .route("/api/v1/themes/{short}", delete(theme::remove_theme))
                .route("/api/v1/audit", get(audit::query))
                .route("/api/v1/nodes/{id}/host-inspect", post(host::host_inspect))
                // 数据库只读运维视图：巡检下发 + 集中结论
                .route("/api/v1/database/overview", get(database::overview))
                .route("/api/v1/nodes/{id}/db-inspect", post(database::db_inspect))
                // 文件只读操作走流通道；变更操作统一走任务机制
                .route("/api/v1/nodes/{id}/files", post(api::file_action))
                .route(
                    "/api/v1/settings/entrance",
                    get(api::read_entrance).put(api::update_entrance),
                )
                .route_layer(middleware::from_fn_with_state(state.clone(), auth::require));
            // 控制台与 Agent 共用的注册端点挂在公开面；它同样位于安全入口之下。
            let public = Router::new().route("/api/v1/auth/login", post(auth::login));
            // Agent 首次注册发生在它拿到客户端证书之前，无法走 mTLS 通道，
            // 因此单独挂在入口之外，并列入门禁白名单。凭据是一次性注册令牌与 CA 指纹核对。
            let agent_public = Router::new()
                .route("/api/v1/enroll", post(api::enroll))
                .with_state(state.clone());
            // 浏览器面：静态资源与全部 API 统一挂在 /{安全入口} 之下，
            // 门禁在最外层按首段路径校验，因此这里不再单独处理路径前缀。
            let console =
                Router::new()
                    .merge(public)
                    .merge(private)
                    .route("/frontend/{short}/", get(theme::console_frontend_index))
                    .route(
                        "/frontend/{short}/{*path}",
                        get(theme::console_frontend_asset),
                    )
                    .nest_service(
                        "/assets",
                        tower_http::services::ServeDir::new(web.join("assets")),
                    )
                    .fallback(
                        |axum::extract::State(s): axum::extract::State<State>,
                         uri: axum::http::Uri| async move {
                            if uri.path().starts_with("/api/") {
                                axum::response::IntoResponse::into_response(ApiError(
                                    axum::http::StatusCode::NOT_FOUND,
                                    "接口不存在或已迁入插件".into(),
                                ))
                            } else {
                                theme::console_index(axum::extract::State(s)).await
                            }
                        },
                    )
                    .layer(axum::extract::DefaultBodyLimit::max(1024 * 1024))
                    .with_state(state.clone());
            // 主题包上传需要更大的体积上限。它必须在全局 1 MB 限制**之后**合并，
            // 否则会被那条限制覆盖，导致稍大的主题包直接被 413 拒绝。
            let theme_upload = Router::new()
                .route(
                    "/api/v1/themes/console/package",
                    post(theme::install_console_package),
                )
                .route_layer(middleware::from_fn_with_state(state.clone(), auth::require))
                .layer(axum::extract::DefaultBodyLimit::max(
                    theme::MAX_PACKAGE_BYTES,
                ))
                .with_state(state.clone());
            let plugin_upload = Router::new()
                .route("/api/v1/plugins/package", post(plugins::install))
                .route_layer(middleware::from_fn_with_state(state.clone(), auth::require))
                .layer(axum::extract::DefaultBodyLimit::max(128 * 1024 * 1024))
                .with_state(state.clone());
            let module_upload = Router::new()
                .route("/api/v1/plugins/{plugin}/package", post(plugins::install_target))
                .route_layer(middleware::from_fn_with_state(state.clone(), auth::require))
                .layer(axum::extract::DefaultBodyLimit::max(256 * 1024 * 1024))
                .with_state(state.clone());
            let console = console.merge(theme_upload).merge(plugin_upload).merge(module_upload);
            let prefix = format!("/{entrance_value}");
            let trimmed = prefix.clone();
            // nest 不会匹配裸的 `/{入口}/`，因此显式补一条根路由；
            // 单页应用使用查询串路由，未知路径回退到 index.html。
            let index_state = state.clone();
            let export_routes = Router::new()
                .route("/api/v1/public/nodes", get(export::nodes))
                .route("/api/v1/public/summary", get(export::summary))
                .route("/api/v1/public/metrics", get(export::metrics))
                .with_state(state.clone());
            let app = Router::new()
                .merge(agent_public)
                .nest(&prefix, console)
                // 分享面挂在入口之外，凭自己的令牌放行；门禁按令牌是否有效决定是否响应。
                .merge(export_routes)
                .route(
                    &prefix,
                    get(move || {
                        let target = format!("{trimmed}/");
                        async move { axum::response::Redirect::permanent(&target) }
                    }),
                )
                .route(
                    &format!("{prefix}/"),
                    get(move || theme::console_index(axum::extract::State(index_state.clone()))),
                );
            // 健康检查独立监听，只绑定回环地址，不经入口门禁。
            let health = Router::new().route(
                "/api/v1/health",
                get(|| async { axum::Json(serde_json::json!({"status":"ok"})) }),
            );
            let agent = Router::new()
                .route("/agent", get(channel::connect))
                .route("/status", get(channel::status))
                .route(
                    "/plugin-packages/{plugin}/{version}/{arch}",
                    get(plugins::node_binary),
                )
                .route("/plugin-context/{task_id}", post(plugins::node_context))
                .route("/renew", post(api::renew))
                .route("/verify", post(api::verify))
                .with_state(state.clone());
            tokio::spawn(channel::dispatch(state));
            let pki_dir = args.data_dir.join("pki");
            let gate = transport::Gate {
                entrance,
                limits: Default::default(),
                allow: entrance::AGENT_PATHS
                    .iter()
                    .chain(entrance::EXPORT_PATHS.iter())
                    .map(|p| (*p).to_owned())
                    .collect(),
            };
            tracing::info!(entrance=%entrance_value,"控制台已启用安全入口");
            tokio::try_join!(
                transport::serve(listen, &pki_dir, false, app, Some(gate)),
                transport::serve(agent_listen, &pki_dir, true, agent, None),
                transport::serve_plain(health_listen, health)
            )?;
        }
        // 健康检查已在获取主控锁之前返回，这里只是为穷尽性检查保留分支。
        Command::Probe { .. } => unreachable!("健康检查在主控锁之前处理"),
    };
    Ok(())
}

impl State {
    /// 当前安全入口。用于把会话 Cookie 的 Path 限定在控制台之下。
    pub fn entrance_path(&self) -> String {
        self.entrance
            .read()
            .map(|value| value.clone())
            .unwrap_or_default()
    }
}
#[derive(Debug)]
pub struct ApiError(pub axum::http::StatusCode, pub String);
impl axum::response::IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        (self.0, axum::Json(serde_json::json!({"error":self.1}))).into_response()
    }
}
impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        Self(axum::http::StatusCode::BAD_REQUEST, e.to_string())
    }
}
pub type ApiResult<T> = std::result::Result<T, ApiError>;
pub fn bad(s: &str) -> ApiError {
    ApiError(axum::http::StatusCode::BAD_REQUEST, s.into())
}
pub fn check(condition: bool, message: &str) -> ApiResult<()> {
    if condition { Ok(()) } else { Err(bad(message)) }
}

impl From<std::io::Error> for ApiError {
    fn from(error: std::io::Error) -> Self {
        bad(&error.to_string())
    }
}
impl From<serde_json::Error> for ApiError {
    fn from(error: serde_json::Error) -> Self {
        bad(&error.to_string())
    }
}
impl From<zip::result::ZipError> for ApiError {
    fn from(error: zip::result::ZipError) -> Self {
        bad(&error.to_string())
    }
}
