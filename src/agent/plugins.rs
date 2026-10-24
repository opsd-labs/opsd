use super::*;
use opsd_plugin_sdk::{Context, Request};

pub async fn execute(
    ctx: &ContextState,
    plugin: &str,
    version: &str,
    operation: &str,
    input: &Value,
    task_id: &str,
) -> Result<Value> {
    anyhow::ensure!(!container_mode(), "节点插件变更需要宿主机 Agent");
    anyhow::ensure!(
        opsd_plugin_sdk::identifier(plugin) && opsd_plugin_sdk::identifier(version),
        "插件编号或版本无效"
    );
    let _guard = ctx.network.lock().await;
    let arch = if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "amd64"
    };
    let directory = ctx.dir.join("plugins").join(plugin).join(version);
    std::fs::create_dir_all(&directory)?;
    let binary = directory.join("plugin");
    if !binary.exists() {
        let bytes = hub_request(
            ctx,
            "GET",
            &format!("/plugin-packages/{plugin}/{version}/{arch}"),
            128 * 1024 * 1024,
        )
        .await?;
        super::atomic_replace(&binary, &bytes)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o500))?;
        }
    }
    let context: Context = serde_json::from_slice(
        &hub_request(
            ctx,
            "POST",
            &format!("/plugin-context/{task_id}"),
            1024 * 1024,
        )
        .await?,
    )?;
    anyhow::ensure!(
        context.plugin == plugin
            && context.version == version
            && context.task_id.as_deref() == Some(task_id),
        "插件执行上下文绑定不匹配"
    );
    let credentials = context.secrets.clone();
    let request = Request {
        context,
        method: "NODE".into(),
        path: operation.into(),
        body: input.clone(),
    };
    let state = ctx.dir.join("plugins-state").join(plugin);
    std::fs::create_dir_all(&state)?;
    let output = opsd_plugin_sdk::command(
        binary.to_str().context("插件路径无法编码")?,
        &["node".into()],
        &[("OPSD_PLUGIN_NODE_DIR".into(), state.display().to_string())],
        Some(serde_json::to_vec(&request)?),
        1800,
    )
    .await?;
    let mut result: opsd_plugin_sdk::Response = serde_json::from_str(&output)?;
    opsd_plugin_sdk::scrub(&mut result.body, &credentials);
    if result.status == 409 {
        return Err(opsd_plugin_sdk::uncertain(
            result.body["error"]
                .as_str()
                .unwrap_or("节点插件结果待核实"),
        ));
    }
    anyhow::ensure!(
        result.status < 400,
        "{}",
        result.body["error"].as_str().unwrap_or("节点插件执行失败")
    );
    Ok(result.body)
}

async fn hub_request(
    ctx: &ContextState,
    method: &str,
    path: &str,
    limit: usize,
) -> Result<Vec<u8>> {
    use http_body_util::{BodyExt, Empty, Limited};
    let url = reqwest::Url::parse(&ctx.config.agent_url)?;
    let host = url.host_str().context("缺少主控名称")?.to_owned();
    let port = url.port_or_known_default().unwrap_or(443);
    let target = if network::overlay_active(ctx).await {
        ctx.db
            .get::<opsd::network::DesiredNetwork>("network", "desired")
            .await?
            .and_then(|d| d.hub_overlay_address)
            .context("缺少主控 ET 地址")?
    } else {
        host.clone()
    };
    let flow = async {
        let tcp = tokio::net::TcpStream::connect((target.as_str(), port)).await?;
        // 原始主机名也可以是 IP；拨号到 ET 时仍按原始身份验证证书。
        let tls = tokio_rustls::TlsConnector::from(Arc::new(pki::client_config(&ctx.dir)?))
            .connect(rustls::pki_types::ServerName::try_from(host.clone())?, tcp)
            .await?;
        let (mut client, connection) =
            hyper::client::conn::http1::handshake(hyper_util::rt::TokioIo::new(tls)).await?;
        tokio::spawn(async move {
            let _ = connection.await;
        });
        let authority = if host.contains(':') {
            format!("[{host}]:{port}")
        } else {
            format!("{host}:{port}")
        };
        let request = hyper::Request::builder()
            .method(method)
            .uri(path)
            .header("host", authority)
            .body(Empty::<hyper::body::Bytes>::new())?;
        let response = client.send_request(request).await?;
        anyhow::ensure!(
            response.status().is_success(),
            "主控插件请求失败：{}",
            response.status()
        );
        Ok::<_, anyhow::Error>(
            Limited::new(response.into_body(), limit)
                .collect()
                .await?
                .to_bytes()
                .to_vec(),
        )
    };
    tokio::time::timeout(std::time::Duration::from_secs(120), flow).await?
}
