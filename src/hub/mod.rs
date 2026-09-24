mod api;
mod audit;
mod auth;
mod channel;
mod database;
mod metrics;
mod share;
mod storage;
mod storage_cluster;
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
    pub login_limits: Arc<Mutex<HashMap<String, (i64, u32)>>>,
    /// 运行期可改的安全入口，由门禁直接读取。
    pub entrance: entrance::Shared,
    /// 各节点最新一次指标，属于有界内存缓存；历史在控制库中分层保存。
    pub latest_metrics: Arc<RwLock<HashMap<String, metrics::NodeMetrics>>>,
    /// 分享面的活跃令牌索引，由门禁直接读取。
    pub share_index: share::SharedIndex,
    /// 分享面开关与站点信息。
    pub share_settings: share::SharedSettings,
    /// 构建产物根目录，用于提供分享页与其静态资源。
    pub web: PathBuf,
    /// 各节点最近一次主机盘点，供存储预检做集中分析。
    pub host_inventories: storage::SharedInventories,
    /// 各节点最近一次数据库只读巡检结果。同样只留在内存里：
    /// 它体积不大但变化很快，落库只会得到一份过期的运维视图。
    pub db_reports: database::SharedDbReports,
}
type NodeChannels = HashMap<String, (String, mpsc::Sender<Frame>)>;
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
            // 分享面配置随主控启动加载一次，之后由管理接口刷新。
            let share_index: share::SharedIndex =
                Arc::new(std::sync::RwLock::new(share::Index::load(&db).await?));
            let share_enabled = db
                .get::<bool>(share::SETTINGS_BUCKET, share::ENABLED_ID)
                .await?
                .unwrap_or(false);
            let share_site = db
                .get::<share::SitePublic>(share::SITE_BUCKET, share::SITE_ID)
                .await?
                .unwrap_or_default();
            let share_settings: share::SharedSettings =
                Arc::new(std::sync::RwLock::new(share::Settings {
                    enabled: share_enabled,
                    site: share_site,
                }));
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
                login_limits: Default::default(),
                entrance: entrance.clone(),
                latest_metrics: Default::default(),
                share_index: share_index.clone(),
                share_settings: share_settings.clone(),
                web: web.clone(),
                host_inventories: Default::default(),
                db_reports: Default::default(),
            };
            let directory = state.clone();
            tokio::spawn(async move {
                let mut tick = tokio::time::interval(std::time::Duration::from_secs(60));
                loop {
                    tick.tick().await;
                    let _lock = directory.gate.lock().await;
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
                    "/api/v1/nodes/{id}",
                    put(api::update_node).delete(api::revoke),
                )
                .route("/api/v1/enrollment-tokens", post(api::token))
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
                    "/api/v1/share/settings",
                    get(share::read_settings).put(share::update_settings),
                )
                .route(
                    "/api/v1/share/tokens",
                    get(share::list_tokens).post(share::create_token),
                )
                .route("/api/v1/share/tokens/{id}", delete(share::revoke_token))
                .route(
                    "/api/v1/share/keys",
                    get(share::list_keys).post(share::create_key),
                )
                .route("/api/v1/share/keys/{id}", delete(share::revoke_key))
                .route("/api/v1/themes", get(theme::list_themes))
                .route(
                    "/api/v1/themes/active",
                    get(theme::read_active).put(theme::activate),
                )
                .route("/api/v1/themes/console", post(theme::install_console_theme))
                .route("/api/v1/themes/{short}", delete(theme::remove_theme))
                .route("/api/v1/audit", get(audit::query))
                .route("/api/v1/storage/readiness", get(storage::readiness))
                .route(
                    "/api/v1/nodes/{id}/host-inspect",
                    post(storage::host_inspect),
                )
                // 存储集群：定义 → 计划 → 执行。执行含不可逆步骤，因此单独确认。
                .route(
                    "/api/v1/storage/clusters",
                    get(storage_cluster::clusters).put(storage_cluster::put_cluster),
                )
                .route("/api/v1/storage/plans", post(storage_cluster::plan))
                .route(
                    "/api/v1/storage/plans/{id}",
                    get(storage_cluster::read_plan),
                )
                .route("/api/v1/storage/plans/apply", post(storage_cluster::apply))
                .route("/api/v1/storage/status", post(storage_cluster::status))
                .route("/api/v1/storage/buckets", post(storage_cluster::bucket))
                .route("/api/v1/storage/users", post(storage_cluster::user))
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
            let console = Router::new()
                .merge(public)
                .merge(private)
                .fallback_service(tower_http::services::ServeDir::new(&web).not_found_service(
                    tower_http::services::ServeFile::new(web.join("index.html")),
                ))
                .layer(axum::extract::DefaultBodyLimit::max(1024 * 1024))
                .with_state(state.clone());
            // 主题包上传需要更大的体积上限。它必须在全局 1 MB 限制**之后**合并，
            // 否则会被那条限制覆盖，导致稍大的主题包直接被 413 拒绝。
            let theme_upload = Router::new()
                .route("/api/v1/themes/share", post(theme::install_share_theme))
                .route_layer(middleware::from_fn_with_state(state.clone(), auth::require))
                .layer(axum::extract::DefaultBodyLimit::max(
                    theme::MAX_PACKAGE_BYTES,
                ))
                .with_state(state.clone());
            let console = console.merge(theme_upload);
            let prefix = format!("/{entrance_value}");
            let trimmed = prefix.clone();
            // nest 不会匹配裸的 `/{入口}/`，因此显式补一条根路由；
            // 单页应用使用查询串路由，未知路径回退到 index.html。
            let index = tower_http::services::ServeFile::new(web.join("index.html"));
            // 分享面：页面、页面数据与机器接口。全部位于 /share/{令牌}/ 之下。
            let share_routes = Router::new()
                // 缺结尾斜杠时补上，否则页面里的相对资源路径会丢掉令牌那一层
                .route(
                    "/share/{token}",
                    get(|Path(token): Path<String>| async move {
                        axum::response::Redirect::permanent(&format!("/share/{token}/"))
                    }),
                )
                .route("/share/{token}/", get(share::share_index_page))
                .route("/share/{token}/assets/{*path}", get(share::share_asset))
                // 包级主题的资源与公开设置
                .route(
                    "/share/{token}/theme/{*path}",
                    get(theme::share_theme_asset),
                )
                .route("/share/{token}/data/theme", get(theme::share_theme))
                .route("/share/{token}/data/summary", get(share::data_summary))
                .route("/share/{token}/data/nodes", get(share::data_nodes))
                .route("/share/{token}/data/records", get(share::data_records))
                .route(
                    "/share/{token}/api/v1/public/summary",
                    get(share::public_summary),
                )
                .route(
                    "/share/{token}/api/v1/public/nodes",
                    get(share::public_nodes),
                )
                .route(
                    "/share/{token}/api/v1/public/recent/{id}",
                    get(share::public_recent),
                )
                .route(
                    "/share/{token}/api/v1/public/records",
                    get(share::public_records),
                )
                .route(
                    "/share/{token}/api/v1/public/metrics",
                    get(share::public_metrics),
                )
                .with_state(state.clone());
            let app = Router::new()
                .merge(agent_public)
                .nest(&prefix, console)
                // 分享面挂在入口之外，凭自己的令牌放行；门禁按令牌是否有效决定是否响应。
                .merge(share_routes)
                .route(
                    &prefix,
                    get(move || {
                        let target = format!("{trimmed}/");
                        async move { axum::response::Redirect::permanent(&target) }
                    }),
                )
                .route(&format!("{prefix}/"), axum::routing::get_service(index));
            // 健康检查独立监听，只绑定回环地址，不经入口门禁。
            let health = Router::new().route(
                "/api/v1/health",
                get(|| async { axum::Json(serde_json::json!({"status":"ok"})) }),
            );
            let agent = Router::new()
                .route("/agent", get(channel::connect))
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
                    .map(|p| (*p).to_owned())
                    .collect(),
                // 分享面按令牌放行；令牌无效时门禁直接丢弃，不产生响应。
                share: Some({
                    let index = share_index.clone();
                    std::sync::Arc::new(move |token: &str| {
                        index
                            .read()
                            .map(|index| index.find(token).is_some())
                            .unwrap_or(false)
                    })
                }),
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
