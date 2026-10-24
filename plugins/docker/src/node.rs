use anyhow::{Context as _, Result, ensure};
use opsd_plugin_sdk::{self as sdk, Request};
use serde_json::{Value, json};

async fn docker(args: &[String]) -> Result<String> {
    sdk::command("docker", args, &[], None, 600).await
}
async fn prepare(allow_install: bool) -> Result<()> {
    let exists = sdk::command(
        "sh",
        &sdk::strings(&["-c", "command -v docker"]),
        &[],
        None,
        10,
    )
    .await
    .is_ok();
    if !exists {
        ensure!(
            allow_install,
            "Docker 尚未安装，请在安装向导允许安装 Engine 与 Compose"
        );
        let os = std::fs::read_to_string("/etc/os-release")?;
        if os
            .lines()
            .any(|l| l == "ID=debian" || l == "ID=ubuntu" || l == "ID=\"ubuntu\"")
        {
            sdk::command("apt-get", &sdk::strings(&["update"]), &[], None, 300).await?;
            let compose = if sdk::command(
                "apt-cache",
                &sdk::strings(&["show", "docker-compose-v2"]),
                &[],
                None,
                30,
            )
            .await
            .is_ok()
            {
                "docker-compose-v2"
            } else {
                "docker-compose"
            };
            sdk::command(
                "apt-get",
                &sdk::strings(&[
                    "install",
                    "-y",
                    "--no-install-recommends",
                    "docker.io",
                    compose,
                ]),
                &[("DEBIAN_FRONTEND".into(), "noninteractive".into())],
                None,
                900,
            )
            .await?;
        } else {
            let family = if os.contains("ID=fedora") {
                "fedora"
            } else if os.contains("ID=\"rhel\"") || os.contains("ID=rhel") {
                "rhel"
            } else {
                "centos"
            };
            sdk::command(
                "dnf",
                &sdk::strings(&["install", "-y", "dnf-plugins-core"]),
                &[],
                None,
                300,
            )
            .await?;
            sdk::command(
                "dnf",
                &[
                    "config-manager".into(),
                    "--add-repo".into(),
                    format!("https://download.docker.com/linux/{family}/docker-ce.repo"),
                ],
                &[],
                None,
                60,
            )
            .await?;
            sdk::command(
                "dnf",
                &sdk::strings(&[
                    "install",
                    "-y",
                    "docker-ce",
                    "docker-ce-cli",
                    "containerd.io",
                    "docker-compose-plugin",
                ]),
                &[],
                None,
                900,
            )
            .await?;
        }
        sdk::command(
            "systemctl",
            &sdk::strings(&["enable", "--now", "docker"]),
            &[],
            None,
            60,
        )
        .await?;
    }
    docker(&sdk::strings(&["info"])).await?;
    if docker(&sdk::strings(&["compose", "version"]))
        .await
        .is_err()
        && sdk::command("docker-compose", &sdk::strings(&["version"]), &[], None, 10)
            .await
            .is_err()
    {
        ensure!(allow_install, "没有可用的 Compose，请允许安装后重试");
        if std::path::Path::new("/usr/bin/apt-get").exists() {
            sdk::command("apt-get", &sdk::strings(&["update"]), &[], None, 300).await?;
            let package = if sdk::command(
                "apt-cache",
                &sdk::strings(&["show", "docker-compose-plugin"]),
                &[],
                None,
                30,
            )
            .await
            .is_ok()
            {
                "docker-compose-plugin"
            } else if sdk::command(
                "apt-cache",
                &sdk::strings(&["show", "docker-compose-v2"]),
                &[],
                None,
                30,
            )
            .await
            .is_ok()
            {
                "docker-compose-v2"
            } else {
                "docker-compose"
            };
            // 只补 Compose；包管理器若计划改动已有 Engine 或其运行时，就停止。
            let changes = sdk::command(
                "apt-get",
                &sdk::strings(&["--simulate", "--no-remove", "install", package]),
                &[("LC_ALL".into(), "C".into())],
                None,
                60,
            )
            .await?;
            ensure!(
                !changes
                    .lines()
                    .filter(|line| line.starts_with("Inst ") || line.starts_with("Remv "))
                    .any(|line| {
                        line.split_whitespace().nth(1).is_some_and(|name| {
                            [
                                "docker.io",
                                "docker-ce",
                                "docker-ce-cli",
                                "containerd",
                                "containerd.io",
                                "moby-engine",
                                "moby-cli",
                            ]
                            .contains(&name.split(':').next().unwrap_or(name))
                        })
                    }),
                "安装 Compose 需要改动已有 Docker 组件，请手动处理后重试"
            );
            sdk::command(
                "apt-get",
                &sdk::strings(&[
                    "install",
                    "-y",
                    "--no-install-recommends",
                    "--no-remove",
                    package,
                ]),
                &[("DEBIAN_FRONTEND".into(), "noninteractive".into())],
                None,
                300,
            )
            .await?;
        } else {
            sdk::command(
                "dnf",
                &sdk::strings(&["install", "-y", "--exclude=docker-ce,docker-ce-cli,docker.io,containerd.io,moby-engine,moby-cli", "docker-compose-plugin"]),
                &[],
                None,
                300,
            )
            .await?;
        }
        if docker(&sdk::strings(&["compose", "version"]))
            .await
            .is_err()
        {
            sdk::command("docker-compose", &sdk::strings(&["version"]), &[], None, 10)
                .await
                .context("Compose 安装后仍不可用")?;
        }
    }
    Ok(())
}

fn image(input: &Value, agent: bool) -> Result<String> {
    let default = if agent {
        "portainer/agent:2.45.3"
    } else {
        "portainer/portainer-ce:2.45.3"
    };
    let image = input["image"].as_str().unwrap_or(default);
    let prefix = if agent {
        "portainer/agent:"
    } else {
        "portainer/portainer-ce:"
    };
    ensure!(
        image.starts_with(prefix)
            && !image.ends_with(":latest")
            && image
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"/.:@_-".contains(&b)),
        "Portainer 镜像必须使用官方固定版本"
    );
    Ok(image.into())
}

async fn binding(request: &Request) -> Result<String> {
    let address = request.body["address"]
        .as_str()
        .context("缺少节点 ET 地址")?;
    sdk::require_local_address(address).await?;
    Ok(address.into())
}

pub async fn execute(request: Request) -> Result<Value> {
    if request.path == "identity.prepare" {
        return sdk::identity().await;
    }
    ensure!(
        ["server.install", "agent.install"].contains(&request.path.as_str()),
        "未知 Docker 节点动作"
    );
    let address = binding(&request).await?;
    prepare(request.body["allow_engine_install"] == true).await?;
    let agent = request.path == "agent.install";
    let name = if agent {
        "opsd-portainer-agent"
    } else {
        "opsd-portainer"
    };
    let image = image(&request.body, agent)?;
    let state = sdk::node_directory()?;
    let tls = state.join("tls");
    let root = docker(&sdk::strings(&["info", "--format", "{{.DockerRootDir}}"])).await?;
    let volumes = std::path::Path::new(root.trim()).join("volumes");
    ensure!(
        volumes.is_absolute() && !root.trim().contains(['\n', '\r', ':']),
        "Docker 数据目录无效"
    );
    let port = if agent { "9001/tcp" } else { "9443/tcp" };
    let host_port = if agent { "9002" } else { "9443" };
    let binds = if agent {
        vec![
            "/var/run/docker.sock:/var/run/docker.sock".to_owned(),
            format!("{}:/var/lib/docker/volumes", volumes.display()),
        ]
    } else {
        vec![
            "/var/run/docker.sock:/var/run/docker.sock".to_owned(),
            "opsd-portainer-data:/data".into(),
            format!("{}:/certs:ro", tls.display()),
            format!(
                "{}:/run/admin-password:ro",
                state.join("admin-password").display()
            ),
        ]
    };
    let server_args = sdk::strings(&[
        "--admin-password-file",
        "/run/admin-password",
        "--sslcert",
        "/certs/server.pem",
        "--sslkey",
        "/certs/server-key.pem",
        "--http-disabled",
    ]);
    let existing = match docker(&sdk::strings(&["inspect", name])).await {
        Ok(text) => Some(serde_json::from_str::<Value>(&text)?),
        Err(_) => {
            // 区分容器不存在与 Docker 不可用，后者不能继续创建。
            docker(&sdk::strings(&["info"])).await?;
            None
        }
    };
    if let Some(existing) = &existing {
        let item = &existing[0];
        ensure!(
            item["Config"]["Labels"]["io.opsd.plugin"] == "docker",
            "同名容器不属于 Docker 插件，不能接管"
        );
        ensure!(
            item["Config"]["Image"] == image
                && item["HostConfig"]["Binds"] == json!(binds)
                && (agent || item["Config"]["Cmd"] == json!(server_args)),
            "现有管理服务镜像、参数或挂载不同，需显式迁移"
        );
        ensure!(
            item["HostConfig"]["PortBindings"]
                == json!({(port):[{"HostIp":address,"HostPort":host_port}]}),
            "现有管理服务端口绑定不同，需显式迁移"
        );
        let secret = format!(
            "AGENT_SECRET={}",
            sdk::secret(&request.context, "agent_secret")?
        );
        ensure!(
            item["Config"]["Env"]
                .as_array()
                .is_some_and(|env| env.iter().any(|v| v == &secret)),
            "现有服务共享密钥与配置不同，需要显式修复"
        );
    }
    let mut certificate_changed = false;
    if !agent {
        let certificate = request.body["certificate"]
            .as_str()
            .context("缺少管理服务证书")?;
        ensure!(tls.join("server-key.pem").exists(), "管理服务私钥不存在");
        certificate_changed =
            std::fs::read(tls.join("server.pem")).ok().as_deref() != Some(certificate.as_bytes());
        sdk::private_write(&tls.join("server.pem"), certificate.as_bytes())?;
        if existing.is_none() {
            sdk::private_write(
                &state.join("admin-password"),
                sdk::secret(&request.context, "admin_password")?.as_bytes(),
            )?;
        }
    }
    if let Some(existing) = existing {
        let item = &existing[0];
        if certificate_changed && item["State"]["Running"] == true {
            docker(&sdk::strings(&["restart", name])).await?;
        } else if item["State"]["Running"] != true {
            docker(&sdk::strings(&["start", name])).await?;
        }
        let actual: Value =
            serde_json::from_str(&docker(&sdk::strings(&["inspect", name])).await?)?;
        ensure!(
            actual[0]["State"]["Running"] == true,
            "Portainer 容器没有运行"
        );
        return Ok(json!({"name":name,"address":address,"reused":true}));
    }
    docker(&["pull".into(), image.clone()]).await?;
    let mut args = sdk::strings(&[
        "run",
        "-d",
        "--name",
        name,
        "--restart",
        "unless-stopped",
        "--label",
        "io.opsd.plugin=docker",
        "-v",
        "/var/run/docker.sock:/var/run/docker.sock",
    ]);
    let agent_secret = sdk::secret(&request.context, "agent_secret")?;
    let env_file = state.join("portainer.env");
    ensure!(
        !agent_secret.contains(['\r', '\n']),
        "Agent 共享密钥不能包含换行"
    );
    sdk::private_write(
        &env_file,
        format!("AGENT_SECRET={agent_secret}\n").as_bytes(),
    )?;
    args.extend(["--env-file".into(), env_file.display().to_string()]);
    if agent {
        args.extend([
            "-p".into(),
            format!("{}:9002:9001", sdk::address_host(&address)?),
            "-v".into(),
            format!("{}:/var/lib/docker/volumes", volumes.display()),
            image,
        ]);
    } else {
        args.extend([
            "-p".into(),
            format!("{}:9443:9443", sdk::address_host(&address)?),
            "-v".into(),
            "opsd-portainer-data:/data".into(),
            "-v".into(),
            format!("{}:/certs:ro", tls.display()),
            "-v".into(),
            format!(
                "{}:/run/admin-password:ro",
                state.join("admin-password").display()
            ),
            image,
            "--admin-password-file".into(),
            "/run/admin-password".into(),
            "--sslcert".into(),
            "/certs/server.pem".into(),
            "--sslkey".into(),
            "/certs/server-key.pem".into(),
            "--http-disabled".into(),
        ]);
    }
    let id = docker(&args).await?;
    let inspect: Value =
        serde_json::from_str(&docker(&["inspect".into(), id.trim().into()]).await?)?;
    ensure!(
        inspect[0]["State"]["Running"] == true,
        "Portainer 容器没有运行"
    );
    Ok(json!({"name":name,"address":address,"container_id":id.trim()}))
}
