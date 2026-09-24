use crate::protocol::*;
use anyhow::{Result, bail};
use serde::{Serialize, de::DeserializeOwned};
use sqlx::{AnyPool, Row, any::AnyPoolOptions};

#[derive(Clone)]
pub struct Store {
    pub pool: AnyPool,
}
impl Store {
    pub async fn open(url: &str) -> Result<Self> {
        let normalized = url.replace('\\', "/");
        let normalized = if normalized.starts_with("sqlite://") {
            normalized.replacen("sqlite://", "sqlite:", 1)
        } else {
            normalized
        };
        let url = normalized.as_str();
        sqlx::any::install_default_drivers();
        if let Some(path) = url
            .strip_prefix("sqlite:")
            .and_then(|s| s.split('?').next())
            && let Some(parent) = std::path::Path::new(path).parent()
        {
            std::fs::create_dir_all(parent)?;
        }
        let sqlite = url.starts_with("sqlite:");
        let pool = AnyPoolOptions::new()
            .max_connections(if sqlite { 1 } else { 8 })
            .after_connect(move |c, _| {
                Box::pin(async move {
                    if sqlite {
                        for s in [
                            "PRAGMA journal_mode=WAL",
                            "PRAGMA synchronous=FULL",
                            "PRAGMA foreign_keys=ON",
                            "PRAGMA busy_timeout=5000",
                        ] {
                            sqlx::query(s).execute(&mut *c).await?;
                        }
                    }
                    Ok(())
                })
            })
            .connect(url)
            .await?;
        for statement in [
            "CREATE TABLE IF NOT EXISTS records (bucket VARCHAR(48) NOT NULL, id VARCHAR(128) NOT NULL, value LONGTEXT NOT NULL, PRIMARY KEY(bucket,id))",
            "CREATE TABLE IF NOT EXISTS tasks (id VARCHAR(64) PRIMARY KEY, node_id VARCHAR(64) NOT NULL, task_key VARCHAR(128) NOT NULL, digest VARCHAR(64) NOT NULL, value LONGTEXT NOT NULL, UNIQUE(node_id,task_key))",
            // 指标按层级存放：raw 为原始采样，m5/h1 为降采样桶。
            // 桶内保留平均后的标量与最后一次的明细，见 hub::metrics。
            "CREATE TABLE IF NOT EXISTS metrics (node_id VARCHAR(64) NOT NULL, tier VARCHAR(8) NOT NULL, at BIGINT NOT NULL, value LONGTEXT NOT NULL, PRIMARY KEY(node_id,tier,at))",
            "CREATE INDEX IF NOT EXISTS metrics_tier_at ON metrics(tier,at)",
        ] {
            sqlx::query(statement).execute(&pool).await?;
        }
        Ok(Self { pool })
    }
    /// 写入或覆盖一条指标记录。SQLite 与 MySQL 都支持 `REPLACE INTO`。
    pub async fn metrics_put(
        &self,
        node_id: &str,
        tier: &str,
        at: i64,
        value: &str,
    ) -> Result<()> {
        sqlx::query("REPLACE INTO metrics(node_id,tier,at,value) VALUES(?,?,?,?)")
            .bind(node_id)
            .bind(tier)
            .bind(at)
            .bind(value)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
    /// 按时间范围读取指标，按时间升序返回。
    pub async fn metrics_range(
        &self,
        node_id: &str,
        tier: &str,
        from: i64,
        to: i64,
        limit: i64,
    ) -> Result<Vec<(i64, String)>> {
        let rows = sqlx::query(
            "SELECT at,value FROM metrics WHERE node_id=? AND tier=? AND at>=? AND at<=? ORDER BY at LIMIT ?",
        )
        .bind(node_id)
        .bind(tier)
        .bind(from)
        .bind(to)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|r| Ok((r.try_get::<i64, _>("at")?, r.try_get::<String, _>("value")?)))
            .collect()
    }
    /// 删除某层级中早于给定时刻的记录，返回删除条数。
    pub async fn metrics_prune(&self, tier: &str, before: i64) -> Result<u64> {
        Ok(
            sqlx::query("DELETE FROM metrics WHERE tier=? AND at<?")
                .bind(tier)
                .bind(before)
                .execute(&self.pool)
                .await?
                .rows_affected(),
        )
    }
    /// 某层级当前的记录数与最早时间，用于容量观察。
    pub async fn metrics_summary(&self, tier: &str) -> Result<(i64, Option<i64>)> {
        let row = sqlx::query("SELECT COUNT(*) AS n, MIN(at) AS oldest FROM metrics WHERE tier=?")
            .bind(tier)
            .fetch_one(&self.pool)
            .await?;
        Ok((row.try_get::<i64, _>("n")?, row.try_get::<Option<i64>, _>("oldest")?))
    }
    pub async fn get<T: DeserializeOwned>(&self, bucket: &str, id: &str) -> Result<Option<T>> {
        sqlx::query("SELECT value FROM records WHERE bucket=? AND id=?")
            .bind(bucket)
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .map(|r| Ok(serde_json::from_str(&r.try_get::<String, _>("value")?)?))
            .transpose()
    }
    pub async fn list<T: DeserializeOwned>(&self, bucket: &str) -> Result<Vec<T>> {
        sqlx::query("SELECT value FROM records WHERE bucket=? ORDER BY id")
            .bind(bucket)
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(|r| Ok(serde_json::from_str(&r.try_get::<String, _>("value")?)?))
            .collect()
    }
    pub async fn put<T: Serialize>(&self, bucket: &str, id: &str, value: &T) -> Result<()> {
        self.put_records(&[(bucket, id, serde_json::to_string(value)?)])
            .await
    }
    pub async fn put_records(&self, records: &[(&str, &str, String)]) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        for (bucket, id, value) in records {
            sqlx::query("DELETE FROM records WHERE bucket=? AND id=?")
                .bind(*bucket)
                .bind(*id)
                .execute(&mut *tx)
                .await?;
            sqlx::query("INSERT INTO records(bucket,id,value) VALUES(?,?,?)")
                .bind(*bucket)
                .bind(*id)
                .bind(value)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }
    pub async fn insert<T: Serialize>(&self, bucket: &str, id: &str, value: &T) -> Result<()> {
        sqlx::query("INSERT INTO records(bucket,id,value) VALUES(?,?,?)")
            .bind(bucket)
            .bind(id)
            .bind(serde_json::to_string(value)?)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
    pub async fn delete(&self, bucket: &str, id: &str) -> Result<u64> {
        Ok(sqlx::query("DELETE FROM records WHERE bucket=? AND id=?")
            .bind(bucket)
            .bind(id)
            .execute(&self.pool)
            .await?
            .rows_affected())
    }
    pub async fn accept_task(&self, task: &TaskEnvelope) -> Result<TaskEnvelope> {
        anyhow::ensure!(
            task.digest == digest(serde_json::to_vec(&task.action)?),
            "任务参数摘要不匹配"
        );
        let result =
            sqlx::query("INSERT INTO tasks(id,node_id,task_key,digest,value) VALUES(?,?,?,?,?)")
                .bind(&task.id)
                .bind(&task.node_id)
                .bind(&task.key)
                .bind(&task.digest)
                .bind(serde_json::to_string(task)?)
                .execute(&self.pool)
                .await;
        match result {
            Ok(_) => Ok(task.clone()),
            Err(e) => {
                let row = sqlx::query("SELECT value FROM tasks WHERE node_id=? AND task_key=?")
                    .bind(&task.node_id)
                    .bind(&task.key)
                    .fetch_optional(&self.pool)
                    .await?;
                if let Some(row) = row {
                    let old: TaskEnvelope =
                        serde_json::from_str(&row.try_get::<String, _>("value")?)?;
                    if old.digest != task.digest {
                        bail!("幂等键已用于不同参数");
                    }
                    Ok(old)
                } else {
                    Err(e.into())
                }
            }
        }
    }
    pub async fn task(&self, id: &str) -> Result<Option<TaskEnvelope>> {
        sqlx::query("SELECT value FROM tasks WHERE id=?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .map(|r| Ok(serde_json::from_str(&r.try_get::<String, _>("value")?)?))
            .transpose()
    }
    pub async fn tasks(&self) -> Result<Vec<TaskEnvelope>> {
        sqlx::query("SELECT value FROM tasks ORDER BY id DESC")
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(|r| Ok(serde_json::from_str(&r.try_get::<String, _>("value")?)?))
            .collect()
    }
    pub async fn save_task(&self, t: &TaskEnvelope) -> Result<()> {
        let changed = sqlx::query("UPDATE tasks SET value=? WHERE id=? AND node_id=? AND digest=?")
            .bind(serde_json::to_string(t)?)
            .bind(&t.id)
            .bind(&t.node_id)
            .bind(&t.digest)
            .execute(&self.pool)
            .await?
            .rows_affected();
        anyhow::ensure!(changed == 1, "任务目标或摘要不匹配");
        Ok(())
    }
}
