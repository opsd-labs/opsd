//! 公开 GitHub 发行版安装。只下载已发布资产，不运行仓库源码。

use super::{
    ApiError, ApiResult, Bytes, Json, MAX_PACKAGE_BYTES, S, State, StatusCode, bad, check,
    install_frontend_package,
};
use futures_util::StreamExt;
use reqwest::{Client, Url};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolveInput {
    url: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstallInput {
    url: String,
    tag: String,
    asset_id: u64,
}

struct Repository {
    owner: String,
    name: String,
    /// 链接中已经按 URL 编码的标签路径。
    tag_path: Option<String>,
}

impl Repository {
    fn parse(value: &str) -> ApiResult<Self> {
        let url = Url::parse(value).map_err(|_| bad("仓库链接不是有效的 URL"))?;
        check(
            url.scheme() == "https"
                && url.host_str() == Some("github.com")
                && url.port().is_none()
                && url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none(),
            "只支持不含凭据和参数的公开 GitHub HTTPS 仓库或发行版链接",
        )?;
        let parts: Vec<_> = url.path().trim_matches('/').split('/').collect();
        check(parts.len() >= 2, "仓库链接必须包含所有者和仓库名")?;
        let owner = parts[0];
        let name = parts[1].strip_suffix(".git").unwrap_or(parts[1]);
        let valid_name = |name: &str| {
            !name.is_empty()
                && name != "."
                && name != ".."
                && name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        };
        check(
            valid_name(owner) && valid_name(name),
            "仓库所有者或名称不合法",
        )?;
        let tag_path = match parts.as_slice() {
            [_, _] | [_, _, "releases", "latest"] => None,
            [_, _, "releases", "tag", rest @ ..]
                if !rest.is_empty() && rest.iter().all(|p| !p.is_empty()) =>
            {
                Some(rest.join("/"))
            }
            _ => return Err(bad("请粘贴仓库首页、最新发行版或指定标签的发行版链接")),
        };
        Ok(Self {
            owner: owner.into(),
            name: name.into(),
            tag_path,
        })
    }

    fn endpoint(&self, tag: Option<&str>) -> Url {
        let mut url = Url::parse("https://api.github.com").expect("固定 GitHub API 地址有效");
        if let Some(tag) = tag {
            url.path_segments_mut()
                .expect("HTTPS 地址支持路径")
                .extend(["repos", &self.owner, &self.name, "releases", "tags", tag]);
        } else if let Some(tag_path) = &self.tag_path {
            url.set_path(&format!(
                "/repos/{}/{}/releases/tags/{tag_path}",
                self.owner, self.name
            ));
        } else {
            url.path_segments_mut()
                .expect("HTTPS 地址支持路径")
                .extend(["repos", &self.owner, &self.name, "releases", "latest"]);
        }
        url
    }

    fn asset_endpoint(&self, id: u64) -> Url {
        Url::parse(&format!(
            "https://api.github.com/repos/{}/{}/releases/assets/{id}",
            self.owner, self.name
        ))
        .expect("已校验的仓库名与数字资产标识构成有效地址")
    }
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    assets: Vec<Asset>,
}

#[derive(Deserialize, Serialize)]
pub struct Asset {
    id: u64,
    name: String,
    size: u64,
    #[serde(skip_serializing)]
    state: String,
}

impl Release {
    fn packages(self) -> ApiResult<Self> {
        check(!self.draft, "不能安装未发布的发行版")?;
        let assets = self
            .assets
            .into_iter()
            .filter(|asset| {
                asset.state == "uploaded" && asset.name.to_ascii_lowercase().ends_with(".zip")
            })
            .collect::<Vec<_>>();
        check(
            !assets.is_empty(),
            "该发行版没有已上传的主题 ZIP，请先在主题仓库发布构建产物",
        )?;
        Ok(Self { assets, ..self })
    }
}

fn allowed_redirect(url: &Url) -> bool {
    url.scheme() == "https"
        && url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none()
        && url.host_str().is_some_and(|host| {
            host == "api.github.com"
                || host == "github.com"
                || host.ends_with(".githubusercontent.com")
        })
}

fn client() -> ApiResult<Client> {
    Client::builder()
        .user_agent(concat!("opsd/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(60))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 5 || !allowed_redirect(attempt.url()) {
                attempt.error("GitHub 资产重定向地址不受支持")
            } else {
                attempt.follow()
            }
        }))
        .build()
        .map_err(|_| {
            ApiError(
                StatusCode::INTERNAL_SERVER_ERROR,
                "无法初始化 GitHub 客户端".into(),
            )
        })
}

fn upstream(error: reqwest::Error) -> ApiError {
    let message = if error.is_timeout() {
        "GitHub 请求超时"
    } else {
        "GitHub 连接或响应失败"
    };
    ApiError(StatusCode::BAD_GATEWAY, message.into())
}

async fn release(
    client: &Client,
    repository: &Repository,
    tag: Option<&str>,
) -> ApiResult<Release> {
    let response = client
        .get(repository.endpoint(tag))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2026-03-10")
        .send()
        .await
        .map_err(upstream)?;
    if response.status() == StatusCode::NOT_FOUND {
        return Err(ApiError(
            StatusCode::NOT_FOUND,
            "公开 GitHub 仓库或发行版不存在".into(),
        ));
    }
    if !response.status().is_success() {
        return Err(ApiError(
            StatusCode::BAD_GATEWAY,
            "GitHub 请求被拒绝或超过访问限制".into(),
        ));
    }
    response
        .json::<Release>()
        .await
        .map_err(upstream)?
        .packages()
}

pub async fn resolve_repository(
    S(_s): S<State>,
    Json(input): Json<ResolveInput>,
) -> ApiResult<Json<serde_json::Value>> {
    let repository = Repository::parse(&input.url)?;
    let release = release(&client()?, &repository, None).await?;
    Ok(Json(serde_json::json!({
        "owner": repository.owner,
        "repository": repository.name,
        "tag": release.tag_name,
        "assets": release.assets,
    })))
}

pub async fn install_repository(
    S(s): S<State>,
    Json(input): Json<InstallInput>,
) -> ApiResult<Json<serde_json::Value>> {
    let repository = Repository::parse(&input.url)?;
    check(
        !input.tag.trim().is_empty() && input.asset_id > 0,
        "必须选择发行版标签和 ZIP 资产",
    )?;
    let client = client()?;
    let release = release(&client, &repository, Some(&input.tag)).await?;
    let asset = release
        .assets
        .iter()
        .find(|asset| asset.id == input.asset_id)
        .ok_or_else(|| bad("所选资产不属于该发行版的主题 ZIP"))?;
    if asset.size > MAX_PACKAGE_BYTES as u64 {
        return Err(ApiError(
            StatusCode::PAYLOAD_TOO_LARGE,
            "主题 ZIP 超过 20 MiB 上限".into(),
        ));
    }
    let response = client
        .get(repository.asset_endpoint(asset.id))
        .header("Accept", "application/octet-stream")
        .header("X-GitHub-Api-Version", "2026-03-10")
        .send()
        .await
        .map_err(upstream)?;
    if !response.status().is_success() {
        return Err(ApiError(
            StatusCode::BAD_GATEWAY,
            "GitHub 主题资产下载失败".into(),
        ));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_PACKAGE_BYTES as u64)
    {
        return Err(ApiError(
            StatusCode::PAYLOAD_TOO_LARGE,
            "主题 ZIP 超过 20 MiB 上限".into(),
        ));
    }
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(upstream)?;
        if bytes.len() + chunk.len() > MAX_PACKAGE_BYTES {
            return Err(ApiError(
                StatusCode::PAYLOAD_TOO_LARGE,
                "主题 ZIP 超过 20 MiB 上限".into(),
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    let installed = install_frontend_package(&s, Bytes::from(bytes)).await?;
    Ok(Json(
        serde_json::json!({ "short": installed.manifest.short, "digest": installed.digest }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 仓库链接只允许公开_github_入口() {
        let repository =
            Repository::parse("https://github.com/opsd-labs/opsd-theme-web.git").unwrap();
        assert_eq!(
            repository.endpoint(None).path(),
            "/repos/opsd-labs/opsd-theme-web/releases/latest"
        );
        let release =
            Repository::parse("https://github.com/opsd-labs/opsd-theme-web/releases/tag/v1%2Ftest")
                .unwrap();
        assert_eq!(
            release.endpoint(None).path(),
            "/repos/opsd-labs/opsd-theme-web/releases/tags/v1%2Ftest"
        );
        assert_eq!(
            release.endpoint(Some("v1/test")).path(),
            "/repos/opsd-labs/opsd-theme-web/releases/tags/v1%2Ftest"
        );
        for invalid in [
            "http://github.com/a/b",
            "https://github.com.evil.test/a/b",
            "https://localhost/a/b",
            "https://user:secret@github.com/a/b",
            "https://github.com/a/b?token=x",
            "https://github.com/a/b/tree/main",
            "https://github.com/a/b/releases/tag/",
        ] {
            assert!(Repository::parse(invalid).is_err(), "应拒绝地址 {invalid}");
        }
    }

    #[test]
    fn 下载重定向只允许_github_https_资源() {
        for valid in [
            "https://api.github.com/a",
            "https://release-assets.githubusercontent.com/a",
        ] {
            assert!(allowed_redirect(&Url::parse(valid).unwrap()));
        }
        for invalid in [
            "http://github.com/a",
            "https://127.0.0.1/a",
            "https://evil.test/a",
            "https://github.com:444/a",
        ] {
            assert!(!allowed_redirect(&Url::parse(invalid).unwrap()));
        }
    }

    #[test]
    fn 发行版必须包含已上传_zip() {
        let no_zip: Release = serde_json::from_str(r#"{"tag_name":"v1","draft":false,"assets":[{"id":1,"name":"source.tar.gz","size":1,"state":"uploaded"}]}"#).unwrap();
        assert!(no_zip.packages().is_err());
        let ready: Release = serde_json::from_str(r#"{"tag_name":"v1","draft":false,"assets":[{"id":2,"name":"theme.zip","size":10,"state":"uploaded"},{"id":3,"name":"pending.zip","size":0,"state":"open"}]}"#).unwrap();
        let packages = ready.packages().unwrap();
        assert_eq!(packages.assets.len(), 1);
        assert_eq!(packages.assets[0].id, 2);
    }
}
