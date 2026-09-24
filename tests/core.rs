use opsd::{protocol::*, store::Store};
#[tokio::test]
async fn 幂等键拒绝改变参数且不跨节点复用结果() {
    let dir = tempfile::tempdir().unwrap();
    let db = Store::open(&format!(
        "sqlite://{}?mode=rwc",
        dir.path().join("test.db").display()
    ))
    .await
    .unwrap();
    let first = TaskEnvelope::new("node-a".into(), "same-key".into(), Action::Inspect {});
    db.accept_task(&first).await.unwrap();
    let repeat = TaskEnvelope::new("node-a".into(), "same-key".into(), Action::Inspect {});
    assert_eq!(db.accept_task(&repeat).await.unwrap().id, first.id);
    let different = TaskEnvelope::new(
        "node-a".into(),
        "same-key".into(),
        Action::PullImage {
            reference: "nginx:stable".into(),
        },
    );
    assert!(db.accept_task(&different).await.is_err());
    let other = TaskEnvelope::new("node-b".into(), "same-key".into(), Action::Inspect {});
    assert_ne!(db.accept_task(&other).await.unwrap().id, first.id);
    let mut forged = first.clone();
    forged.node_id = "node-b".into();
    assert!(db.save_task(&forged).await.is_err());
}
#[tokio::test]
async fn 重启后仍保留任务而不把运行中视为成功() {
    let dir = tempfile::tempdir().unwrap();
    let url = format!("sqlite://{}?mode=rwc", dir.path().join("test.db").display());
    let db = Store::open(&url).await.unwrap();
    let mut t = TaskEnvelope::new(
        "node".into(),
        "restart".into(),
        Action::Docker {
            container: "abc".into(),
            operation: DockerOperation::Restart,
        },
    );
    db.accept_task(&t).await.unwrap();
    t.status = TaskStatus::Running;
    db.save_task(&t).await.unwrap();
    db.pool.close().await;
    let reopened = Store::open(&url).await.unwrap();
    assert_eq!(
        reopened.task(&t.id).await.unwrap().unwrap().status,
        TaskStatus::Running
    );
}
#[test]
fn 节点地址不把私网与保留地址当公网() {
    for ip in [
        "127.0.0.1",
        "10.1.2.3",
        "100.100.201.1",
        "192.0.2.5",
        "169.254.1.2",
        "::1",
        "fe80::1",
        "fd00::1",
        "2001:db8::1",
        "::ffff:8.8.8.8",
    ] {
        assert!(validate_public_address(ip).is_err(), "{ip}");
    }
    for ip in ["8.8.8.8", "2606:4700:4700::1111"] {
        assert!(validate_public_address(ip).is_ok(), "{ip}");
    }
}
#[test]
fn 任务协议拒绝任意宿主机命令() {
    assert!(serde_json::from_str::<Action>(r#"{"type":"shell","command":"id"}"#).is_err());
    assert!(serde_json::from_str::<Action>(r#"{"type":"inspect","command":"id"}"#).is_err());
}

#[test]
fn 管理界面任务不返回配置秘密且不修改原任务() {
    let t = TaskEnvelope::new(
        "node".into(),
        "secret".into(),
        Action::StackPlan {
            project: "database".into(),
            content: "PASSWORD=secret-example".into(),
        },
    );
    assert!(
        !serde_json::to_string(&t.redacted())
            .unwrap()
            .contains("secret-example")
    );
    assert!(
        serde_json::to_string(&t)
            .unwrap()
            .contains("secret-example")
    );
    assert_eq!(t.digest, t.redacted().digest);
}

#[tokio::test]
async fn 多记录事务失败不留下半提交策略() {
    let dir = tempfile::tempdir().unwrap();
    let db = Store::open(&format!(
        "sqlite:{}?mode=rwc",
        dir.path().join("atomic.db").display()
    ))
    .await
    .unwrap();
    db.put("firewall", "policy", &"old").await.unwrap();
    sqlx::query("CREATE TRIGGER simulate_disk_error BEFORE INSERT ON records WHEN NEW.bucket='recovery' BEGIN SELECT RAISE(ABORT, '模拟存储失败'); END")
        .execute(&db.pool).await.unwrap();
    assert!(
        db.put_records(&[
            ("firewall", "policy", "\"new\"".into()),
            ("recovery", "operation", "\"committed\"".into())
        ])
        .await
        .is_err()
    );
    assert_eq!(
        db.get::<String>("firewall", "policy")
            .await
            .unwrap()
            .unwrap(),
        "old"
    );
    assert!(
        db.get::<String>("recovery", "operation")
            .await
            .unwrap()
            .is_none()
    );
}
