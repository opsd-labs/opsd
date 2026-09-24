use super::*;
use std::path::{Component, Path};
#[derive(Clone, Serialize, Deserialize)]
pub struct StackRevision {
    pub project: String,
    pub directory: String,
    pub files: Vec<String>,
    pub env_files: Vec<String>,
    pub fingerprint: String,
    pub revision: i64,
    pub protected: bool,
}
#[derive(Clone, Serialize, Deserialize)]
struct Plan {
    id: String,
    project: String,
    base: String,
    content: String,
    before: String,
    expires: i64,
}
fn project(value: &str) -> Result<()> {
    anyhow::ensure!(
        !value.is_empty()
            && value.len() < 80
            && value
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_-".contains(&b))
            && value
                .bytes()
                .next()
                .is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit()),
        "Compose 项目名无效"
    );
    Ok(())
}
fn path(directory: &Path, file: &str) -> Result<PathBuf> {
    let p = Path::new(file);
    anyhow::ensure!(
        !p.is_absolute() && p.components().all(|c| matches!(c, Component::Normal(_))),
        "配置路径必须位于项目目录内"
    );
    let joined = directory.join(p);
    let canon = joined.canonicalize()?;
    anyhow::ensure!(
        canon.starts_with(directory.canonicalize()?),
        "配置文件越出项目目录"
    );
    Ok(canon)
}
fn fingerprint(s: &StackRevision) -> Result<String> {
    let mut bytes = Vec::new();
    for f in s.files.iter().chain(s.env_files.iter()) {
        let p = path(Path::new(&s.directory), f)?;
        bytes.extend_from_slice(f.as_bytes());
        bytes.extend_from_slice(&std::fs::read(p)?);
    }
    let dotenv = Path::new(&s.directory).join(".env");
    if dotenv.exists() && !s.env_files.iter().any(|f| f == ".env") {
        bytes.extend_from_slice(&std::fs::read(path(Path::new(&s.directory), ".env")?)?);
    }
    Ok(digest(bytes))
}
async fn compose(s: &StackRevision, extra: &[&str]) -> Result<String> {
    let mut args = vec![
        "compose".to_string(),
        "--project-directory".into(),
        s.directory.clone(),
        "--project-name".into(),
        s.project.clone(),
    ];
    for f in &s.files {
        args.extend([
            "-f".into(),
            path(Path::new(&s.directory), f)?.display().to_string(),
        ]);
    }
    for f in &s.env_files {
        args.extend([
            "--env-file".into(),
            path(Path::new(&s.directory), f)?.display().to_string(),
        ]);
    }
    args.extend(extra.iter().map(|s| s.to_string()));
    command(
        "/usr/bin/docker",
        &args.iter().map(String::as_str).collect::<Vec<_>>(),
        300,
    )
    .await
}
pub async fn import(
    ctx: &ContextState,
    name: &str,
    directory: &str,
    files: &[String],
    env_files: &[String],
) -> Result<Value> {
    project(name)?;
    anyhow::ensure!(
        !files.is_empty() && files.len() <= 16 && env_files.len() <= 16,
        "必须指定原始 Compose 文件"
    );
    let dir = std::fs::canonicalize(directory)?;
    anyhow::ensure!(dir.is_dir(), "工作目录不存在");
    anyhow::ensure!(
        ctx.db.get::<StackRevision>("stacks", name).await?.is_none(),
        "Stack 已接管"
    );
    let mut s = StackRevision {
        project: name.into(),
        directory: dir.display().to_string(),
        files: files.to_vec(),
        env_files: env_files.to_vec(),
        fingerprint: String::new(),
        revision: 1,
        protected: false,
    };
    s.fingerprint = fingerprint(&s)?;
    let containers = super::docker::get("/containers/json?all=1").await?;
    for container in containers.as_array().context("容器发现结果无效")? {
        let labels = &container["Labels"];
        if labels["com.docker.compose.project"] != name {
            continue;
        }
        let original_directory = labels["com.docker.compose.project.working_dir"]
            .as_str()
            .context("已有项目缺少原工作目录标签，无法确认接管来源")?;
        anyhow::ensure!(
            std::fs::canonicalize(original_directory)? == dir,
            "填写目录与现有 Compose 项目标签不一致"
        );
        let original_files = labels["com.docker.compose.project.config_files"]
            .as_str()
            .context("已有项目缺少原配置文件标签")?;
        let expected: Vec<_> = files.iter().map(|f| path(&dir, f)).collect::<Result<_>>()?;
        let actual: Vec<_> = original_files
            .split(',')
            .map(|f| std::fs::canonicalize(dir.join(f)))
            .collect::<std::io::Result<_>>()?;
        anyhow::ensure!(
            expected == actual,
            "配置文件或合并顺序与现有 Compose 标签不一致"
        );
    }
    let config = compose(&s, &["config", "--format", "json"]).await?;
    let value: Value = serde_json::from_str(&config)?;
    s.protected = value["services"].as_object().is_some_and(|services| {
        services.values().any(|v| {
            [
                "mariadb",
                "mysql",
                "postgres",
                "openresty",
                "nginx",
                "haproxy",
                "proxysql",
            ]
            .iter()
            .any(|image| v["image"].as_str().unwrap_or("").contains(image))
        })
    });
    ctx.db.insert("stacks", name, &s).await?;
    Ok(json!(s))
}
pub async fn create(ctx: &ContextState, name: &str, content: &str) -> Result<Value> {
    project(name)?;
    anyhow::ensure!(content.len() < 512 * 1024, "Compose 配置过大");
    let _: Value = serde_yaml::from_str(content)?;
    anyhow::ensure!(
        ctx.db.get::<StackRevision>("stacks", name).await?.is_none(),
        "Stack 名称已存在"
    );
    let inventory = super::docker::inventory().await?;
    anyhow::ensure!(
        !inventory["containers"]
            .as_array()
            .context("容器发现失败")?
            .iter()
            .any(|c| c["project"] == name),
        "该项目名已有容器，请导入原配置而不是新建"
    );
    let directory = ctx.dir.canonicalize()?.join("stacks").join(name);
    anyhow::ensure!(!directory.exists(), "项目目录已存在，拒绝覆盖");
    std::fs::create_dir_all(&directory)?;
    pki::private_write(&directory.join("compose.yml"), content)?;
    let imported = import(
        ctx,
        name,
        directory.to_str().context("目录编码错误")?,
        &["compose.yml".into()],
        &[],
    )
    .await?;
    let s: StackRevision = serde_json::from_value(imported)?;
    let actual = compose(&s, &["config", "--format", "json"]).await?;
    let config: Value = serde_json::from_str(&actual)?;
    let p = Plan {
        id: id(),
        project: name.into(),
        base: s.fingerprint.clone(),
        content: content.into(),
        before: content.into(),
        expires: now() + 300,
    };
    ctx.db.insert("stack_plans", &p.id, &p).await?;
    Ok(
        json!({"plan_id":p.id,"expires":p.expires,"project":name,"services":service_summary(&config),"created":true}),
    )
}
fn service_summary(config: &Value) -> Vec<Value> {
    config["services"].as_object().map(|s|s.iter().map(|(name,v)|json!({"name":name,"image":v["image"],"ports":v["ports"],"volumes":v["volumes"]})).collect()).unwrap_or_default()
}
pub async fn plan(ctx: &ContextState, name: &str, content: &str) -> Result<Value> {
    project(name)?;
    anyhow::ensure!(content.len() < 512 * 1024, "Compose 配置过大");
    let s = ctx
        .db
        .get::<StackRevision>("stacks", name)
        .await?
        .context("先导入原始 Stack 配置")?;
    anyhow::ensure!(
        s.files.len() == 1,
        "多文件项目首版只支持原地运维，不能用单文件覆盖"
    );
    anyhow::ensure!(
        fingerprint(&s)? == s.fingerprint,
        "Compose 或环境文件发生外部变化，请重新核对接管"
    );
    let parsed: Value = serde_yaml::from_str(content)?;
    anyhow::ensure!(parsed["services"].is_object(), "Compose 缺少 services");
    let before = std::fs::read_to_string(path(Path::new(&s.directory), &s.files[0])?)?;
    let previous: Value =
        serde_json::from_str(&compose(&s, &["config", "--format", "json"]).await?)?;
    // 在原项目目录验证候选文件，保留相对路径和环境文件语义，不修改正在运行的配置。
    let candidate_name = format!(".opsd-plan-{}.yml", id());
    let candidate_path = Path::new(&s.directory).join(&candidate_name);
    pki::private_write(&candidate_path, content)?;
    let mut candidate = s.clone();
    candidate.files = vec![candidate_name];
    let validated = compose(&candidate, &["config", "--format", "json"]).await;
    let removed = std::fs::remove_file(&candidate_path);
    let next: Value = serde_json::from_str(&validated?)?;
    removed.context("候选配置清理失败")?;
    anyhow::ensure!(fingerprint(&s)? == s.fingerprint, "校验期间原配置发生变化");
    let p = Plan {
        id: id(),
        project: name.into(),
        base: s.fingerprint,
        content: content.into(),
        before,
        expires: now() + 300,
    };
    ctx.db.insert("stack_plans", &p.id, &p).await?;
    Ok(
        json!({"plan_id":p.id,"expires":p.expires,"before":service_summary(&previous),"after":service_summary(&next),"services":service_summary(&next),"protected":s.protected}),
    )
}
pub async fn apply(ctx: &ContextState, plan_id: &str) -> Result<Value> {
    let p = ctx
        .db
        .get::<Plan>("stack_plans", plan_id)
        .await?
        .context("部署计划不存在")?;
    anyhow::ensure!(p.expires > now(), "部署计划已过期");
    let mut s = ctx
        .db
        .get::<StackRevision>("stacks", &p.project)
        .await?
        .context("Stack 不存在")?;
    anyhow::ensure!(fingerprint(&s)? == p.base, "计划生成后配置已变化");
    let target = path(Path::new(&s.directory), &s.files[0])?;
    let revision_id = format!("{}:{}", s.project, s.revision);
    ctx.db.put("stack_backups", &revision_id, &p.before).await?;
    atomic_replace(&target, p.content.as_bytes())?;
    if let Err(e) = compose(&s, &["config", "--quiet"]).await {
        atomic_replace(&target, p.before.as_bytes())?;
        return Err(e.context("Compose 校验失败，已恢复配置文件"));
    }
    let applied = compose(&s, &["up", "-d", "--wait", "--wait-timeout", "90"]).await;
    s.revision += 1;
    s.fingerprint = fingerprint(&s)?;
    ctx.db.put("stacks", &s.project, &s).await?;
    let actual = compose(&s, &["ps", "--all", "--format", "json"]).await?;
    applied.context(format!(
        "部署未全部成功；当前服务结果：{actual}；配置已保留，可核对后重新部署上一版本"
    ))?;
    Ok(
        json!({"revision":s.revision,"services":actual,"network_check":"已读取 Docker 发布状态；外部可达性以防火墙验证结果为准"}),
    )
}
pub async fn control(ctx: &ContextState, name: &str, operation: &StackOperation) -> Result<Value> {
    let s = ctx
        .db
        .get::<StackRevision>("stacks", name)
        .await?
        .context("Stack 未接管")?;
    anyhow::ensure!(fingerprint(&s)? == s.fingerprint, "外部配置变更阻止执行");
    let args: Vec<&str> = match operation {
        StackOperation::Start => vec!["start"],
        StackOperation::Stop => vec!["stop", "--timeout", "20"],
        StackOperation::Remove => vec!["down", "--timeout", "20"],
    };
    compose(&s, &args).await?;
    let actual = compose(&s, &["ps", "--all", "--format", "json"]).await?;
    Ok(json!({"services":actual,"volumes_preserved":true}))
}
pub fn atomic_replace(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = path.with_extension(format!("opsd-{}", id()));
    pki::private_write(&tmp, bytes)?;
    if path.exists() {
        std::fs::set_permissions(&tmp, std::fs::metadata(path)?.permissions())?;
    }
    #[cfg(windows)]
    {
        std::fs::remove_file(path)?;
    }
    std::fs::rename(&tmp, path)?;
    #[cfg(unix)]
    {
        std::fs::File::open(path.parent().context("缺少父目录")?)?.sync_all()?;
    }
    Ok(())
}

#[cfg(all(test, target_os = "linux"))]
mod laboratory {
    use super::*;
    #[tokio::test]
    #[ignore = "需要专用 Linux Docker 实验室和离线测试镜像"]
    async fn docker_compose_preserves_identity_and_data() -> Result<()> {
        anyhow::ensure!(
            std::env::var("OPSD_LAB").as_deref() == Ok("1"),
            "必须指定实验室"
        );
        let temporary = tempfile::tempdir()?;
        let directory = temporary.path().join("project");
        std::fs::create_dir(&directory)?;
        let name = format!("opsd-test-{}", id());
        let content = "services:\n  worker:\n    image: opsd-lab-fixture:v1\n    command: [/bin/sleep, '3600']\n    network_mode: none\n    volumes: [state:/data]\nvolumes:\n  state: {}\n";
        std::fs::write(directory.join("compose.yml"), content)?;
        let ctx = ContextState {
            db: Store::open(&format!(
                "sqlite:{}?mode=rwc",
                temporary.path().join("node.db").display()
            ))
            .await?,
            dir: temporary.path().into(),
            config: Config {
                node_id: "实验节点".into(),
                hub: "https://localhost".into(),
                agent_url: "wss://localhost/agent".into(),
            },
            outbound: Default::default(),
            network: Default::default(),
        };
        let revision = StackRevision {
            project: name.clone(),
            directory: directory.display().to_string(),
            files: vec!["compose.yml".into()],
            env_files: vec![],
            fingerprint: String::new(),
            revision: 0,
            protected: false,
        };
        let result: Result<()> = async {
            compose(&revision, &["up", "-d", "--wait"]).await?;
            let cid = compose(&revision, &["ps", "-q"]).await?.trim().to_string();
            let before = super::super::docker::get(&format!("/containers/{cid}/json")).await?;
            import(&ctx, &name, &revision.directory, &revision.files, &[]).await?;
            anyhow::ensure!(
                compose(&revision, &["ps", "-q"]).await?.trim() == cid,
                "导入重建了容器"
            );
            for operation in [
                DockerOperation::Stop,
                DockerOperation::Start,
                DockerOperation::Restart,
            ] {
                super::super::docker::action(&cid, &operation).await?;
            }
            let after = super::super::docker::get(&format!("/containers/{cid}/json")).await?;
            anyhow::ensure!(
                before["Id"] == after["Id"] && before["Mounts"] == after["Mounts"],
                "容器或卷身份改变"
            );
            let invalid = content.replace("network_mode: none", "unknown_compose_property: true");
            anyhow::ensure!(
                plan(&ctx, &name, &invalid).await.is_err(),
                "非法 Compose 配置未被计划校验拒绝"
            );
            anyhow::ensure!(
                std::fs::read_to_string(directory.join("compose.yml"))? == content,
                "生成计划修改了原文件"
            );
            let valid = plan(&ctx, &name, content).await?;
            std::fs::write(directory.join(".env"), "EXTERNAL_CHANGE=1\n")?;
            anyhow::ensure!(
                apply(&ctx, valid["plan_id"].as_str().unwrap())
                    .await
                    .is_err(),
                "外部环境文件冲突未被拒绝"
            );
            std::fs::remove_file(directory.join(".env"))?;
            control(&ctx, &name, &StackOperation::Remove).await?;
            command(
                "/usr/bin/docker",
                &["volume", "inspect", &format!("{name}_state")],
                10,
            )
            .await?;
            Ok(())
        }
        .await;
        let _ = compose(&revision, &["down", "--volumes"]).await;
        let _ = command(
            "/usr/bin/docker",
            &["volume", "rm", &format!("{name}_state")],
            10,
        )
        .await;
        result
    }
}
