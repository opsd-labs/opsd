//! 宿主机受控终端。
//!
//! 「受控」体现在三处，缺一不可：
//!
//! - **固定 shell**：可执行文件从白名单里选，调用方不能指定任意程序。
//! - **固定环境**：不透传 Agent 自己的环境变量（里面有节点身份与主控地址），
//!   只给出终端所需的最小集合。
//! - **限定起始目录**：起始目录同样过路径闸门，不能从被禁目录起。
//!
//! 会话本身不在这里做权限判断——主控在建立会话前已经校验管理员身份并写入审计，
//! Agent 只负责在授权范围内执行。
use super::files::Jail;
use anyhow::{Context, Result};
use std::process::Stdio;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::mpsc::Receiver,
};

/// 允许使用的 shell。不接受调用方指定任意可执行文件。
pub const ALLOWED_SHELLS: &[&str] = &["/bin/bash", "/bin/sh", "/usr/bin/bash", "/usr/bin/sh"];

pub fn validate_shell(shell: &str) -> Result<()> {
    anyhow::ensure!(
        ALLOWED_SHELLS.contains(&shell),
        "只允许使用 {ALLOWED_SHELLS:?} 中的 shell"
    );
    anyhow::ensure!(
        std::path::Path::new(shell).is_file(),
        "该 shell 在当前节点上不存在：{shell}"
    );
    Ok(())
}

/// 计算会话可用的起始目录。未指定或不可用时回落到 `/`。
pub fn resolve_workdir(jail: &Jail, workdir: Option<&str>) -> String {
    let Some(requested) = workdir else {
        return "/".into();
    };
    match jail.resolve(requested) {
        Ok(path) if path.is_dir() => path.display().to_string(),
        _ => "/".into(),
    }
}

/// 打开一个受控的宿主机终端。
///
/// 返回的读取端输出原始字节；调用方负责编码后送入流通道。
pub struct Session {
    pub child: tokio::process::Child,
}

impl Session {
    pub async fn spawn(jail: &Jail, shell: &str, workdir: Option<&str>) -> Result<Self> {
        validate_shell(shell)?;
        let cwd = resolve_workdir(jail, workdir);
        let child = tokio::process::Command::new(shell)
            // 交互式登录 shell；TERM 固定，避免依赖客户端声明
            .arg("-l")
            .current_dir(&cwd)
            .env_clear()
            .env("TERM", "xterm-256color")
            .env("PATH", "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin")
            .env("LANG", "C.UTF-8")
            .env("HOME", "/root")
            .env("SHELL", shell)
            // 让 shell 认为自己挂在终端上，行为与手工登录一致
            .env("PS1", "\\u@\\h:\\w\\$ ")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .with_context(|| format!("无法启动 {shell}"))?;
        Ok(Self { child })
    }

    /// 把会话的输入输出接到流通道上。
    ///
    /// 输入是界面送来的 base64 分块（与其它流一致），解码后写入 stdin；
    /// 输出编码后以同样的方式回传。终端不在这里做权限判断——主控在建立会话前
    /// 已经校验管理员身份并写入审计。
    pub async fn pump(
        mut self,
        mut input: Receiver<String>,
        out: tokio::sync::mpsc::Sender<opsd::protocol::Frame>,
        stream_id: String,
    ) -> Result<()> {
        use base64::Engine;
        let engine = base64::engine::general_purpose::STANDARD;
        let mut stdin = self.child.stdin.take().context("缺少标准输入")?;
        let mut stdout = self.child.stdout.take().context("缺少标准输出")?;
        let mut stderr = self.child.stderr.take().context("缺少标准错误")?;
        let mut out_buffer = [0u8; 8192];
        let mut err_buffer = [0u8; 8192];
        loop {
            tokio::select! {
                Some(chunk) = input.recv() => {
                    let data = engine.decode(&chunk)?;
                    stdin.write_all(&data).await?;
                    stdin.flush().await?;
                }
                read = stdout.read(&mut out_buffer) => {
                    let count = read?;
                    if count == 0 { break; }
                    let frame = opsd::protocol::Frame::StreamData {
                        stream_id: stream_id.clone(),
                        data: engine.encode(&out_buffer[..count]),
                    };
                    if out.send(frame).await.is_err() { break; }
                }
                read = stderr.read(&mut err_buffer) => {
                    let count = read?;
                    if count == 0 { continue; }
                    let frame = opsd::protocol::Frame::StreamData {
                        stream_id: stream_id.clone(),
                        data: engine.encode(&err_buffer[..count]),
                    };
                    if out.send(frame).await.is_err() { break; }
                }
            }
        }
        let _ = self.child.kill().await;
        Ok(())
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn 只允许白名单内的_shell() {
        for bad in [
            "/usr/bin/python3",
            "/bin/rm",
            "/bin/bash -c id",
            "bash",
            "/tmp/evil",
            "",
        ] {
            assert!(
                validate_shell(bad).is_err(),
                "{bad:?} 不在白名单内，必须被拒绝"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn 白名单内的_shell_必须真实存在() {
        // /bin/sh 在 Unix 上必然存在
        validate_shell("/bin/sh").unwrap();
    }

    #[test]
    fn 起始目录回落到根且拒绝被禁路径() {
        let jail = Jail::new(vec![], vec![]);
        assert_eq!(resolve_workdir(&jail, None), "/");
        // 受保护路径不可作为起始目录
        assert_eq!(resolve_workdir(&jail, Some("/proc/self")), "/");
        // 不存在的目录也回落到根
        assert_eq!(resolve_workdir(&jail, Some("/definitely/not/here")), "/");
        // 正常目录原样使用
        assert_eq!(resolve_workdir(&jail, Some("/tmp")), "/tmp");
    }
}
