use opsd::protocol::AgentInstallMode;
use serde::Serialize;

#[derive(Serialize)]
pub struct Installation {
    release: String,
    script: String,
}

pub struct Input<'a> {
    pub node_id: &'a str,
    pub token: &'a str,
    pub expires_at: i64,
    pub ca_pem: &'a str,
    pub fingerprint: &'a str,
    pub mode: &'a AgentInstallMode,
    pub hub: &'a str,
    pub agent_url: &'a str,
    pub agent_image: &'a str,
}

// 发布构建注入固定发行标签，开发构建不猜测 latest 或 main。
pub fn render(input: Input<'_>) -> Option<Installation> {
    let release = std::env::var("OPSD_AGENT_RELEASE").ok()?;
    render_release(input, &release)
}

fn render_release(input: Input<'_>, release: &str) -> Option<Installation> {
    let revision = release.strip_prefix("agent-")?;
    if revision.len() != 40 || !revision.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    if input.mode == &AgentInstallMode::Docker {
        let digest = input
            .agent_image
            .strip_prefix("ghcr.io/opsd-labs/opsd-agent@sha256:")?;
        if digest.len() != 64 || !digest.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
    }
    let mode = if input.mode == &AgentInstallMode::Host {
        "host"
    } else {
        "docker"
    };
    let mut config = String::new();
    for (name, value) in [
        ("node_id", input.node_id),
        ("hub", input.hub),
        ("agent_url", input.agent_url),
        ("mode", mode),
        ("release", release),
        ("agent_image", input.agent_image),
        ("fingerprint", input.fingerprint),
        ("token", input.token),
        ("ca_pem", input.ca_pem),
        (
            "service_unit",
            include_str!("../../deploy/opsd-agent.service"),
        ),
    ] {
        config.push_str(&format!("{name}={}\n", shell_quote(value)));
    }
    config.push_str(&format!("expires_at={}\n", input.expires_at));
    Some(Installation {
        release: release.to_owned(),
        script: include_str!("../../deploy/install-agent.sh")
            .replace("__OPSD_INSTALL_CONFIG__", &config)
            .replace("\r\n", "\n"),
    })
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 安装参数按单个_shell_字符串处理且发行版必须固定() {
        let input = || Input {
            node_id: "测试节点",
            token: "一次性'令牌",
            expires_at: 600,
            ca_pem: "公共证书\n第二行",
            fingerprint: "指纹",
            mode: &AgentInstallMode::Host,
            hub: "https://localhost:65535",
            agent_url: "wss://localhost:8444/agent",
            agent_image: "ghcr.io/opsd-labs/opsd-agent:main",
        };
        assert!(render_release(input(), "main").is_none());
        let package = render_release(input(), &format!("agent-{}", "a".repeat(40))).unwrap();
        assert!(package.script.contains("token='一次性'\\''令牌'"));
        assert!(package.script.contains("ca_pem='公共证书\n第二行'"));
        assert!(!package.script.contains("__OPSD_INSTALL_CONFIG__"));
        assert!(
            render_release(
                Input {
                    mode: &AgentInstallMode::Docker,
                    ..input()
                },
                &package.release
            )
            .is_none()
        );
    }
}
