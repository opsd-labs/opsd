//! 数据库只读巡检（Galera / 主机侧 / ProxySQL / 备份链路）。
//!
//! # 三条硬约束
//!
//! 1. **只读**。语句全部是代码里的常量，`Action::DbInspect` **不接受任何参数**，
//!    因此没有任何 SQL 会经过网络，也就不存在把这条通道变成任意查询通道的可能。
//!    不建库、不建用户、不改配置、不触发 SST、不 kill 查询。
//! 2. **凭据不出本机**。只读账号复用既有运维脚本已经建好的账号，
//!    密码由 Agent 从运维自己的 0600 文件里读，只通过子进程环境传给容器内的客户端，
//!    既不上传主控，也不进审计。密码文件权限不合格时**拒绝读取**。
//! 3. **连不上就是未知**。任何一部分失败都产出「未知 + 原因」。
//!    绝不能把采集失败补成 0 或"没有告警"——那正是运维视图最危险的假象。
//!
//! 为什么经 `docker exec` 而不是直连数据库：只读账号是按 `'user'@'127.0.0.1'` 授权的，
//! 从宿主机走 TCP 连进来源地址是网桥地址，根本匹配不上。既有运维脚本
//! （`audit-proxysql.sh`、`proxysql-textfile-metrics.sh`）用的也是同一手法。
use anyhow::{Context, Result};
use opsd::protocol::*;
use serde::Deserialize;
use std::path::{Path, PathBuf};

use super::command_env;

/// 默认客户端容器名，与既有运维脚本一致。
const DEFAULT_CONTAINER: &str = "mariadb-galera";
/// 容器内的客户端可执行文件。
const CLIENT: &str = "mariadb";
/// 备份根目录，与 `galera-backup-common.sh` 一致。
const DEFAULT_BACKUP_ROOT: &str = "/data1/server/db/backups";
/// 期望的集群成员数，与告警规则 `GaleraClusterSizeMismatch` 一致。
const DEFAULT_CLUSTER_SIZE: u64 = 5;
/// 单个语句的超时。
const QUERY_TIMEOUT: u64 = 20;
/// 查询摘要最多带回多少条。
const MAX_DIGESTS: usize = 20;
/// 单个 .age 归档只读前若干字节做头部判断，不整份读取。
const AGE_HEADER: &[u8] = b"age-encryption.org/v1";

/// 巡检用的只读账号。
#[derive(Debug, Clone, Deserialize)]
pub struct Account {
    pub user: String,
    #[serde(default = "loopback")]
    pub host: String,
    pub port: u16,
    /// 存放密码的文件。必须是 0600 的普通文件。
    pub password_file: String,
    /// 密码在该文件里的键名，例如 `EXPORTER_PASSWORD`。
    pub password_key: String,
}

fn loopback() -> String {
    "127.0.0.1".into()
}

/// 本机巡检配置，放在 Agent 数据目录下的 `db-inspect.json`。
///
/// 它只保存**路径与账号名**，不保存任何密码：密码留在运维原有的秘密文件里，
/// 因此不需要第二套秘密分发机制，也不会出现"两处密码不一致"。
#[derive(Debug, Clone, Deserialize)]
pub struct DbConfig {
    #[serde(default = "default_container")]
    pub container: String,
    /// Galera 成员自身的只读账号（3306）。
    #[serde(default)]
    pub galera: Option<Account>,
    /// ProxySQL 管理口的只读账号（6032）。
    #[serde(default)]
    pub proxysql: Option<Account>,
    #[serde(default = "default_backup_root")]
    pub backup_root: String,
    /// 期望的集群成员数，用于判断"本节点看到的集群大小是否正常"。
    #[serde(default = "default_cluster_size")]
    pub expected_cluster_size: u64,
}

fn default_container() -> String {
    DEFAULT_CONTAINER.into()
}
fn default_backup_root() -> String {
    DEFAULT_BACKUP_ROOT.into()
}
fn default_cluster_size() -> u64 {
    DEFAULT_CLUSTER_SIZE
}

/// 读取本机巡检配置。文件不存在即「未配置」，这不是错误。
pub fn load_config(dir: &Path) -> Result<Option<DbConfig>> {
    let path = dir.join("db-inspect.json");
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("读取巡检配置失败：{}", path.display()))?;
    let config: DbConfig = serde_json::from_str(&text)
        .with_context(|| format!("巡检配置不是合法 JSON：{}", path.display()))?;
    Ok(Some(config))
}

/// 从 `KEY=value` 形式的秘密文件里取一个键。
///
/// 只做最小解析：忽略空行与 `#` 注释，不展开变量、不执行任何内容。
pub fn parse_secret(text: &str, key: &str) -> Option<String> {
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        if name.trim() == key {
            let value = value.trim().trim_matches('"').trim_matches('\'');
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

/// 读密码。**权限不合格就拒绝**，因为把只读账号密码交给一个别人也能读的文件，
/// 等于把这台机器的数据库打开了一半。
///
/// 只检查「组与其他用户不可读写」，不要求属主是 root：秘密文件由谁创建是运维的选择，
/// 但 0644 是不可接受的。
pub fn read_password(account: &Account) -> Result<String> {
    let path = Path::new(&account.password_file);
    let metadata = std::fs::metadata(path)
        .with_context(|| format!("读取密码文件失败：{}", path.display()))?;
    anyhow::ensure!(
        metadata.is_file(),
        "密码文件必须是普通文件：{}",
        path.display()
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        anyhow::ensure!(
            metadata.mode() & 0o077 == 0,
            "密码文件权限过于宽松（{:o}），请改为 0600：{}",
            metadata.mode() & 0o777,
            path.display()
        );
    }
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("读取密码文件失败：{}", path.display()))?;
    parse_secret(&text, &account.password_key).with_context(|| {
        format!(
            "密码文件里没有 {}：{}",
            account.password_key,
            path.display()
        )
    })
}

/* ------------------------------------------------------------------ *
 * 固定语句
 *
 * 这些字符串就是"只读通道"的边界：主控无法影响它们。
 * 与既有运维脚本（galera-backup-common.sh、audit-galera-node.sh）保持同一组变量，
 * 因此控制台的结论与现场脚本、告警规则看到的是同一批事实。
 * ------------------------------------------------------------------ */

/// 用 `CONCAT_WS('=', ...)` 统一成 `名字=值` 行，避免列数与制表符解析的歧义。
const SQL_GALERA: &str = "SELECT CONCAT_WS('=', VARIABLE_NAME, VARIABLE_VALUE) \
FROM information_schema.GLOBAL_STATUS WHERE VARIABLE_NAME IN (\
'WSREP_CLUSTER_STATUS','WSREP_CLUSTER_SIZE','WSREP_LOCAL_STATE','WSREP_LOCAL_STATE_COMMENT',\
'WSREP_READY','WSREP_CONNECTED','WSREP_DESYNC','WSREP_FLOW_CONTROL_PAUSED',\
'WSREP_FLOW_CONTROL_PAUSED_NS','WSREP_LAST_COMMITTED','WSREP_LOCAL_RECV_QUEUE',\
'WSREP_LOCAL_SEND_QUEUE','WSREP_LOCAL_CERT_FAILURES','WSREP_LOCAL_BF_ABORTS',\
'WSREP_CLUSTER_STATE_UUID','WSREP_LOCAL_STATE_UUID','WSREP_SST_DONOR',\
'WSREP_PROVIDER_VERSION','WSREP_INCOMING_ADDRESSES','WSREP_REJECT_QUERIES',\
'WSREP_SST_DONOR_REJECTS_QUERIES') ORDER BY VARIABLE_NAME;";

const SQL_HOST: &str = "SELECT CONCAT_WS('=', VARIABLE_NAME, VARIABLE_VALUE) \
FROM information_schema.GLOBAL_STATUS WHERE VARIABLE_NAME IN \
('UPTIME','THREADS_CONNECTED','THREADS_RUNNING');";

const SQL_IDENTITY: &str = "SELECT CONCAT_WS('=', 'VERSION', VERSION()), \
CONCAT_WS('=', 'VERSION_COMMENT', @@version_comment), \
CONCAT_WS('=', 'HOSTNAME', @@hostname), \
CONCAT_WS('=', 'SERVER_ID', @@server_id), \
CONCAT_WS('=', 'READ_ONLY', @@read_only);";

const SQL_BINLOG: &str = "SHOW MASTER STATUS;";

const SQL_PROXYSQL_UPTIME: &str =
    "SELECT Variable_Value FROM stats_mysql_global WHERE Variable_Name='ProxySQL_Uptime';";

const SQL_PROXYSQL_SERVERS: &str = "SELECT hostgroup_id, hostname, port, status, weight, \
max_connections FROM runtime_mysql_servers ORDER BY hostgroup_id, weight DESC, hostname;";

const SQL_PROXYSQL_POOLS: &str =
    "SELECT hostgroup, srv_host, srv_port, ConnUsed, ConnFree, ConnOK, ConnERR, Queries, \
Bytes_data_sent, Bytes_data_recv, Latency_us FROM stats_mysql_connection_pool \
ORDER BY hostgroup, srv_host;";

/// **不查 `digest_text`**：那里面是真实 SQL，可能带着用户名、邮箱、令牌等值。
/// 运维视图只需要知道"哪类语句最重"，摘要哈希足够用于跨节点比对。
const SQL_PROXYSQL_DIGESTS: &str = "SELECT hostgroup, schemaname, digest, count_star, \
sum_time, max_time FROM stats_mysql_query_digest ORDER BY sum_time DESC LIMIT 20;";

/// 一次只读巡检。
pub async fn inspect(dir: &Path) -> DbInspectReport {
    let at = now();
    let config = match load_config(dir) {
        Ok(Some(config)) => config,
        Ok(None) => {
            let reason = "本机未配置数据库只读巡检账号（缺 db-inspect.json）";
            return DbInspectReport {
                collected_at: at,
                configured: false,
                galera: DbSection::unknown(reason),
                host: DbSection::unknown(reason),
                proxysql: DbSection::unknown(reason),
                backup: DbSection::unknown(reason),
                gaps: vec![reason.into()],
            };
        }
        Err(error) => {
            let reason = format!("{error:#}");
            return DbInspectReport {
                collected_at: at,
                configured: false,
                galera: DbSection::unknown(reason.clone()),
                host: DbSection::unknown(reason.clone()),
                proxysql: DbSection::unknown(reason.clone()),
                backup: DbSection::unknown(reason.clone()),
                gaps: vec![reason],
            };
        }
    };

    let mut gaps = Vec::new();
    let client = Client::new(&config);

    // 凭据每个分区只读一次：这样"权限不合格"只会被报告一次，也少一次读秘密文件。
    let galera_account = client.credentials(config.galera.as_ref());
    let proxysql_account = client.credentials(config.proxysql.as_ref());

    // ---- Galera ----
    let galera = match &galera_account {
        Err(reason) => DbSection::unknown(reason.clone()),
        Ok((account, password)) => match client.query(account, password, SQL_GALERA).await {
            Err(error) => {
                let reason = format!("{error:#}");
                gaps.push(format!("Galera 状态：{reason}"));
                DbSection::unknown(reason)
            }
            Ok(output) => match galera_from_rows(&parse_pairs(&output)) {
                None => DbSection::unknown("查询有返回，但没有 wsrep 变量：本机可能不是 Galera 节点"),
                Some(mut state) => {
                    state.expected_cluster_size = config.expected_cluster_size;
                    DbSection::ok(state)
                }
            },
        },
    };

    // ---- 主机侧：身份与状态分成两次查询，一次失败不会让另一次也归零 ----
    let mut host = DbHostState::default();
    let mut host_problem: Option<String> = None;
    match &galera_account {
        Err(reason) => host_problem = Some(reason.clone()),
        Ok((account, password)) => {
            match client.query(account, password, SQL_IDENTITY).await {
                Ok(output) => apply_identity(&mut host, &parse_pairs(&output)),
                Err(error) => host_problem = Some(format!("{error:#}")),
            }
            match client.query(account, password, SQL_HOST).await {
                Ok(output) => apply_host_status(&mut host, &parse_pairs(&output)),
                Err(error) => {
                    host_problem.get_or_insert(format!("{error:#}"));
                }
            };
            // binlog 位点是独立的可选事实：没开 binlog 时为空，这不是错误
            if let Ok(output) = client.query(account, password, SQL_BINLOG).await {
                let (file, position) = parse_binlog(&output);
                host.binlog_file = file;
                host.binlog_position = position;
            }
        }
    }
    let host_section = match host_problem {
        Some(reason) => {
            gaps.push(format!("主机侧状态：{reason}"));
            DbSection::unknown(reason)
        }
        None => DbSection::ok(host),
    };

    // ---- ProxySQL ----
    let mut proxysql = ProxySqlState::default();
    let mut proxysql_problem: Option<String> = None;
    match &proxysql_account {
        Err(reason) => proxysql_problem = Some(reason.clone()),
        Ok((account, password)) => {
            match client.query(account, password, SQL_PROXYSQL_UPTIME).await {
                Ok(output) => proxysql.uptime = output.trim().parse::<u64>().ok(),
                Err(error) => proxysql_problem = Some(format!("{error:#}")),
            }
            if proxysql_problem.is_none() {
                match client.query(account, password, SQL_PROXYSQL_SERVERS).await {
                    Ok(output) => proxysql.servers = parse_servers(&output),
                    Err(error) => proxysql_problem = Some(format!("{error:#}")),
                }
            }
            if proxysql_problem.is_none() {
                // 池与延迟是展示项，取不到只是少一块信息，不把整个分区降级为未知
                if let Ok(output) = client.query(account, password, SQL_PROXYSQL_POOLS).await {
                    proxysql.pools = parse_pools(&output);
                }
                if let Ok(output) = client.query(account, password, SQL_PROXYSQL_DIGESTS).await {
                    proxysql.digests = parse_digests(&output);
                }
            }
        }
    }
    if proxysql_problem.is_none() {
        // 池里的延迟按主机/端口合并到后端列表上：后端表本身没有延迟列
        merge_latency(&mut proxysql);
        proxysql.writer_online = Some(count_online(&proxysql.servers, 10));
        proxysql.backup_writer_online = Some(count_online(&proxysql.servers, 20));
    }
    let proxysql = match proxysql_problem {
        Some(reason) => {
            gaps.push(format!("ProxySQL：{reason}"));
            DbSection::unknown(reason)
        }
        None => DbSection::ok(proxysql),
    };

    // ---- 备份链路 ----
    let backup = match backup_state(&config.backup_root, at).await {
        Ok(state) => DbSection::ok(state),
        Err(error) => {
            let reason = format!("{error:#}");
            gaps.push(format!("备份链路：{reason}"));
            DbSection::unknown(reason)
        }
    };

    DbInspectReport {
        collected_at: at,
        configured: true,
        galera,
        host: host_section,
        proxysql,
        backup,
        gaps,
    }
}

/// 容器内的客户端调用封装。
struct Client {
    container: String,
    /// 是否还能靠"不带值的 `--env` 转发"传密码。
    ///
    /// `docker run --env VAR`（不带 `=`）会从 docker 客户端自身的环境里取值转发，
    /// 这是文档化的行为；`docker exec` 是否同样如此没有把握，各版本也不一致。
    /// 因此先试不带值的形式——它**不会把密码写进任何命令行**；
    /// 一旦失败就退回显式赋值（`--env MYSQL_PWD=<值>`），
    /// 代价是密码会短暂出现在 docker 客户端的命令行里。
    forward_env: std::sync::atomic::AtomicBool,
}

impl Client {
    fn new(config: &DbConfig) -> Self {
        Self {
            container: config.container.clone(),
            forward_env: std::sync::atomic::AtomicBool::new(true),
        }
    }

    /// 取账号与密码。没配置账号、密码文件权限不合规、键名不存在，
    /// 都在这里变成「未知」的原因，而不是一个空值。
    fn credentials(
        &self,
        account: Option<&Account>,
    ) -> std::result::Result<(Account, String), String> {
        let account = account
            .ok_or_else(|| "本机未配置这一部分的只读巡检账号".to_string())?
            .clone();
        let password = read_password(&account).map_err(|error| format!("{error:#}"))?;
        Ok((account, password))
    }

    async fn query(&self, account: &Account, password: &str, sql: &str) -> Result<String> {
        use std::sync::atomic::Ordering;
        if self.forward_env.load(Ordering::Relaxed) {
            match self.run(account, password, sql, true).await {
                Ok(output) => return Ok(output),
                // 转发不可用（或这次真的失败）：改用显式赋值再试一次
                Err(_) => self.forward_env.store(false, Ordering::Relaxed),
            }
        }
        self.run(account, password, sql, false).await
    }

    async fn run(
        &self,
        account: &Account,
        password: &str,
        sql: &str,
        forward: bool,
    ) -> Result<String> {
        let args = client_args(&self.container, account, password, sql, forward);
        let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();
        let env: Vec<(String, String)> = client_env(password, forward);
        let borrowed_env: Vec<(&str, &str)> = env
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
            .collect();
        command_env("/usr/bin/docker", &borrowed, &borrowed_env, QUERY_TIMEOUT)
            .await
            .with_context(|| "查询容器内数据库客户端失败")
    }
}

/// 容器内客户端的调用参数。
///
/// 纯函数，因此"到底执行了什么"可以被测试固定下来——这一点很重要：
/// 这里**不经过任何 shell**，SQL 与密码都各自是一个完整参数，
/// 不存在被再解释一次的可能。
///
/// `forward` 为 true 时用不带值的 `--env MYSQL_PWD`（密码只走环境变量，
/// 调用方见 `client_env`）；为 false 时退回显式的 `--env MYSQL_PWD=<值>`，
/// 此时密码会出现在 docker 客户端的命令行里——这是为了在不支持转发的
/// docker 版本上仍然能用而付的代价，因此只在必要时才走这条路。
pub fn client_args(
    container: &str,
    account: &Account,
    password: &str,
    sql: &str,
    forward: bool,
) -> Vec<String> {
    let mut args: Vec<String> = vec!["exec".into(), "--env".into()];
    if forward {
        args.push("MYSQL_PWD".into());
    } else {
        args.push(format!("MYSQL_PWD={password}"));
    }
    args.extend([
        container.to_string(),
        CLIENT.to_string(),
        format!("-h{}", account.host),
        format!("-P{}", account.port),
        format!("-u{}", account.user),
        "--batch".into(),
        "--skip-column-names".into(),
        "--execute".into(),
        sql.to_string(),
    ]);
    args
}

/// 与 `client_args` 配套的环境变量：只有转发模式下才注入密码。
pub fn client_env(password: &str, forward: bool) -> Vec<(String, String)> {
    if forward {
        vec![("MYSQL_PWD".to_string(), password.to_string())]
    } else {
        Vec::new()
    }
}

/* ------------------------------------------------------------------ *
 * 解析：全部是纯函数，因此可以被穷举测试
 * ------------------------------------------------------------------ */

/// 把 `名字=值` 行解析成键值对。非法行直接跳过，不做猜测。
pub fn parse_pairs(output: &str) -> Vec<(String, String)> {
    output
        .lines()
        .filter_map(|line| {
            let line = line.trim_end_matches('\r');
            let (name, value) = line.split_once('=')?;
            if name.is_empty() {
                return None;
            }
            Some((name.trim().to_uppercase(), value.trim().to_string()))
        })
        .collect()
}

fn pair<'a>(rows: &'a [(String, String)], key: &str) -> Option<&'a str> {
    rows.iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.as_str())
}

/// 与 `pair` 的区别：**空字符串按「没有」处理**。
///
/// 这一点很重要：`WSREP_SST_DONOR=` 为空表示"当前没有指定捐赠者"，
/// 展示成空串会让人以为采集到了内容。
fn nonempty(rows: &[(String, String)], key: &str) -> Option<String> {
    pair(rows, key)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn number(rows: &[(String, String)], key: &str) -> Option<u64> {
    pair(rows, key)?.trim().parse().ok()
}

fn decimal(rows: &[(String, String)], key: &str) -> Option<f64> {
    pair(rows, key)?.trim().parse().ok()
}

/// `ON`/`OFF`（以及数字 1/0）转布尔。**识别不了就是 None**，不默认成 false。
pub fn parse_on_off(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "on" | "1" | "true" | "yes" => Some(true),
        "off" | "0" | "false" | "no" => Some(false),
        _ => None,
    }
}

/// 由 wsrep 变量构造 Galera 状态。
///
/// 一个 wsrep 变量都没有时返回 `None`：那说明这台机器根本不是集群成员，
/// 这与"集群健康"是完全不同的两件事。
pub fn galera_from_rows(rows: &[(String, String)]) -> Option<GaleraState> {
    if !rows.iter().any(|(name, _)| name.starts_with("WSREP_")) {
        return None;
    }
    Some(GaleraState {
        cluster_status: pair(rows, "WSREP_CLUSTER_STATUS").unwrap_or("未知").into(),
        cluster_size: number(rows, "WSREP_CLUSTER_SIZE"),
        // 期望值由调用方按本机配置补上，解析层只负责事实
        expected_cluster_size: 0,
        local_state: number(rows, "WSREP_LOCAL_STATE"),
        local_state_comment: pair(rows, "WSREP_LOCAL_STATE_COMMENT").unwrap_or("未知").into(),
        ready: pair(rows, "WSREP_READY").and_then(parse_on_off),
        connected: pair(rows, "WSREP_CONNECTED").and_then(parse_on_off),
        desync: pair(rows, "WSREP_DESYNC").and_then(parse_on_off),
        flow_control_paused: decimal(rows, "WSREP_FLOW_CONTROL_PAUSED"),
        flow_control_paused_ns: number(rows, "WSREP_FLOW_CONTROL_PAUSED_NS"),
        last_committed: pair(rows, "WSREP_LAST_COMMITTED")
            .and_then(|v| v.trim().parse().ok()),
        recv_queue: number(rows, "WSREP_LOCAL_RECV_QUEUE"),
        send_queue: number(rows, "WSREP_LOCAL_SEND_QUEUE"),
        cert_failures: number(rows, "WSREP_LOCAL_CERT_FAILURES"),
        bf_aborts: number(rows, "WSREP_LOCAL_BF_ABORTS"),
        cluster_state_uuid: nonempty(rows, "WSREP_CLUSTER_STATE_UUID"),
        node_uuid: nonempty(rows, "WSREP_LOCAL_STATE_UUID"),
        sst_donor: nonempty(rows, "WSREP_SST_DONOR"),
        provider_version: nonempty(rows, "WSREP_PROVIDER_VERSION"),
        incoming: pair(rows, "WSREP_INCOMING_ADDRESSES")
            .map(|value| {
                value
                    .split(',')
                    .map(str::trim)
                    .filter(|part| !part.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
    })
}

pub fn apply_identity(state: &mut DbHostState, rows: &[(String, String)]) {
    if let Some(version) = pair(rows, "VERSION") {
        state.version = version.into();
    }
    if let Some(comment) = pair(rows, "VERSION_COMMENT") {
        state.version_comment = comment.into();
    }
    state.hostname = pair(rows, "HOSTNAME").map(str::to_string);
    state.server_id = number(rows, "SERVER_ID");
    state.read_only = pair(rows, "READ_ONLY").and_then(parse_on_off);
}

pub fn apply_host_status(state: &mut DbHostState, rows: &[(String, String)]) {
    state.uptime = number(rows, "UPTIME");
    state.threads_connected = number(rows, "THREADS_CONNECTED");
    state.threads_running = number(rows, "THREADS_RUNNING");
}

/// `SHOW MASTER STATUS`：列数随版本变化，因此只按位置取前两列，
/// 并要求第一列看起来像 binlog 文件名。空结果表示没开 binlog（是「没有」，不是错误）。
pub fn parse_binlog(output: &str) -> (Option<String>, Option<u64>) {
    let Some(line) = output.lines().find(|line| !line.trim().is_empty()) else {
        return (None, None);
    };
    let mut fields = line.trim_end_matches('\r').split('\t');
    let file = fields.next().unwrap_or_default().trim();
    if file.is_empty() || !file.contains('.') {
        return (None, None);
    }
    let position = fields.next().and_then(|value| value.trim().parse().ok());
    (Some(file.to_string()), position)
}

/// 逐行取字段。列数不足的行直接丢弃：宁可少一行，也不要错位解读。
fn rows_of(output: &str) -> Vec<Vec<String>> {
    output
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            line.trim_end_matches('\r')
                .split('\t')
                .map(|field| field.trim().to_string())
                .collect()
        })
        .collect()
}

pub fn parse_servers(output: &str) -> Vec<ProxySqlServer> {
    rows_of(output)
        .into_iter()
        .filter_map(|fields| {
            if fields.len() < 6 {
                return None;
            }
            Some(ProxySqlServer {
                hostgroup_id: fields[0].parse().ok()?,
                hostname: fields[1].clone(),
                port: fields[2].parse().unwrap_or(3306),
                status: fields[3].clone(),
                weight: fields[4].parse().unwrap_or(0),
                max_connections: fields[5].parse().unwrap_or(0),
                ..Default::default()
            })
        })
        .collect()
}

pub fn parse_pools(output: &str) -> Vec<ProxySqlPool> {
    rows_of(output)
        .into_iter()
        .filter_map(|fields| {
            if fields.len() < 11 {
                return None;
            }
            Some(ProxySqlPool {
                hostgroup: fields[0].parse().ok()?,
                srv_host: fields[1].clone(),
                srv_port: fields[2].parse().unwrap_or(3306),
                conn_used: fields[3].parse().ok(),
                conn_free: fields[4].parse().ok(),
                conn_ok: fields[5].parse().ok(),
                conn_err: fields[6].parse().ok(),
                queries: fields[7].parse().ok(),
                bytes_sent: fields[8].parse().ok(),
                bytes_recv: fields[9].parse().ok(),
                latency_us: fields[10].parse().ok(),
            })
        })
        .collect()
}

pub fn parse_digests(output: &str) -> Vec<ProxySqlDigest> {
    rows_of(output)
        .into_iter()
        .filter_map(|fields| {
            if fields.len() < 6 {
                return None;
            }
            let digest = fields[2].clone();
            if digest.is_empty() {
                return None;
            }
            Some(ProxySqlDigest {
                hostgroup: fields[0].parse().unwrap_or(0),
                schemaname: fields[1].clone(),
                digest,
                count_star: fields[3].parse().unwrap_or(0),
                sum_time: fields[4].parse().unwrap_or(0),
                max_time: fields[5].parse().unwrap_or(0),
            })
        })
        .take(MAX_DIGESTS)
        .collect()
}

/// 把连接池里的延迟合并到后端列表上（后端表本身没有延迟列）。
pub fn merge_latency(state: &mut ProxySqlState) {
    for server in &mut state.servers {
        if let Some(pool) = state
            .pools
            .iter()
            .find(|pool| pool.srv_host == server.hostname && pool.srv_port == server.port)
        {
            server.latency_us = pool.latency_us;
            server.queries = pool.queries;
            server.conn_used = pool.conn_used;
            server.conn_free = pool.conn_free;
        }
    }
}

/// 某个 hostgroup 里 `ONLINE` 的后端数。
pub fn count_online(servers: &[ProxySqlServer], hostgroup: i64) -> u64 {
    servers
        .iter()
        .filter(|server| server.hostgroup_id == hostgroup && server.status == "ONLINE")
        .count() as u64
}

/// 一层备份的现状：只描述目录里的事实，新鲜度由 `backup_state` 结合成功标记给出。
pub fn tier_state(tier: &str, directory: &Path) -> BackupTier {
    let mut state = BackupTier {
        tier: tier.into(),
        directory: directory.display().to_string(),
        ..Default::default()
    };
    let Ok(entries) = std::fs::read_dir(directory) else {
        return state;
    };
    // 目录名是 `%Y%m%dT%H%M%SZ`，因此按名字排序即按时间排序
    let mut runs: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    runs.sort();
    let Some(latest) = runs.last() else {
        return state;
    };
    state.directory = latest.display().to_string();
    let mut age_files = 0usize;
    let mut age_ok = true;
    if let Ok(files) = std::fs::read_dir(latest) {
        for file in files.filter_map(|entry| entry.ok()) {
            let path = file.path();
            if !path.is_file() {
                continue;
            }
            state.files += 1;
            if let Ok(metadata) = path.metadata() {
                state.size_bytes = Some(state.size_bytes.unwrap_or(0) + metadata.len());
            }
            if path.file_name().is_some_and(|name| name == "SHA256SUMS") {
                state.has_checksums = true;
            }
            if is_age_path(&path) {
                age_files += 1;
                age_ok &= has_age_header(&path);
            }
        }
    }
    // 没有 .age 文件就是「不知道」，有一个不合格就是「不合格」
    state.age_header_ok = (age_files > 0).then_some(age_ok);
    state
}

fn is_age_path(path: &Path) -> bool {
    path.extension().is_some_and(|extension| extension == "age")
}

/// 归档是不是 age 格式。
///
/// **只读文件头**：归档可能有几十 GB，绝不整份读进来。解密私钥按设计不在节点上，
/// 因此这里不是（也不假装是）密码学校验，只是确认"备份脚本发布前做过的那一步"仍然成立。
fn has_age_header(path: &Path) -> bool {
    use std::io::Read;
    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    let mut head = [0u8; 32];
    match file.read(&mut head) {
        Ok(count) => head[..count].starts_with(AGE_HEADER),
        Err(_) => false,
    }
}

/// 备份链路的完整现状：各层级的目录事实 + 定时器时刻。
pub async fn backup_state(root: &str, now: i64) -> Result<BackupState> {
    let root_path = Path::new(root);
    anyhow::ensure!(
        root_path.is_dir(),
        "备份目录不存在：{root}（本机可能不承担备份职责）"
    );
    let status_dir = root_path.join("status");
    let mut tiers = Vec::new();
    for tier in ["hourly", "daily", "weekly"] {
        let mut state = tier_state(tier, &root_path.join(tier));
        // success 标记由备份脚本在成功时 touch，因此它的 mtime 就是最近成功时刻
        if let Ok(metadata) = std::fs::metadata(status_dir.join(format!("{tier}.success")))
            && let Ok(modified) = metadata.modified()
            && let Ok(since) = modified.duration_since(std::time::UNIX_EPOCH)
        {
            let at = since.as_secs() as i64;
            state.last_success = Some(at);
            state.age_seconds = Some(now - at);
        }
        tiers.push(state);
    }
    let (next_run, last_trigger) = timer_times().await;
    Ok(BackupState {
        root: root.into(),
        tiers,
        next_run,
        last_trigger,
    })
}

/// 读取备份定时器的下次触发时刻。
///
/// 请求时带 `--timestamp=unix`，因此正常情况下拿到的是微秒整数；
/// 但不同 systemd 版本仍可能给出人类可读时间，所以两种都认。
/// 认不出来就返回 None，界面显示「未知」——绝不用猜出来的时间冒充调度事实。
pub fn parse_timer_value(raw: &str) -> Option<i64> {
    let raw = raw.trim();
    if raw.is_empty() || raw == "n/a" {
        return None;
    }
    if let Ok(micros) = raw.parse::<i64>() {
        // systemd 用 0 表示"没有下次触发"
        return (micros > 0).then_some(micros / 1_000_000);
    }
    // 形态：`Fri 2026-09-11 03:30:00 UTC`，星期几可有可无
    let mut parts: Vec<&str> = raw.split_whitespace().collect();
    if parts.first().is_some_and(|part| !part.contains('-')) {
        parts.remove(0);
    }
    let [date, time, zone] = parts[..] else {
        return None;
    };
    let naive =
        chrono::NaiveDateTime::parse_from_str(&format!("{date} {time}"), "%Y-%m-%d %H:%M:%S")
            .ok()?;
    match zone.to_ascii_uppercase().as_str() {
        // 只有明确的 UTC 才能无歧义换算
        "UTC" | "GMT" | "+0000" => Some(naive.and_utc().timestamp()),
        // 本地时间：只在唯一解时采用（夏令时重叠等歧义一律显示未知）
        _ => chrono::TimeZone::from_local_datetime(&chrono::Local, &naive)
            .single()
            .map(|value| value.timestamp()),
    }
}

async fn timer_times() -> (Option<i64>, Option<i64>) {
    let mut next: Option<i64> = None;
    let mut last: Option<i64> = None;
    for tier in ["hourly", "daily"] {
        let unit = format!("galera-backup-{tier}.timer");
        if let Ok(raw) = super::command(
            "/usr/bin/systemctl",
            &[
                "show",
                &unit,
                "--timestamp=unix",
                "--property=NextElapseUSecRealtime",
                "--value",
            ],
            QUERY_TIMEOUT,
        )
        .await
        {
            let value = parse_timer_value(&raw);
            // 取两个层级里最近的一次：界面只需要回答"下一次备份什么时候来"
            next = match (next, value) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            };
        }
        if let Ok(raw) = super::command(
            "/usr/bin/systemctl",
            &[
                "show",
                &unit,
                "--timestamp=unix",
                "--property=LastTriggerUSec",
                "--value",
            ],
            QUERY_TIMEOUT,
        )
        .await
        {
            let value = parse_timer_value(&raw);
            last = match (last, value) {
                (Some(a), Some(b)) => Some(a.max(b)),
                (a, b) => a.or(b),
            };
        }
    }
    (next, last)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect()
    }

    /// 一段真实的 wsrep 输出：健康成员。
    fn healthy_rows() -> Vec<(String, String)> {
        rows(&[
            ("WSREP_CLUSTER_SIZE", "5"),
            ("WSREP_CLUSTER_STATUS", "Primary"),
            ("WSREP_CONNECTED", "ON"),
            ("WSREP_FLOW_CONTROL_PAUSED", "0.000000"),
            ("WSREP_INCOMING_ADDRESSES", "100.100.201.1:3306,100.100.201.52:3306"),
            ("WSREP_LAST_COMMITTED", "18446744073709551615"),
            ("WSREP_LOCAL_RECV_QUEUE", "0"),
            ("WSREP_LOCAL_SEND_QUEUE", "0"),
            ("WSREP_LOCAL_STATE", "4"),
            ("WSREP_LOCAL_STATE_COMMENT", "Synced"),
            ("WSREP_LOCAL_STATE_UUID", "a1b2c3d4-0000-1111-2222-333344445555"),
            ("WSREP_READY", "ON"),
            ("WSREP_SST_DONOR", ""),
        ])
    }

    #[test]
    fn 正常成员的全部字段都被解析出来() {
        let state = galera_from_rows(&healthy_rows()).unwrap();
        assert_eq!(state.cluster_size, Some(5));
        assert_eq!(state.cluster_status, "Primary");
        assert_eq!(state.local_state, Some(4));
        assert_eq!(state.local_state_comment, "Synced");
        assert_eq!(state.ready, Some(true));
        assert_eq!(state.connected, Some(true));
        assert_eq!(state.flow_control_paused, Some(0.0));
        assert_eq!(state.recv_queue, Some(0));
        assert_eq!(state.incoming.len(), 2);
        assert_eq!(state.incoming[1], "100.100.201.52:3306");
        assert_eq!(
            state.node_uuid.as_deref(),
            Some("a1b2c3d4-0000-1111-2222-333344445555")
        );
    }

    #[test]
    fn 没有_wsrep_变量就不是集群成员() {
        // 这与"集群不健康"是两件事，因此必须是 None 而不是一份全默认的状态
        assert!(galera_from_rows(&rows(&[("UPTIME", "10")])).is_none());
        assert!(galera_from_rows(&[]).is_none());
    }

    #[test]
    fn 认不出的开关不会默认成正常() {
        // 采集到奇怪的值时必须留成未知，不能让界面显示"正常"
        assert_eq!(parse_on_off("ON"), Some(true));
        assert_eq!(parse_on_off("off"), Some(false));
        assert_eq!(parse_on_off("disconnected"), None);
        let mut data = healthy_rows();
        data.retain(|(name, _)| name != "WSREP_READY");
        data.push(("WSREP_READY".into(), "weird".into()));
        let state = galera_from_rows(&data).unwrap();
        assert_eq!(state.ready, None);
    }

    #[test]
    fn pair_解析跳过非法行且名称不区分大小写() {
        let parsed = parse_pairs("WSREP_READY=ON\n\n没有等号的一行\nwsrep_ready=OFF\n");
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].0, "WSREP_READY");
        assert_eq!(parsed[0].1, "ON");
        assert_eq!(parsed[1].1, "OFF");
    }

    #[test]
    fn 主机侧身份与状态分别落到各自字段() {
        let mut host = DbHostState::default();
        apply_identity(
            &mut host,
            &parse_pairs("VERSION=12.2.2-MariaDB\nHOSTNAME=C001\nSERVER_ID=1\nREAD_ONLY=OFF\n"),
        );
        apply_host_status(
            &mut host,
            &parse_pairs("UPTIME=12345\nTHREADS_CONNECTED=42\nTHREADS_RUNNING=3\n"),
        );
        assert_eq!(host.version, "12.2.2-MariaDB");
        assert_eq!(host.hostname.as_deref(), Some("C001"));
        assert_eq!(host.server_id, Some(1));
        assert_eq!(host.read_only, Some(false));
        assert_eq!(host.uptime, Some(12345));
        assert_eq!(host.threads_connected, Some(42));
        assert_eq!(host.threads_running, Some(3));
    }

    #[test]
    fn binlog_只按位置取前两列() {
        let (file, position) = parse_binlog("mariadb-bin.000042\t12345\t\t\t0-1-2\n");
        assert_eq!(file.as_deref(), Some("mariadb-bin.000042"));
        assert_eq!(position, Some(12345));
        // 没开 binlog：是「没有」，不是错误
        assert_eq!(parse_binlog(""), (None, None));
        assert_eq!(parse_binlog("\n"), (None, None));
        // 认不出来就不要瞎猜
        assert_eq!(parse_binlog("不是文件名\t123\n"), (None, None));
    }

    #[test]
    fn proxysql_后端解析与延迟合并() {
        let servers = parse_servers(
            "10\t100.100.201.1\t3306\tONLINE\t1000000\t100\n\
             20\t100.100.201.41\t3306\tONLINE\t900000\t100\n\
             40\t100.100.201.71\t3306\tSHUNNED\t1\t100\n",
        );
        assert_eq!(servers.len(), 3);
        assert_eq!(servers[0].hostname, "100.100.201.1");
        assert_eq!(servers[0].status, "ONLINE");
        assert_eq!(servers[0].latency_us, None, "后端表本身没有延迟列");
        assert_eq!(count_online(&servers, 10), 1);
        assert_eq!(count_online(&servers, 40), 0, "SHUNNED 不算在线");

        let pools = parse_pools(
            "10\t100.100.201.1\t3306\t2\t8\t10\t0\t1234\t999\t888\t321\n\
             20\t100.100.201.41\t3306\t0\t4\t4\t1\t10\t1\t1\t654\n",
        );
        let mut state = ProxySqlState {
            servers,
            pools,
            ..Default::default()
        };
        merge_latency(&mut state);
        assert_eq!(state.servers[0].latency_us, Some(321));
        assert_eq!(state.servers[0].queries, Some(1234));
        assert_eq!(state.servers[0].conn_free, Some(8));
        assert_eq!(state.servers[1].latency_us, Some(654));
        assert_eq!(state.servers[2].latency_us, None, "池里没有这一行就不补 0");
    }

    #[test]
    fn 查询摘要不带_sql_正文() {
        let digests = parse_digests(
            "10\tapp\t0A1B2C3D\t120\t5000\t200\n\
             10\tapp\t\t5\t1\t1\n",
        );
        assert_eq!(digests.len(), 1, "空摘要是无意义行，直接丢弃");
        assert_eq!(digests[0].digest, "0A1B2C3D");
        assert_eq!(digests[0].count_star, 120);
        // 类型里根本没有 digest_text 字段，因此正文不可能被带出去
        let json = serde_json::to_string(&digests).unwrap();
        assert!(!json.contains("SELECT"), "{json}");
        assert!(!json.contains("digest_text"), "{json}");
    }

    #[test]
    fn 列数不足的行被丢弃而不是错位解读() {
        assert!(parse_servers("10\t1.2.3.4\t3306\n").is_empty());
        assert!(parse_pools("10\t1.2.3.4\t3306\n").is_empty());
        assert!(parse_digests("10\tapp\tABC\n").is_empty());
    }

    #[test]
    fn 秘密文件解析只认指定键() {
        let text = "# 注释\nSST_PASSWORD=aaa\nEXPORTER_PASSWORD='bbb'\nBACKUP_PASSWORD=\"ccc\"\n";
        assert_eq!(parse_secret(text, "EXPORTER_PASSWORD").as_deref(), Some("bbb"));
        assert_eq!(parse_secret(text, "BACKUP_PASSWORD").as_deref(), Some("ccc"));
        assert_eq!(parse_secret(text, "SST_PASSWORD").as_deref(), Some("aaa"));
        assert_eq!(parse_secret(text, "NOT_THERE"), None);
        // 空值不算"取到了密码"
        assert_eq!(parse_secret("EXPORTER_PASSWORD=\n", "EXPORTER_PASSWORD"), None);
    }

    #[test]
    fn 密码文件权限过于宽松时拒绝读取() {
        #[cfg(unix)]
        {
            use std::io::Write;
            use std::os::unix::fs::PermissionsExt;
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("secrets.env");
            let mut file = std::fs::File::create(&path).unwrap();
            writeln!(file, "EXPORTER_PASSWORD=bbb").unwrap();
            drop(file);
            let account = Account {
                user: "mysqld_exporter".into(),
                host: "127.0.0.1".into(),
                port: 3306,
                password_file: path.display().to_string(),
                password_key: "EXPORTER_PASSWORD".into(),
            };
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
            let error = read_password(&account).unwrap_err().to_string();
            assert!(error.contains("0600"), "{error}");
            // 收紧到 0600 之后就能正常读取
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
            assert_eq!(read_password(&account).unwrap(), "bbb");
        }
    }

    #[test]
    fn 未配置账号时的原因是人话() {
        let config = DbConfig {
            container: DEFAULT_CONTAINER.into(),
            galera: None,
            proxysql: None,
            backup_root: DEFAULT_BACKUP_ROOT.into(),
            expected_cluster_size: DEFAULT_CLUSTER_SIZE,
        };
        let client = Client::new(&config);
        let reason = client.credentials(config.galera.as_ref()).unwrap_err();
        assert!(reason.contains("未配置"), "{reason}");
    }

    #[test]
    fn 转发模式下密码不进命令行() {
        let account = Account {
            user: "mysqld_exporter".into(),
            host: "127.0.0.1".into(),
            port: 3306,
            password_file: "/root/galera-secrets/cluster-secrets.env".into(),
            password_key: "EXPORTER_PASSWORD".into(),
        };
        let args = client_args("mariadb-galera", &account, "顶机密", SQL_GALERA, true);
        // 不带值的 --env：密码只在子进程环境里
        assert_eq!(args[0], "exec");
        assert_eq!(args[1], "--env");
        assert_eq!(args[2], "MYSQL_PWD");
        assert!(
            !args.iter().any(|arg| arg.contains("顶机密")),
            "转发模式下密码不得出现在命令行：{args:?}"
        );
        assert_eq!(
            client_env("顶机密", true),
            vec![("MYSQL_PWD".to_string(), "顶机密".to_string())]
        );
        // 容器与连接参数按位置排好
        assert_eq!(args[3], "mariadb-galera");
        assert_eq!(args[4], "mariadb");
        assert!(args.contains(&"-h127.0.0.1".to_string()));
        assert!(args.contains(&"-P3306".to_string()));
        assert!(args.contains(&"-umysqld_exporter".to_string()));
        assert!(args.contains(&"--batch".to_string()));
        assert!(args.contains(&"--skip-column-names".to_string()));
    }

    #[test]
    fn 退回显式赋值时密码才进命令行() {
        let account = Account {
            user: "admin".into(),
            host: "127.0.0.1".into(),
            port: 6032,
            password_file: "/root/proxysql-secrets/admin.env".into(),
            password_key: "PROXYSQL_ADMIN_PASSWORD".into(),
        };
        let args = client_args("mariadb-galera", &account, "s3cret", SQL_PROXYSQL_SERVERS, false);
        assert!(args.contains(&"MYSQL_PWD=s3cret".to_string()));
        assert!(client_env("s3cret", false).is_empty(), "显式赋值时不再注入环境");
        assert!(args.contains(&"-P6032".to_string()));
    }

    #[test]
    fn 语句是单个参数且不经过_shell() {
        let account = Account {
            user: "u".into(),
            host: "127.0.0.1".into(),
            port: 3306,
            password_file: "/x".into(),
            password_key: "K".into(),
        };
        let args = client_args("c", &account, "p", SQL_GALERA, true);
        // 没有 sh -c，SQL 整体是一个参数——不存在被 shell 再解释一次的可能
        assert!(!args.iter().any(|arg| arg == "-c" || arg == "sh" || arg == "bash"));
        let sql = args.last().unwrap();
        assert_eq!(sql, SQL_GALERA, "SQL 必须是最后一个完整参数");
        assert!(sql.contains("WSREP_CLUSTER_STATUS"));
        // 固定语句里不得出现任何写操作
        for statement in [SQL_GALERA, SQL_HOST, SQL_IDENTITY, SQL_BINLOG] {
            let upper = statement.to_uppercase();
            for forbidden in [
                "INSERT", "UPDATE ", "DELETE", "DROP", "GRANT", "CREATE USER", "SET GLOBAL",
                "ALTER ", "FLUSH", "KILL", "RELOAD", "SHUTDOWN",
            ] {
                assert!(
                    !upper.contains(forbidden),
                    "只读巡检不得包含 {forbidden}：{statement}"
                );
            }
        }
    }

    #[test]
    fn proxysql_语句不查_sql_正文() {
        assert!(
            !SQL_PROXYSQL_DIGESTS.contains("digest_text"),
            "查询摘要统计不得取回 SQL 正文"
        );
        assert!(SQL_PROXYSQL_DIGESTS.contains("stats_mysql_query_digest"));
    }

    #[test]
    fn 备份层级按目录名取最新并统计文件() {
        let dir = tempfile::tempdir().unwrap();
        let tier = dir.path().join("daily");
        // 名字即时间，因此字典序最大的就是最新的
        for name in ["20260903T190534Z", "20260910T190534Z", "20260901T000000Z"] {
            std::fs::create_dir_all(tier.join(name)).unwrap();
        }
        let latest = tier.join("20260910T190534Z");
        std::fs::write(latest.join("physical.tar.gz.age"), b"age-encryption.org/v1\n").unwrap();
        std::fs::write(latest.join("SHA256SUMS"), b"hash  physical.tar.gz.age\n").unwrap();
        std::fs::write(latest.join("cluster-state.tsv"), b"WSREP_CLUSTER_SIZE\t5\n").unwrap();

        let state = tier_state("daily", &tier);
        assert!(state.directory.ends_with("20260910T190534Z"), "{}", state.directory);
        assert_eq!(state.files, 3);
        assert!(state.has_checksums);
        assert_eq!(state.age_header_ok, Some(true));
        assert!(state.size_bytes.unwrap() > 0);
    }

    #[test]
    fn 不是_age_归档的不会被当成_age() {
        let dir = tempfile::tempdir().unwrap();
        let tier = dir.path().join("hourly");
        let run = tier.join("20260910T190534Z");
        std::fs::create_dir_all(&run).unwrap();
        // 内容被截断／根本不是 age：必须如实报告 false，而不是"有文件就算好"
        std::fs::write(run.join("business.sql.gz.age"), b"not-an-age-file").unwrap();
        let state = tier_state("hourly", &tier);
        assert_eq!(state.age_header_ok, Some(false));
    }

    #[test]
    fn 没有成功标记时新鲜度保持未知而不是零() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("daily")).unwrap();
        let state = tier_state("daily", &dir.path().join("daily"));
        assert_eq!(state.last_success, None);
        assert_eq!(state.age_seconds, None, "没有成功记录时不能说'刚刚成功'");
    }

    #[test]
    fn 空目录不会被当作有备份() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("daily")).unwrap();
        let state = tier_state("daily", &dir.path().join("daily"));
        assert_eq!(state.files, 0);
        assert!(!state.has_checksums);
        assert_eq!(state.age_header_ok, None);
        assert_eq!(state.size_bytes, None);
    }

    #[test]
    fn 定时器时刻两种格式都能认() {
        // 微秒整数（--timestamp=unix 的正常输出）
        assert_eq!(parse_timer_value("1757500200000000\n"), Some(1_757_500_200));
        // 人类可读时间（某些 systemd 版本）
        assert_eq!(
            parse_timer_value("Fri 2026-09-11 03:30:00 UTC\n"),
            Some(1_789_097_400)
        );
        // 没有下次触发（systemd 用 0 表示）
        assert_eq!(parse_timer_value("0"), None);
        assert_eq!(parse_timer_value("n/a"), None);
        assert_eq!(parse_timer_value(""), None);
        // 认不出来就是未知，不能瞎猜一个时间
        assert_eq!(parse_timer_value("明天"), None);
    }
}
