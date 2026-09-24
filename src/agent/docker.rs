use anyhow::{Context, Result};
use base64::Engine;
use futures_util::StreamExt;
use opsd::protocol::*;
use serde_json::{Value, json};

fn client() -> Result<reqwest::Client> {
    #[cfg(unix)]
    {
        Ok(reqwest::Client::builder()
            .unix_socket("/var/run/docker.sock")
            .connect_timeout(std::time::Duration::from_secs(5))
            .build()?)
    }
    #[cfg(not(unix))]
    {
        anyhow::bail!("Docker 运维仅支持 Linux 宿主机 Unix socket")
    }
}
fn component(id: &str) -> Result<&str> {
    anyhow::ensure!(
        !id.is_empty()
            && id.len() <= 256
            && id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b)),
        "Docker 资源标识无效"
    );
    Ok(id)
}
pub async fn get(path: &str) -> Result<Value> {
    let response = client()?
        .get(format!("http://localhost{path}"))
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await?
        .error_for_status()?;
    Ok(response.json().await?)
}
async fn post(path: &str, body: Value) -> Result<Value> {
    let response = client()?
        .post(format!("http://localhost{path}"))
        .json(&body)
        .timeout(std::time::Duration::from_secs(90))
        .send()
        .await?
        .error_for_status()?;
    let bytes = response.bytes().await?;
    if bytes.is_empty() {
        Ok(Value::Null)
    } else {
        Ok(serde_json::from_slice(&bytes)?)
    }
}
pub async fn inventory() -> Result<Value> {
    let (containers, images, networks, volumes, version) = tokio::try_join!(
        get("/containers/json?all=1"),
        get("/images/json"),
        get("/networks"),
        get("/volumes"),
        get("/version")
    )?;
    let containers=containers.as_array().context("Docker 容器列表格式无效")?.iter().map(|c|json!({"id":c["Id"],"names":c["Names"],"image":c["Image"],"state":c["State"],"status":c["Status"],"ports":c["Ports"],"mounts":c["Mounts"],"project":c["Labels"]["com.docker.compose.project"],"protected":protected(c)})).collect::<Vec<_>>();
    Ok(
        json!({"containers":containers,"images":images,"networks":networks,"volumes":volumes["Volumes"],"version":version}),
    )
}
fn protected(c: &Value) -> bool {
    let text = format!("{} {}", c["Image"], c["Names"]).to_lowercase();
    [
        "mariadb",
        "mysql",
        "postgres",
        "openresty",
        "nginx",
        "haproxy",
        "proxysql",
        "traefik",
    ]
    .iter()
    .any(|p| text.contains(p))
}
pub async fn action(container: &str, operation: &DockerOperation) -> Result<Value> {
    let c = component(container)?;
    let before = get(&format!("/containers/{c}/json")).await?;
    let op = match operation {
        DockerOperation::Start => "start",
        DockerOperation::Stop => "stop",
        DockerOperation::Restart => "restart",
    };
    post(&format!("/containers/{c}/{op}?t=20"), json!({})).await?;
    let after = get(&format!("/containers/{c}/json")).await?;
    anyhow::ensure!(
        after["Id"] == before["Id"] && after["Mounts"] == before["Mounts"],
        "容器身份或挂载发生非预期变化"
    );
    let running = after["State"]["Running"].as_bool().unwrap_or(false);
    anyhow::ensure!(
        running != matches!(operation, DockerOperation::Stop),
        "容器没有达到预期状态"
    );
    Ok(json!({"id":after["Id"],"state":after["State"],"mounts":after["Mounts"]}))
}
pub async fn detail(container: &str) -> Result<Value> {
    let id = component(container)?;
    let v = get(&format!("/containers/{id}/json")).await?;
    let stats = if v["State"]["Running"] == true {
        get(&format!(
            "/containers/{id}/stats?stream=false&one-shot=true"
        ))
        .await
        .ok()
    } else {
        None
    };
    Ok(
        json!({"id":v["Id"],"name":v["Name"],"image":v["Config"]["Image"],"state":v["State"],"mounts":v["Mounts"],"ports":v["NetworkSettings"]["Ports"],"networks":v["NetworkSettings"]["Networks"],"restart_policy":v["HostConfig"]["RestartPolicy"],"stats":stats}),
    )
}
pub async fn pull(reference: &str) -> Result<Value> {
    anyhow::ensure!(
        !reference.is_empty()
            && reference.len() < 512
            && !reference.chars().any(char::is_whitespace),
        "镜像引用无效"
    );
    let mut response = client()?
        .post("http://localhost/images/create")
        .query(&[("fromImage", reference)])
        .send()
        .await?
        .error_for_status()?
        .bytes_stream();
    let mut buffer = Vec::new();
    let mut latest = Value::Null;
    while let Some(chunk) =
        tokio::time::timeout(std::time::Duration::from_secs(120), response.next()).await?
    {
        buffer.extend_from_slice(&chunk?);
        anyhow::ensure!(buffer.len() < 1024 * 1024, "镜像进度帧过大");
        while let Some(i) = buffer.iter().position(|b| *b == b'\n') {
            let bytes = buffer.drain(..=i).collect::<Vec<_>>();
            if bytes.len() > 1 {
                let item: Value = serde_json::from_slice(&bytes)?;
                anyhow::ensure!(
                    item.get("error").is_none(),
                    "镜像拉取失败：{}",
                    item["error"]
                );
                latest = item;
            }
        }
    }
    if buffer.iter().any(|b| !b.is_ascii_whitespace()) {
        let item: Value = serde_json::from_slice(&buffer)?;
        anyhow::ensure!(
            item.get("error").is_none(),
            "镜像拉取失败：{}",
            item["error"]
        );
        latest = item;
    }
    anyhow::ensure!(!latest.is_null(), "镜像拉取未返回任何状态");
    Ok(json!({"reference":reference,"progress":latest}))
}
pub async fn stream(
    stream_id: &str,
    container: &str,
    terminal: bool,
    command: Vec<String>,
    mut input: tokio::sync::mpsc::Receiver<String>,
    out: tokio::sync::mpsc::Sender<Frame>,
) -> Result<()> {
    let c = component(container)?;
    if !terminal {
        let mut response = client()?
            .get(format!("http://localhost/containers/{c}/logs"))
            .query(&[
                ("stdout", "1"),
                ("stderr", "1"),
                ("follow", "1"),
                ("tail", "200"),
                ("timestamps", "1"),
            ])
            .send()
            .await?
            .error_for_status()?
            .bytes_stream();
        let tty = get(&format!("/containers/{c}/json")).await?["Config"]["Tty"]
            .as_bool()
            .unwrap_or(false);
        let mut pending = Vec::new();
        while let Some(bytes) = response.next().await {
            pending.extend_from_slice(&bytes?);
            anyhow::ensure!(pending.len() < 2 * 1024 * 1024, "日志缓冲超限");
            let mut payload = Vec::new();
            if tty {
                payload.append(&mut pending)
            } else {
                while pending.len() >= 8 {
                    let len = u32::from_be_bytes(pending[4..8].try_into()?) as usize;
                    anyhow::ensure!(len <= 1024 * 1024, "日志帧过大");
                    if pending.len() < len + 8 {
                        break;
                    }
                    payload.extend_from_slice(&pending[8..8 + len]);
                    pending.drain(..8 + len);
                }
            }
            if !payload.is_empty() {
                out.try_send(Frame::StreamData {
                    stream_id: stream_id.into(),
                    data: base64::engine::general_purpose::STANDARD.encode(payload),
                })
                .map_err(|_| anyhow::anyhow!("日志消费者过慢，已中断流"))?;
            }
        }
        return Ok(());
    }
    anyhow::ensure!(
        command.len() == 1 && matches!(command[0].as_str(), "/bin/sh" | "/bin/bash"),
        "首版容器终端仅支持 sh 或 bash"
    );
    let exec=post(&format!("/containers/{c}/exec"),json!({"AttachStdin":true,"AttachStdout":true,"AttachStderr":true,"Tty":true,"Cmd":command})).await?;
    let id = exec["Id"].as_str().context("Docker 未返回执行会话")?;
    #[cfg(unix)]
    {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut stream = tokio::net::UnixStream::connect("/var/run/docker.sock").await?;
        let body = r#"{"Detach":false,"Tty":true}"#;
        stream.write_all(format!("POST /exec/{id}/start HTTP/1.1\r\nHost: localhost\r\nConnection: Upgrade\r\nUpgrade: tcp\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",body.len()).as_bytes()).await?;
        let mut headers = Vec::new();
        while !headers.ends_with(b"\r\n\r\n") {
            let b = tokio::time::timeout(std::time::Duration::from_secs(10), stream.read_u8())
                .await??;
            headers.push(b);
            anyhow::ensure!(headers.len() < 32768, "终端响应头过大");
        }
        anyhow::ensure!(
            headers.starts_with(b"HTTP/1.1 101") || headers.starts_with(b"HTTP/1.1 200"),
            "Docker 拒绝终端升级"
        );
        let mut buffer = vec![0; 8192];
        loop {
            tokio::select! {
                count=stream.read(&mut buffer)=>{let n=count?;if n==0{break}out.try_send(Frame::StreamData{stream_id:stream_id.into(),data:base64::engine::general_purpose::STANDARD.encode(&buffer[..n])}).map_err(|_|anyhow::anyhow!("终端消费者过慢"))?;},
                data=input.recv()=>{let Some(data)=data else{break};let bytes=base64::engine::general_purpose::STANDARD.decode(data)?;anyhow::ensure!(bytes.len()<=65536,"终端输入过大");stream.write_all(&bytes).await?;}
            }
        }
        Ok(())
    }
    #[cfg(not(unix))]
    {
        let _ = (id, &mut input, out);
        anyhow::bail!("容器终端仅支持 Linux")
    }
}
