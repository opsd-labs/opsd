//! 主题系统。
//!
//! 两条主题线的粒度**刻意不同**，这不是实现偷懒，而是风险差异决定的：
//!
//! | | 控制台主题 | 分享页主题 |
//! |---|---|---|
//! | 粒度 | **令牌级**：颜色、圆角、密度、字体、明暗 | **包级**：完整前端包 |
//! | 替换范围 | 不替换组件结构 | 替换整个分享页前端 |
//! | 理由 | 含终端/任务/表单/键盘交互，结构替换风险过高 | 纯只读展示，隔离后可自由替换 |
//!
//! 参考项目 Komari 也明确约定主题不替换 `/admin` 与 `/terminal`，只替换公开监控面。
//!
//! 安全边界：
//!
//! - 首版只支持 `configuration.type = "managed"`；**`raw` 不做**——它允许主题在管理区
//!   渲染自带 HTML，等于在控制台里执行第三方代码。
//! - 主题包使用 SHA-256 校验，仅支持本地上传安装，**不接在线市场、不自动远程拉取**。
//! - 分享页主题运行在 `/share/{令牌}/` 之下，与控制台**不同路径**，
//!   控制台会话 Cookie 的 Path 限定在安全入口之下，因此主题拿不到它。
//! - 分享页主题的响应带独立 CSP，默认禁止外联。
//! - 分享页主题的配置**公开可读**，因此不得放密钥。
use anyhow::{Context, Result};
use opsd::{protocol::now, store::Store};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

pub const BUCKET: &str = "themes";
/// 当前启用的主题：`settings` 桶中的两条记录。
pub const SETTINGS_BUCKET: &str = "settings";
pub const CONSOLE_ID: &str = "theme_console";
pub const SHARE_ID: &str = "theme_share";
/// 内置主题的 `short`，不可删除。
pub const DEFAULT: &str = "default";
/// 单个主题包的体积上限。
pub const MAX_PACKAGE_BYTES: usize = 20 * 1024 * 1024;
/// 解压后的总体积上限，防止压缩炸弹。
pub const MAX_EXTRACTED_BYTES: u64 = 64 * 1024 * 1024;
/// 主题设置值的总体积上限。
pub const MAX_SETTINGS_BYTES: usize = 32 * 1024;

/// 多语言文本：普通字符串或 `{ "zh-CN": "...", "en": "..." }`。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Localized {
    Plain(String),
    Map(HashMap<String, String>),
}

impl Localized {
    /// 取显示文本，按「当前语言 → 基础语言 → 同基础语言的方言 → 任意一个」回退。
    ///
    /// 最后一步按 key 排序取第一个，而不是依赖 `HashMap` 的遍历顺序——
    /// 否则同一个清单在不同进程里会显示不同的名称，测试也会随机失败。
    pub fn text(&self, language: &str) -> String {
        match self {
            Localized::Plain(text) => text.clone(),
            Localized::Map(map) => {
                if let Some(value) = map.get(language) {
                    return value.clone();
                }
                let base = language.split('-').next().unwrap_or(language);
                if let Some(value) = map.get(base) {
                    return value.clone();
                }
                // 请求 zh 而清单只声明了 zh-CN 时，仍应命中中文
                if let Some(value) = map
                    .iter()
                    .find(|(key, _)| key.split('-').next() == Some(base))
                    .map(|(_, value)| value)
                {
                    return value.clone();
                }
                let mut keys: Vec<&String> = map.keys().collect();
                keys.sort();
                keys.first()
                    .and_then(|key| map.get(*key))
                    .cloned()
                    .unwrap_or_default()
            }
        }
    }
}

/// 主题覆盖的界面。同时覆盖两者的主题也是合法的。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Surface {
    Share,
    Console,
}

/// 控制台主题的令牌覆盖。键是 `style.css` 中的自定义属性名。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Tokens {
    #[serde(default)]
    pub light: HashMap<String, String>,
    #[serde(default)]
    pub dark: HashMap<String, String>,
}

/// 托管配置项的声明。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Field {
    pub key: String,
    pub name: Localized,
    /// `switch` / `select` / `number` / `string` / `text`
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub default: serde_json::Value,
    #[serde(default)]
    pub help: Option<Localized>,
    /// 仅 `select` 使用。
    #[serde(default)]
    pub options: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Configuration {
    /// 首版只接受 `managed`。
    #[serde(default = "managed")]
    pub r#type: String,
    #[serde(default)]
    pub data: Vec<Field>,
}
fn managed() -> String {
    "managed".into()
}

/// 主题清单，对应包内的 `theme.json`。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    /// 唯一标识：只能包含大小写字母、数字、下划线与连字符，且不得为 `default`。
    pub short: String,
    pub name: Localized,
    #[serde(default)]
    pub description: Option<Localized>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub author: Option<Localized>,
    #[serde(default)]
    pub preview: Option<String>,
    pub surfaces: Vec<Surface>,
    /// 仅控制台主题使用。
    #[serde(default)]
    pub tokens: Option<Tokens>,
    /// 仅控制台主题使用：说明该主题替代了哪些令牌，便于界面提示。
    #[serde(default)]
    pub configuration: Option<Configuration>,
}

/// 已安装主题的索引记录。清单与资源分开存放。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Installed {
    pub manifest: Manifest,
    /// 安装时计算的内容摘要（包级主题为整个字节流的 SHA-256）。
    pub digest: String,
    pub installed_at: i64,
    /// 包级主题是否已解压出资源目录。
    pub has_assets: bool,
}

/// 校验清单。任何不合规都直接拒绝，不做"尽力而为"的修补。
pub fn validate(manifest: &Manifest) -> Result<()> {
    let short = manifest.short.trim();
    anyhow::ensure!(!short.is_empty(), "主题标识不能为空");
    anyhow::ensure!(short.len() <= 48, "主题标识过长（不超过 48 个字符）");
    anyhow::ensure!(
        short
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'),
        "主题标识只能包含大小写字母、数字、下划线与连字符"
    );
    anyhow::ensure!(
        !short.eq_ignore_ascii_case(DEFAULT),
        "主题标识不能是 default，它保留给内置主题"
    );
    anyhow::ensure!(!manifest.surfaces.is_empty(), "主题必须声明至少一个界面");
    anyhow::ensure!(
        !manifest
            .surfaces
            .iter()
            .any(|s| matches!(s, Surface::Console))
            || manifest.tokens.is_some(),
        "控制台主题必须提供 tokens，否则不会产生任何效果"
    );
    if let Some(tokens) = &manifest.tokens {
        for (mode, values) in [("light", &tokens.light), ("dark", &tokens.dark)] {
            for (key, value) in values {
                anyhow::ensure!(
                    key.starts_with("--"),
                    "{mode} 中的令牌名必须以 -- 开头：{key}"
                );
                // 令牌值最终会写进 CSS 自定义属性。除了直接注入声明，
                // 还要挡住能引发外部请求或代码执行的写法。
                anyhow::ensure!(
                    !value.contains([';', '{', '}', '<', '>', '\n', '\r', '\\']),
                    "{mode} 中 {key} 的值包含不允许的字符"
                );
                let lowered = value.to_ascii_lowercase();
                for forbidden in ["url(", "@import", "expression(", "javascript:", "/*"] {
                    anyhow::ensure!(
                        !lowered.contains(forbidden),
                        "{mode} 中 {key} 的值包含不允许的写法：{forbidden}"
                    );
                }
                anyhow::ensure!(value.len() <= 120, "{mode} 中 {key} 的值过长");
            }
        }
    }
    if let Some(configuration) = &manifest.configuration {
        anyhow::ensure!(
            configuration.r#type == "managed",
            "首版只支持 managed 配置；raw 会在管理界面渲染第三方 HTML，风险与收益不匹配"
        );
        for field in &configuration.data {
            anyhow::ensure!(
                !field.key.is_empty() && field.key.len() <= 48,
                "配置项标识不合法"
            );
            anyhow::ensure!(
                ["switch", "select", "number", "string", "text"].contains(&field.kind.as_str()),
                "未知的配置项类型：{}",
                field.kind
            );
            if field.kind == "select" {
                anyhow::ensure!(field.options.is_some(), "select 配置项必须给出选项");
            }
        }
    }
    Ok(())
}

/// 主题设置值。键为配置项 `key`。
pub type Settings = HashMap<String, serde_json::Value>;

/// 按声明补齐默认值。已保存的值优先；缺省值不会覆盖已保存的值。
pub fn with_defaults(manifest: &Manifest, saved: &Settings) -> Settings {
    let mut result = saved.clone();
    if let Some(configuration) = &manifest.configuration {
        for field in &configuration.data {
            result.entry(field.key.clone()).or_insert_with(|| {
                if field.default.is_null() {
                    match field.kind.as_str() {
                        "switch" => serde_json::json!(false),
                        "number" => serde_json::json!(0),
                        "select" => field
                            .options
                            .as_ref()
                            .and_then(|o| o.split(',').next())
                            .map(|first| serde_json::json!(first.trim()))
                            .unwrap_or(serde_json::json!("")),
                        _ => serde_json::json!(""),
                    }
                } else {
                    field.default.clone()
                }
            });
        }
    }
    result
}

/// 主题资源目录：`{data}/themes/{short}`。仅包级主题会有内容。
pub fn asset_dir(data: &Path, short: &str) -> PathBuf {
    data.join("themes").join(short)
}

/// 解压主题包到资源目录。
///
/// 拒绝一切可能逃出目标目录的条目：绝对路径、`..`、以及符号链接。
/// 同时限制解压后的总体积，避免压缩炸弹。
pub fn extract(package: &[u8], destination: &Path) -> Result<()> {
    use std::io::Read;
    let mut archive =
        zip::ZipArchive::new(std::io::Cursor::new(package)).context("主题包不是有效的 zip")?;
    std::fs::create_dir_all(destination)?;
    let mut total: u64 = 0;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let Some(relative) = entry.enclosed_name() else {
            anyhow::bail!("主题包包含越界路径，已拒绝安装");
        };
        total += entry.size();
        anyhow::ensure!(total <= MAX_EXTRACTED_BYTES, "主题解压后体积超出上限");
        let target = destination.join(relative);
        if entry.is_dir() {
            std::fs::create_dir_all(&target)?;
            continue;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut bytes = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut bytes)?;
        std::fs::write(&target, bytes)?;
    }
    Ok(())
}

/// 从解压目录中读取清单。清单必须位于包的根目录。
pub fn read_manifest(directory: &Path) -> Result<Manifest> {
    let path = directory.join("theme.json");
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("主题包根目录缺少 theme.json：{}", path.display()))?;
    let manifest: Manifest = serde_json::from_str(&text).context("theme.json 解析失败")?;
    Ok(manifest)
}

/// 安装控制台主题：清单直接以 JSON 提交，不涉及文件解压。
pub async fn install_console(db: &Store, manifest: Manifest, digest: String) -> Result<Installed> {
    validate(&manifest)?;
    anyhow::ensure!(
        manifest.surfaces.contains(&Surface::Console),
        "该主题未声明支持控制台"
    );
    let installed = Installed {
        manifest,
        digest,
        installed_at: now(),
        has_assets: false,
    };
    db.put(BUCKET, &installed.manifest.short, &installed)
        .await?;
    Ok(installed)
}

/// 安装分享页主题：解压到资源目录，校验清单后登记。
pub async fn install_share(
    db: &Store,
    data: &Path,
    package: &[u8],
    digest: String,
) -> Result<Installed> {
    anyhow::ensure!(
        package.len() <= MAX_PACKAGE_BYTES,
        "主题包超过 {} MB 上限",
        MAX_PACKAGE_BYTES / 1024 / 1024
    );
    // 先解压到临时目录，校验通过后再就位，避免留下半个主题
    let staging = data
        .join("themes")
        .join(format!(".staging-{}", opsd::protocol::id()));
    let _ = std::fs::remove_dir_all(&staging);
    let result = (|| -> Result<Manifest> {
        extract(package, &staging)?;
        let manifest = read_manifest(&staging)?;
        validate(&manifest)?;
        anyhow::ensure!(
            manifest.surfaces.contains(&Surface::Share),
            "该主题未声明支持分享页"
        );
        anyhow::ensure!(
            staging.join("index.html").is_file(),
            "分享页主题包根目录必须包含 index.html"
        );
        Ok(manifest)
    })();
    let manifest = match result {
        Ok(manifest) => manifest,
        Err(error) => {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(error);
        }
    };
    let destination = asset_dir(data, &manifest.short);
    let _ = std::fs::remove_dir_all(&destination);
    std::fs::rename(&staging, &destination).context("主题资源就位失败")?;
    let installed = Installed {
        manifest,
        digest,
        installed_at: now(),
        has_assets: true,
    };
    db.put(BUCKET, &installed.manifest.short, &installed)
        .await?;
    Ok(installed)
}

pub async fn list(db: &Store) -> Result<Vec<Installed>> {
    let mut themes = db.list::<Installed>(BUCKET).await?;
    themes.sort_by(|a, b| a.manifest.short.cmp(&b.manifest.short));
    Ok(themes)
}

/// 删除主题。内置主题不可删除；删除时同时清理已保存的设置。
pub async fn remove(db: &Store, data: &Path, short: &str) -> Result<()> {
    anyhow::ensure!(!short.eq_ignore_ascii_case(DEFAULT), "内置主题不可删除");
    let installed = db
        .get::<Installed>(BUCKET, short)
        .await?
        .ok_or_else(|| anyhow::anyhow!("主题不存在"))?;
    db.delete(BUCKET, short).await?;
    db.delete(SETTINGS_BUCKET, &format!("theme_settings_{short}"))
        .await?;
    if installed.has_assets {
        let _ = std::fs::remove_dir_all(asset_dir(data, short));
    }
    Ok(())
}

/// 读取当前启用的主题与它的设置。返回 `None` 表示使用内置主题。
pub async fn active(db: &Store, id: &str) -> Result<Option<(Installed, Settings)>> {
    let Some(short) = db.get::<String>(SETTINGS_BUCKET, id).await? else {
        return Ok(None);
    };
    if short.eq_ignore_ascii_case(DEFAULT) {
        return Ok(None);
    }
    let Some(installed) = db.get::<Installed>(BUCKET, &short).await? else {
        // 主题被删除后回落到内置，不报错
        return Ok(None);
    };
    let saved = db
        .get::<Settings>(SETTINGS_BUCKET, &format!("theme_settings_{short}"))
        .await?
        .unwrap_or_default();
    let settings = with_defaults(&installed.manifest, &saved);
    Ok(Some((installed, settings)))
}

/// 分享页主题的 CSP：默认禁止外联，只放行自身资源与内联样式。
///
/// 主题不得引用控制台接口，因此 `connect-src` 只允许同源。
pub fn share_csp() -> &'static str {
    "default-src 'none'; \
     script-src 'self'; \
     style-src 'self' 'unsafe-inline'; \
     img-src 'self' data:; \
     font-src 'self'; \
     connect-src 'self'; \
     base-uri 'none'; \
     form-action 'none'; \
     frame-ancestors 'none'"
}

/* ------------------------------------------------------------------ *
 * 处理函数
 * ------------------------------------------------------------------ */

use super::*;
use axum::{
    Json,
    body::Bytes,
    extract::{Path as AxumPath, State as S},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};

/// 列出已安装主题，并标明当前启用的是哪一个。
pub async fn list_themes(S(s): S<State>) -> ApiResult<Json<serde_json::Value>> {
    let themes: Vec<serde_json::Value> = list(&s.db)
        .await?
        .into_iter()
        .map(|installed| {
            serde_json::json!({
                "short": installed.manifest.short,
                "name": installed.manifest.name,
                "description": installed.manifest.description,
                "version": installed.manifest.version,
                "author": installed.manifest.author,
                "surfaces": installed.manifest.surfaces,
                "has_assets": installed.has_assets,
                "installed_at": installed.installed_at,
                "digest": installed.digest,
                "fields": installed
                    .manifest
                    .configuration
                    .as_ref()
                    .map(|c| c.data.clone())
                    .unwrap_or_default(),
            })
        })
        .collect();
    Ok(Json(serde_json::json!({ "themes": themes })))
}

/// 读取两个界面各自启用的主题，以及控制台主题需要覆盖的令牌。
pub async fn read_active(S(s): S<State>) -> ApiResult<Json<serde_json::Value>> {
    let console = active(&s.db, CONSOLE_ID).await?;
    let share = active(&s.db, SHARE_ID).await?;
    Ok(Json(serde_json::json!({
        "console": describe(console),
        "share": describe(share),
    })))
}

fn describe(active: Option<(Installed, Settings)>) -> serde_json::Value {
    match active {
        Some((installed, settings)) => serde_json::json!({
            "short": installed.manifest.short,
            "name": installed.manifest.name,
            "tokens": installed.manifest.tokens,
            "settings": settings,
            "fields": installed
                .manifest
                .configuration
                .as_ref()
                .map(|c| c.data.clone())
                .unwrap_or_default(),
        }),
        None => serde_json::json!({
            "short": DEFAULT,
            "name": { "zh-CN": "内置主题" },
            "tokens": null,
            "settings": {},
            "fields": [],
        }),
    }
}

/// 请求侧严格清单：拒绝未知字段（含嵌套）。持久化/包内读取仍用宽松 `Manifest`。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThemeTokensInput {
    #[serde(default)]
    pub light: HashMap<String, String>,
    #[serde(default)]
    pub dark: HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThemeFieldInput {
    pub key: String,
    pub name: Localized,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub default: serde_json::Value,
    #[serde(default)]
    pub help: Option<Localized>,
    #[serde(default)]
    pub options: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThemeConfigurationInput {
    #[serde(default = "managed")]
    pub r#type: String,
    #[serde(default)]
    pub data: Vec<ThemeFieldInput>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThemeManifestInput {
    pub short: String,
    pub name: Localized,
    #[serde(default)]
    pub description: Option<Localized>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub author: Option<Localized>,
    #[serde(default)]
    pub preview: Option<String>,
    pub surfaces: Vec<Surface>,
    #[serde(default)]
    pub tokens: Option<ThemeTokensInput>,
    #[serde(default)]
    pub configuration: Option<ThemeConfigurationInput>,
}

impl From<ThemeManifestInput> for Manifest {
    fn from(input: ThemeManifestInput) -> Self {
        Manifest {
            short: input.short,
            name: input.name,
            description: input.description,
            version: input.version,
            author: input.author,
            preview: input.preview,
            surfaces: input.surfaces,
            tokens: input.tokens.map(|t| Tokens {
                light: t.light,
                dark: t.dark,
            }),
            configuration: input.configuration.map(|c| Configuration {
                r#type: c.r#type,
                data: c
                    .data
                    .into_iter()
                    .map(|f| Field {
                        key: f.key,
                        name: f.name,
                        kind: f.kind,
                        default: f.default,
                        help: f.help,
                        options: f.options,
                    })
                    .collect(),
            }),
        }
    }
}

/// 安装控制台主题。清单直接以 JSON 提交；令牌级主题不需要资源文件。
pub async fn install_console_theme(
    S(s): S<State>,
    Json(input): Json<ThemeManifestInput>,
) -> ApiResult<Json<serde_json::Value>> {
    let manifest: Manifest = input.into();
    let digest =
        opsd::protocol::digest(serde_json::to_vec(&manifest).map_err(anyhow::Error::from)?);
    let installed = install_console(&s.db, manifest, digest).await?;
    Ok(Json(
        serde_json::json!({ "short": installed.manifest.short }),
    ))
}

/// 安装分享页主题。请求体是主题包原始字节，便于用 curl 直接上传。
pub async fn install_share_theme(
    S(s): S<State>,
    body: Bytes,
) -> ApiResult<Json<serde_json::Value>> {
    check(!body.is_empty(), "请求体为空")?;
    // 摘要由主控自己计算，不信任调用方提供的任何校验值
    let digest = opsd::protocol::digest(&body);
    let installed = install_share(&s.db, &s.dir, &body, digest).await?;
    Ok(Json(serde_json::json!({
        "short": installed.manifest.short,
        "digest": installed.digest,
    })))
}

pub async fn remove_theme(
    S(s): S<State>,
    AxumPath(short): AxumPath<String>,
) -> ApiResult<Json<serde_json::Value>> {
    remove(&s.db, &s.dir, &short).await?;
    // 删掉的正好是启用中的主题时，回落到内置
    for id in [CONSOLE_ID, SHARE_ID] {
        if s.db.get::<String>(SETTINGS_BUCKET, id).await?.as_deref() == Some(short.as_str()) {
            s.db.delete(SETTINGS_BUCKET, id).await?;
        }
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Activate {
    /// `console` 或 `share`。
    surface: String,
    /// `default` 表示回到内置主题。
    short: String,
    #[serde(default)]
    settings: Settings,
}

pub async fn activate(
    S(s): S<State>,
    Json(input): Json<Activate>,
) -> ApiResult<Json<serde_json::Value>> {
    let id = match input.surface.as_str() {
        "console" => CONSOLE_ID,
        "share" => SHARE_ID,
        other => return Err(bad(&format!("未知界面：{other}"))),
    };
    if input.short.eq_ignore_ascii_case(DEFAULT) {
        s.db.delete(SETTINGS_BUCKET, id).await?;
        return Ok(Json(serde_json::json!({ "short": DEFAULT })));
    }
    let installed =
        s.db.get::<Installed>(BUCKET, &input.short)
            .await?
            .ok_or_else(|| bad("主题未安装"))?;
    // 主题必须声明覆盖该界面，否则选了也不会有任何效果
    let surface = if input.surface == "console" {
        Surface::Console
    } else {
        Surface::Share
    };
    check(
        installed.manifest.surfaces.contains(&surface),
        "该主题未声明支持这个界面",
    )?;
    let settings = with_defaults(&installed.manifest, &input.settings);
    let encoded = serde_json::to_string(&settings).map_err(anyhow::Error::from)?;
    check(encoded.len() <= MAX_SETTINGS_BYTES, "主题设置体积过大")?;
    s.db.put(SETTINGS_BUCKET, id, &input.short).await?;
    s.db.put(
        SETTINGS_BUCKET,
        &format!("theme_settings_{}", input.short),
        &settings,
    )
    .await?;
    // 日志里用清单声明的人类可读名称，便于运维核对；界面自己按浏览器语言解析。
    tracing::info!(
        theme = %installed.manifest.short,
        name = %installed.manifest.name.text("zh-CN"),
        surface = %input.surface,
        "主题已切换"
    );
    Ok(Json(serde_json::json!({
        "short": input.short,
        "settings": settings,
    })))
}

/* ------------------------------------------------------------------ *
 * 分享面：公开的主题信息与主题资源
 * ------------------------------------------------------------------ */

/// 分享页的主题设置是**公开可读**的，与 Komari 的 `theme_settings` 同理。
/// 因此这里也必须在界面与文档中反复提示：**不得放入密钥**。
pub async fn share_theme(S(s): S<State>) -> ApiResult<Json<serde_json::Value>> {
    let active = active(&s.db, SHARE_ID).await?;
    Ok(Json(describe(active)))
}

/// 分享页主题的静态资源。路径必须落在该主题的资源目录内。
pub async fn share_theme_asset(
    S(s): S<State>,
    AxumPath((_token, path)): AxumPath<(String, String)>,
) -> Response {
    if path.contains("..") || path.starts_with('/') {
        return (StatusCode::NOT_FOUND, "").into_response();
    }
    let Ok(Some((installed, _))) = active(&s.db, SHARE_ID).await else {
        return (StatusCode::NOT_FOUND, "").into_response();
    };
    if !installed.has_assets {
        return (StatusCode::NOT_FOUND, "").into_response();
    }
    let target = asset_dir(&s.dir, &installed.manifest.short).join(&path);
    let Ok(bytes) = tokio::fs::read(&target).await else {
        return (StatusCode::NOT_FOUND, "").into_response();
    };
    let mime = match target.extension().and_then(|e| e.to_str()) {
        Some("js") | Some("mjs") => "text/javascript",
        Some("css") => "text/css",
        Some("html") => "text/html; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("json") => "application/json",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    };
    (
        [
            (header::CONTENT_TYPE, mime),
            // 主题资源一律注入独立 CSP：默认禁止外联
            (header::CONTENT_SECURITY_POLICY, share_csp()),
        ],
        bytes,
    )
        .into_response()
}

/// 分享页主题的入口 HTML。未启用包级主题时由调用方回落到内置页面。
pub async fn share_theme_index(S(s): S<State>) -> Option<Response> {
    let Ok(Some((installed, _))) = active(&s.db, SHARE_ID).await else {
        return None;
    };
    if !installed.has_assets {
        return None;
    }
    let target = asset_dir(&s.dir, &installed.manifest.short).join("index.html");
    let html = tokio::fs::read_to_string(&target).await.ok()?;
    Some(
        (
            [
                (header::CONTENT_TYPE, "text/html; charset=utf-8"),
                (header::CONTENT_SECURITY_POLICY, share_csp()),
            ],
            html,
        )
            .into_response(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use opsd::protocol::digest as sha;

    #[test]
    fn 主题清单请求拒绝未知字段() {
        let raw = r#"{"short":"x","name":"N","surfaces":["share"],"nope":1}"#;
        assert!(serde_json::from_str::<ThemeManifestInput>(raw).is_err());
        // 嵌套 tokens 也必须拒绝未知字段
        let nested = r#"{"short":"x","name":"N","surfaces":["console"],"tokens":{"light":{"--a":"1"},"extra":true}}"#;
        assert!(serde_json::from_str::<ThemeManifestInput>(nested).is_err());
        let ok = r#"{"short":"x","name":"N","surfaces":["share"]}"#;
        let input: ThemeManifestInput = serde_json::from_str(ok).unwrap();
        let m: Manifest = input.into();
        assert_eq!(m.short, "x");
    }

    #[test]
    fn 旧主题清单仍可读取() {
        let raw =
            r#"{"short":"old","name":"N","surfaces":["share"],"preview":null,"legacy_field":1}"#;
        let m: Manifest = serde_json::from_str(raw).unwrap();
        assert_eq!(m.short, "old");
    }

    fn manifest(short: &str, surfaces: Vec<Surface>) -> Manifest {
        Manifest {
            short: short.into(),
            name: Localized::Plain("测试主题".into()),
            description: None,
            version: Some("1.0.0".into()),
            author: None,
            preview: None,
            surfaces,
            tokens: Some(Tokens {
                light: HashMap::from([("--accent".to_string(), "#123456".to_string())]),
                dark: HashMap::new(),
            }),
            configuration: None,
        }
    }

    #[test]
    fn 标识必须合法且不能占用_default() {
        validate(&manifest("MyTheme", vec![Surface::Console])).unwrap();
        validate(&manifest("my_theme-2", vec![Surface::Console])).unwrap();
        for bad in [
            "",
            "default",
            "DEFAULT",
            "has space",
            "斜杠/",
            "点.号",
            "很长".repeat(30).as_str(),
        ] {
            assert!(
                validate(&manifest(bad, vec![Surface::Console])).is_err(),
                "应拒绝标识 {bad:?}"
            );
        }
    }

    #[test]
    fn 声明控制台就必须给出令牌() {
        let mut m = manifest("ok", vec![Surface::Console]);
        m.tokens = None;
        assert!(
            validate(&m).is_err(),
            "没有 tokens 的控制台主题不会产生效果"
        );
        // 只声明分享页时不需要 tokens
        let m = Manifest {
            tokens: None,
            ..manifest("shareonly", vec![Surface::Share])
        };
        validate(&m).unwrap();
    }

    #[test]
    fn 必须声明至少一个界面() {
        assert!(validate(&manifest("empty", vec![])).is_err());
    }

    #[test]
    fn 令牌值不能注入额外的_css_声明() {
        for bad in [
            "#fff; --x: red",
            "red}",
            "url(x)",
            "url(https://evil.example/x)",
            "@import 'x'",
            "expression(alert(1))",
            "javascript:alert(1)",
            "a/*b*/",
            "a\\62 c",
            "a\nb",
            "a{b",
        ] {
            let mut m = manifest("inject", vec![Surface::Console]);
            m.tokens = Some(Tokens {
                light: HashMap::from([("--accent".to_string(), bad.to_string())]),
                dark: HashMap::new(),
            });
            assert!(validate(&m).is_err(), "应拒绝令牌值 {bad:?}");
        }
        // 正常取值必须通过
        let mut m = manifest("fine", vec![Surface::Console]);
        m.tokens = Some(Tokens {
            light: HashMap::from([
                ("--accent".to_string(), "#0F2540".to_string()),
                ("--radius-control".to_string(), "6px".to_string()),
            ]),
            dark: HashMap::new(),
        });
        validate(&m).unwrap();
    }

    #[test]
    fn 令牌名必须以双连字符开头() {
        let mut m = manifest("badkey", vec![Surface::Console]);
        m.tokens = Some(Tokens {
            light: HashMap::from([("accent".to_string(), "#fff".to_string())]),
            dark: HashMap::new(),
        });
        assert!(validate(&m).is_err());
    }

    #[test]
    fn 只接受_managed_配置() {
        let mut m = manifest("rawbad", vec![Surface::Console]);
        m.configuration = Some(Configuration {
            r#type: "raw".into(),
            data: vec![],
        });
        let error = validate(&m).unwrap_err().to_string();
        assert!(
            error.contains("managed"),
            "错误信息应说明只支持 managed：{error}"
        );
        m.configuration = Some(Configuration {
            r#type: "managed".into(),
            data: vec![Field {
                key: "show_tags".into(),
                name: Localized::Plain("显示标签".into()),
                kind: "switch".into(),
                default: serde_json::json!(true),
                help: None,
                options: None,
            }],
        });
        validate(&m).unwrap();
    }

    #[test]
    fn 未知配置类型与缺少选项都要拒绝() {
        let mut m = manifest("cfg", vec![Surface::Console]);
        m.configuration = Some(Configuration {
            r#type: "managed".into(),
            data: vec![Field {
                key: "x".into(),
                name: Localized::Plain("X".into()),
                kind: "colour".into(),
                default: serde_json::Value::Null,
                help: None,
                options: None,
            }],
        });
        assert!(validate(&m).is_err());
        m.configuration = Some(Configuration {
            r#type: "managed".into(),
            data: vec![Field {
                key: "y".into(),
                name: Localized::Plain("Y".into()),
                kind: "select".into(),
                default: serde_json::Value::Null,
                help: None,
                options: None,
            }],
        });
        assert!(validate(&m).is_err(), "select 必须给出选项");
    }

    #[test]
    fn 多语言文本按语言逐级回退() {
        let text = Localized::Map(HashMap::from([
            ("zh-CN".to_string(), "中文".to_string()),
            ("en".to_string(), "English".to_string()),
        ]));
        assert_eq!(text.text("zh-CN"), "中文", "精确匹配优先");
        assert_eq!(text.text("en"), "English");
        // 请求基础语言时应命中同基础语言的方言，而不是随机取一个
        assert_eq!(text.text("zh"), "中文");
        assert_eq!(text.text("zh-TW"), "中文", "同基础语言的方言也要命中");
        // 完全不相干的语言才落到最后一步，且必须稳定
        let fallback = text.text("fr");
        assert_eq!(fallback, "English", "按 key 排序取第一个，结果必须确定");
        assert_eq!(text.text("fr"), fallback, "多次调用结果一致");
        assert_eq!(Localized::Plain("纯文本".into()).text("en"), "纯文本");
        // 空映射不能 panic
        assert_eq!(Localized::Map(HashMap::new()).text("zh"), "");
    }

    #[test]
    fn 默认值补齐不会覆盖已保存的值() {
        let mut m = manifest("defaults", vec![Surface::Console]);
        m.configuration = Some(Configuration {
            r#type: "managed".into(),
            data: vec![
                Field {
                    key: "switch_a".into(),
                    name: Localized::Plain("开关".into()),
                    kind: "switch".into(),
                    default: serde_json::json!(true),
                    help: None,
                    options: None,
                },
                Field {
                    key: "number_b".into(),
                    name: Localized::Plain("数字".into()),
                    kind: "number".into(),
                    default: serde_json::Value::Null,
                    help: None,
                    options: None,
                },
                Field {
                    key: "select_c".into(),
                    name: Localized::Plain("选择".into()),
                    kind: "select".into(),
                    default: serde_json::Value::Null,
                    help: None,
                    options: Some("甲,乙,丙".into()),
                },
            ],
        });
        let saved = Settings::from([("switch_a".to_string(), serde_json::json!(false))]);
        let merged = with_defaults(&m, &saved);
        assert_eq!(
            merged["switch_a"],
            serde_json::json!(false),
            "已保存的值优先"
        );
        assert_eq!(merged["number_b"], serde_json::json!(0), "number 缺省为 0");
        assert_eq!(
            merged["select_c"],
            serde_json::json!("甲"),
            "select 取第一个选项"
        );
    }

    #[test]
    fn 压缩包拒绝越界路径() {
        use std::io::Write;
        // 构造一个含 ../ 条目的 zip
        let mut buffer = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(std::io::Cursor::new(&mut buffer));
            let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
            writer.start_file("../escape.txt", options).unwrap();
            writer.write_all(b"x").unwrap();
            writer.finish().unwrap();
        }
        let dir = std::env::temp_dir().join(format!("opsd-zip-{}", opsd::protocol::id()));
        let error = extract(&buffer, &dir).unwrap_err().to_string();
        assert!(error.contains("越界"), "应拒绝越界路径：{error}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn 分享页的_csp_默认禁止外联() {
        let csp = share_csp();
        assert!(csp.contains("default-src 'none'"));
        assert!(csp.contains("script-src 'self'"));
        assert!(csp.contains("connect-src 'self'"));
        assert!(csp.contains("frame-ancestors 'none'"));
        // 不得出现任何允许任意外部来源的通配
        assert!(!csp.contains("https:"), "CSP 不应放行任意外部来源");
        assert!(!csp.contains("unsafe-eval"));
    }

    #[test]
    fn 摘要对内容敏感() {
        // 安装时记录的摘要必须能区分不同内容，否则校验形同虚设
        assert_ne!(sha(b"theme-a"), sha(b"theme-b"));
        assert_eq!(sha(b"theme-a"), sha(b"theme-a"));
    }
}
