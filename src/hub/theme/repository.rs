//! 控制台主题的发行版安装入口，下载规则与业务模块共用。
use super::{ApiResult, Json, S, State, MAX_PACKAGE_BYTES, install_frontend_package};
use crate::hub::repository::{self, InstallInput, ResolveInput};

pub async fn resolve_repository(S(_s): S<State>, Json(input): Json<ResolveInput>) -> ApiResult<Json<serde_json::Value>> {
    Ok(Json(repository::resolve(&input).await?))
}
pub async fn install_repository(S(s): S<State>, Json(input): Json<InstallInput>) -> ApiResult<Json<serde_json::Value>> {
    let bytes = repository::download(&input, MAX_PACKAGE_BYTES).await?;
    let installed = install_frontend_package(&s, bytes).await?;
    Ok(Json(serde_json::json!({ "short": installed.manifest.short, "digest": installed.digest })))
}
