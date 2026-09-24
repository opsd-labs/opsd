//! 结构化文件操作与宿主机终端。
//!
//! # 为什么文件操作必须是结构化的
//!
//! 如果让界面拼 shell 命令去 `ls`、`cat`、`rm`，主控就既无法审计也无法约束：
//! 审计只能记录"执行了一条命令"，权限只能靠黑名单字符串。这里把每个动作拆成独立的
//! 结构化操作，路径在 Agent 侧强制校验，**不经过任何 shell**。
//!
//! # 四条防线
//!
//! 1. **路径规范化**：只接受绝对路径，词法展开 `.` 与 `..`，并在校验前先解析已存在部分的
//!    符号链接，避免通过目录软链逃逸。
//! 2. **拒绝符号链接叶子**：打开文件时带 `O_NOFOLLOW`；无法使用该标志的操作
//!    （列目录、删除、重命名）显式检查 `symlink_metadata`。
//! 3. **硬禁前缀**：`/proc`、`/sys`、`/dev`、Docker socket，以及 Agent 自己的数据目录与
//!    PKI 目录——后者是运行时注入的，不能写死。
//! 4. **体积上限**：单次读取、单次写入与递归删除的条目数都有上限。
//!
//! 递归删除另外要求调用方显式确认，避免误触。所有变更操作与节点变更锁协调，
//! 不会与防火墙或存储任务同时改动同一台机器。
//!
//! # 平台假设
//!
//! 路径闸门按 **Unix 路径语义**设计：绝对路径以 `/` 开头，受保护前缀是 `/proc`
//! 这类固定位置。Agent 只在 Linux 上运行，因此判定与测试都以 Unix 为准；
//! 在 Windows 上模块仍能编译，但语义没有意义（那里也没有 `/proc` 可保护）。
use opsd::protocol::{DirEntry, FileStat};
use anyhow::{Context, Result};
use std::{
    io::{Read, Seek, SeekFrom},
    path::{Component, Path, PathBuf},
};

/// 单次读取返回的最大字节数。
pub const MAX_READ_BYTES: u64 = 1024 * 1024;
/// 单次写入允许的最大字节数。
pub const MAX_WRITE_BYTES: u64 = 64 * 1024 * 1024;
/// 递归删除允许的条目数上限。
pub const MAX_RECURSIVE_ENTRIES: usize = 10_000;
/// 列目录返回的条目上限。
pub const MAX_DIR_ENTRIES: usize = 5_000;
/// 禁止访问的路径前缀。这些位置的改动会破坏系统或泄漏密钥。
pub const DENY_PREFIXES: &[&str] = &["/proc", "/sys", "/dev", "/run/docker.sock"];

/// 路径闸门。除固定前缀外，还会拒绝 Agent 自己的数据目录与 PKI 目录。
#[derive(Clone)]
pub struct Jail {
    /// 允许访问的根；默认是整个文件系统，运维可以收窄。
    roots: Vec<PathBuf>,
    /// 额外禁止的路径（Agent 数据目录、PKI 目录等）。
    deny: Vec<PathBuf>,
}

impl Jail {
    /// `roots` 为空时默认取 `/`。`deny` 由调用方注入运行时敏感目录。
    pub fn new(roots: Vec<PathBuf>, deny: Vec<PathBuf>) -> Self {
        let roots = if roots.is_empty() {
            vec![PathBuf::from("/")]
        } else {
            roots
                .into_iter()
                .map(|root| normalize(&root))
                .collect()
        };
        Self {
            roots,
            deny: deny.into_iter().map(|path| normalize(&path)).collect(),
        }
    }

    /// 校验并返回可用于系统调用的路径。
    ///
    /// 返回的是**词法规范化后的原路径**，而不是解析软链后的目标：
    /// 叶子是否符号链接由打开方式（`O_NOFOLLOW`）决定，提前解析反而会掩盖攻击意图。
    pub fn resolve(&self, input: &str) -> Result<PathBuf> {
        anyhow::ensure!(!input.is_empty(), "路径不能为空");
        anyhow::ensure!(!input.contains('\0'), "路径包含非法字符");
        let path = Path::new(input);
        anyhow::ensure!(path.is_absolute(), "只接受绝对路径");
        let normalized = normalize(path);
        self.check(&normalized)?;
        // 已存在部分的软链也要解析，否则可以通过目录软链指向被禁目录
        if let Some(existing) = canonical_prefix(&normalized) {
            self.check(&existing)?;
        }
        Ok(normalized)
    }

    /// 前缀检查。用组件边界比较，避免 `/procx` 被误判为 `/proc` 的子路径。
    fn check(&self, path: &Path) -> Result<()> {
        for denied in DENY_PREFIXES.iter().map(PathBuf::from).chain(self.deny.clone()) {
            if path.starts_with(&denied) {
                anyhow::bail!("该路径属于受保护范围，禁止操作：{}", denied.display());
            }
        }
        anyhow::ensure!(
            self.roots.iter().any(|root| path.starts_with(root)),
            "该路径不在允许的范围内"
        );
        Ok(())
    }
}

/// 词法规范化：展开 `.` 与 `..`，不触碰文件系统。
fn normalize(path: &Path) -> PathBuf {
    let mut result = PathBuf::from("/");
    for component in path.components() {
        match component {
            Component::RootDir | Component::Prefix(_) => {}
            Component::CurDir => {}
            Component::ParentDir => {
                result.pop();
            }
            Component::Normal(part) => result.push(part),
        }
    }
    result
}

/// 找到路径中最长的已存在前缀并解析其软链，再把剩余部分接回去。
fn canonical_prefix(path: &Path) -> Option<PathBuf> {
    let mut remaining: Vec<&std::ffi::OsStr> = Vec::new();
    let mut cursor = path;
    loop {
        if let Ok(resolved) = std::fs::canonicalize(cursor) {
            let mut result = resolved;
            for part in remaining.iter().rev() {
                result.push(part);
            }
            return Some(result);
        }
        let name = cursor.file_name()?;
        remaining.push(name);
        cursor = cursor.parent()?;
        if cursor.as_os_str().is_empty() {
            return None;
        }
    }
}

/// 拒绝符号链接叶子。用于无法用 `O_NOFOLLOW` 覆盖的操作。
fn ensure_not_symlink(path: &Path) -> Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => {
            anyhow::bail!("拒绝操作符号链接：{}", path.display())
        }
        _ => Ok(()),
    }
}

fn stat_of(path: &Path, name: String) -> Result<FileStat> {
    let meta = std::fs::symlink_metadata(path)?;
    let kind = if meta.file_type().is_symlink() {
        "symlink"
    } else if meta.is_dir() {
        "dir"
    } else {
        "file"
    };
    Ok(FileStat {
        name,
        path: path.display().to_string(),
        kind: kind.into(),
        size: meta.len(),
        modified: meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0),
        readonly: meta.permissions().readonly(),
    })
}

/// 列目录。**不跟随条目符号链接的名称判断**：符号链接单列，不会伪装成目录。
pub fn list(jail: &Jail, path: &str) -> Result<Vec<DirEntry>> {
    let target = jail.resolve(path)?;
    ensure_not_symlink(&target)?;
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(&target)
        .with_context(|| format!("无法读取目录：{}", target.display()))?
    {
        if entries.len() >= MAX_DIR_ENTRIES {
            anyhow::bail!("目录条目超过 {MAX_DIR_ENTRIES} 条，请分批查看");
        }
        let entry = entry?;
        let meta = std::fs::symlink_metadata(entry.path());
        let Ok(meta) = meta else { continue };
        let kind = if meta.file_type().is_symlink() {
            "symlink"
        } else if meta.is_dir() {
            "dir"
        } else {
            "file"
        };
        entries.push(DirEntry {
            name: entry.file_name().to_string_lossy().into_owned(),
            kind: kind.into(),
            size: meta.len(),
            modified: meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0),
            readonly: meta.permissions().readonly(),
        });
    }
    entries.sort_by(|a, b| {
        // 目录在前，其余按名称，保证界面顺序稳定
        (a.kind != "dir", a.name.to_lowercase()).cmp(&(b.kind != "dir", b.name.to_lowercase()))
    });
    Ok(entries)
}

/// 读取文件片段。带 `O_NOFOLLOW`，符号链接叶子直接失败。
pub fn read(jail: &Jail, path: &str, offset: u64, limit: u64) -> Result<Vec<u8>> {
    let target = jail.resolve(path)?;
    let mut file = open_no_follow(&target, false)?;
    anyhow::ensure!(
        file.metadata()?.is_file(),
        "只允许读取普通文件"
    );
    let limit = limit.clamp(1, MAX_READ_BYTES);
    if offset > 0 {
        file.seek(SeekFrom::Start(offset))?;
    }
    let mut buffer = Vec::new();
    file.take(limit).read_to_end(&mut buffer)?;
    Ok(buffer)
}

/// 上传暂存路径。**由目标路径确定**，因此内容传完后任务不需要额外信息
/// 就能找到它并核对摘要；并发上传同一路径时后写者会覆盖暂存内容，
/// 前一个任务随即因摘要不符而失败——这比让错误内容落盘安全得多。
pub fn staging_path(jail: &Jail, path: &str) -> Result<PathBuf> {
    let target = jail.resolve(path)?;
    let parent = target
        .parent()
        .ok_or_else(|| anyhow::anyhow!("目标路径没有父目录"))?;
    let tag = &opsd::protocol::digest(target.display().to_string())[..16];
    Ok(parent.join(format!(".opsd-upload-{tag}")))
}

/// 把上传的暂存内容校验后就位。
///
/// 这是上传的**最后一步**，也是最关键的一步：只有暂存文件的摘要与大小都与任务里
/// 声明的一致，才会原子改名覆盖目标；任何不符都会失败，**原文件保持原样**。
///
/// 校验失败时**不删除暂存内容**：任务可能因网络或主控重启而重试，删掉就无法重试了。
/// 暂存文件名由目标路径决定，下一次上传会直接覆盖它；彻底放弃时由
/// [`discard_staged`] 清理——删除目标文件时也会顺带清掉。
pub fn put_staged(jail: &Jail, path: &str, digest: &str, size: u64) -> Result<u64> {
    let target = jail.resolve(path)?;
    let staged = staging_path(jail, path)?;
    let meta = std::fs::symlink_metadata(&staged)
        .with_context(|| "上传内容尚未就位，请先完成分块传输")?;
    anyhow::ensure!(!meta.file_type().is_symlink(), "暂存文件不应是符号链接");
    anyhow::ensure!(meta.len() <= MAX_WRITE_BYTES, "上传内容超过单文件上限");
    anyhow::ensure!(meta.len() == size, "上传内容大小与声明不一致");
    let bytes = std::fs::read(&staged)?;
    anyhow::ensure!(
        opsd::protocol::digest(&bytes) == digest,
        "上传内容摘要与声明不一致"
    );
    ensure_not_symlink(&target)?;
    std::fs::rename(&staged, &target)?;
    Ok(meta.len())
}

/// 清理未完成的暂存上传。删除目标文件时一并调用，避免留下半截内容。
pub fn discard_staged(jail: &Jail, path: &str) -> Result<()> {
    let staged = staging_path(jail, path)?;
    match std::fs::remove_file(&staged) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

pub fn mkdir(jail: &Jail, path: &str) -> Result<()> {
    let target = jail.resolve(path)?;
    ensure_not_symlink(&target)?;
    std::fs::create_dir(&target).with_context(|| format!("创建目录失败：{}", target.display()))?;
    Ok(())
}

/// 删除。递归删除必须由调用方显式确认，并限制条目数。
pub fn remove(jail: &Jail, path: &str, recursive: bool, confirmed: bool) -> Result<u64> {
    let target = jail.resolve(path)?;
    // 不允许删除根或受保护目录本身
    anyhow::ensure!(target.parent().is_some(), "拒绝删除根目录");
    ensure_not_symlink(&target)?;
    let meta = std::fs::symlink_metadata(&target)
        .with_context(|| format!("目标不存在：{}", target.display()))?;
    if meta.is_dir() {
        let entries = count_entries(&target)?;
        if entries > 0 {
            // 非空目录必须同时声明递归并显式确认，缺一不可
            anyhow::ensure!(recursive, "目标是非空目录，需要显式确认递归删除");
            anyhow::ensure!(confirmed, "递归删除需要显式确认");
            anyhow::ensure!(
                entries <= MAX_RECURSIVE_ENTRIES,
                "目录条目超过 {MAX_RECURSIVE_ENTRIES} 条，请先分批清理"
            );
            std::fs::remove_dir_all(&target)?;
            return Ok(entries as u64);
        }
        // 空目录直接删除即可，不需要递归确认
        std::fs::remove_dir(&target)?;
        return Ok(1);
    }
    std::fs::remove_file(&target)?;
    Ok(1)
}

fn count_entries(path: &Path) -> Result<usize> {
    let mut total = 0;
    let mut stack = vec![path.to_path_buf()];
    while let Some(current) = stack.pop() {
        for entry in std::fs::read_dir(&current)? {
            let entry = entry?;
            total += 1;
            anyhow::ensure!(total <= MAX_RECURSIVE_ENTRIES, "条目过多");
            if entry.file_type()?.is_dir() {
                stack.push(entry.path());
            }
        }
    }
    Ok(total)
}

pub fn rename(jail: &Jail, from: &str, to: &str) -> Result<()> {
    let source = jail.resolve(from)?;
    let target = jail.resolve(to)?;
    ensure_not_symlink(&source)?;
    ensure_not_symlink(&target)?;
    std::fs::rename(&source, &target)
        .with_context(|| format!("重命名失败：{} → {}", source.display(), target.display()))?;
    Ok(())
}

#[cfg(unix)]
pub fn chmod(jail: &Jail, path: &str, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    anyhow::ensure!(mode <= 0o7777, "权限位不合法");
    let target = jail.resolve(path)?;
    ensure_not_symlink(&target)?;
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(mode))?;
    Ok(())
}

#[cfg(not(unix))]
pub fn chmod(_jail: &Jail, _path: &str, _mode: u32) -> Result<()> {
    anyhow::bail!("该平台不支持修改文件权限")
}

#[cfg(unix)]
pub fn chown(jail: &Jail, path: &str, uid: u32, gid: u32) -> Result<()> {
    use std::ffi::CString;
    let target = jail.resolve(path)?;
    ensure_not_symlink(&target)?;
    // 显式使用 lchown 语义：即便竞态中出现软链也不会跟随
    let raw = CString::new(target.display().to_string())?;
    // SAFETY: 传入的是以 NUL 结尾的合法路径
    let result = unsafe { libc::lchown(raw.as_ptr(), uid, gid) };
    anyhow::ensure!(result == 0, "修改属主失败：{}", std::io::Error::last_os_error());
    Ok(())
}

#[cfg(not(unix))]
pub fn chown(_jail: &Jail, _path: &str, _uid: u32, _gid: u32) -> Result<()> {
    anyhow::bail!("该平台不支持修改文件属主")
}

pub fn stat(jail: &Jail, path: &str) -> Result<FileStat> {
    let target = jail.resolve(path)?;
    let name = target
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "/".into());
    stat_of(&target, name)
}

/// 以 `O_NOFOLLOW` 打开：叶子是符号链接时直接失败，不跟随。
#[cfg(unix)]
fn open_no_follow(path: &Path, create: bool) -> Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut options = std::fs::OpenOptions::new();
    options.read(true).write(create).create(create).mode(0o600);
    options.custom_flags(libc::O_NOFOLLOW);
    options
        .open(path)
        .with_context(|| format!("打开失败（符号链接会被拒绝）：{}", path.display()))
}

#[cfg(not(unix))]
fn open_no_follow(path: &Path, create: bool) -> Result<std::fs::File> {
    // 非 Unix 平台没有 O_NOFOLLOW，退化为显式检查，安全性由 ensure_not_symlink 兜底
    ensure_not_symlink(path)?;
    let mut options = std::fs::OpenOptions::new();
    options.read(true).write(create).create(create);
    options
        .open(path)
        .with_context(|| format!("打开失败：{}", path.display()))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    /// 造一个临时目录作为允许根，避免测试碰到真实系统路径。
    struct Sandbox {
        root: PathBuf,
        jail: Jail,
    }
    impl Sandbox {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!("opsd-jail-{}", opsd::protocol::id()));
            std::fs::create_dir_all(&root).unwrap();
            let jail = Jail::new(vec![root.clone()], Vec::new());
            Self { root, jail }
        }
    }
    impl Drop for Sandbox {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn 只接受绝对路径() {
        let s = Sandbox::new();
        assert!(s.jail.resolve("relative/path").is_err());
        assert!(s.jail.resolve("./x").is_err());
        assert!(s.jail.resolve("").is_err());
        assert!(s.jail.resolve(&format!("{}/x", s.root.display())).is_ok());
    }

    #[test]
    fn 词法展开上级目录后仍受根限制() {
        let s = Sandbox::new();
        // 展开后仍在允许根内
        let inside = format!("{}/a/../b", s.root.display());
        assert_eq!(s.jail.resolve(&inside).unwrap(), s.root.join("b"));
        // 展开后逃出允许根
        let outside = format!("{}/../../etc/passwd", s.root.display());
        assert!(
            s.jail.resolve(&outside).is_err(),
            "越过允许根的路径必须被拒绝"
        );
    }

    #[test]
    fn 硬禁前缀逐条生效() {
        let jail = Jail::new(vec![PathBuf::from("/")], Vec::new());
        for denied in ["/proc/self/environ", "/sys/kernel", "/dev/sda", "/run/docker.sock"] {
            assert!(jail.resolve(denied).is_err(), "{denied} 必须被拒绝");
        }
        // 组件边界比较：不能把 /procx 当成 /proc 的子路径
        assert!(
            jail.resolve("/procx/file").is_ok(),
            "前缀比较必须按组件边界"
        );
        assert!(jail.resolve("/development").is_ok());
        assert!(jail.resolve("/etc/hosts").is_ok());
    }

    #[test]
    fn agent_自身目录由运行时注入而非写死() {
        let s = Sandbox::new();
        let secrets = s.root.join("data");
        std::fs::create_dir_all(&secrets).unwrap();
        let jail = Jail::new(
            vec![s.root.clone()],
            vec![secrets.clone(), s.root.join("data/pki")],
        );
        assert!(
            jail.resolve(&secrets.join("client-key.pem").display().to_string())
                .is_err(),
            "Agent 数据目录必须被拒绝"
        );
        assert!(
            jail.resolve(&s.root.join("data/pki/ca-key.pem").display().to_string())
                .is_err(),
            "PKI 目录必须被拒绝"
        );
        // 同一目录树里的其他位置不受影响
        assert!(
            jail.resolve(&s.root.join("other/file").display().to_string())
                .is_ok()
        );
    }

    #[cfg(unix)]
    #[test]
    fn 符号链接叶子被拒绝且不会跟随() {
        let s = Sandbox::new();
        let secret = s.root.join("secret.txt");
        std::fs::write(&secret, b"top secret").unwrap();
        let link = s.root.join("link.txt");
        std::os::unix::fs::symlink(&secret, &link).unwrap();
        // 读取符号链接必须失败，而不是读到目标内容
        let error = read(&s.jail, &link.display().to_string(), 0, 1024).unwrap_err();
        assert!(
            error.to_string().contains("符号链接"),
            "错误信息应说明拒绝原因：{error}"
        );
        // 直接读取真实文件仍然可以
        assert_eq!(
            read(&s.jail, &secret.display().to_string(), 0, 1024).unwrap(),
            b"top secret"
        );
        // 删除与重命名同样拒绝软链叶子
        assert!(
            remove(&s.jail, &link.display().to_string(), false, false).is_err()
        );
        assert!(
            rename(&s.jail, &link.display().to_string(), &s.root.join("x").display().to_string())
                .is_err()
        );
    }

    #[cfg(unix)]
    #[test]
    fn 指向被禁目录的目录软链不能用来逃逸() {
        let s = Sandbox::new();
        std::fs::create_dir_all("/proc/self").ok();
        let escape = s.root.join("escape");
        // 指向 /proc 的目录软链
        if std::os::unix::fs::symlink("/proc", &escape).is_ok() {
            let target = escape.join("self/environ");
            let result = s.jail.resolve(&target.display().to_string());
            assert!(result.is_err(), "通过目录软链指向 /proc 必须被拒绝");
        }
    }

    #[test]
    fn 列目录区分文件_目录与符号链接() {
        let s = Sandbox::new();
        std::fs::write(s.root.join("a.txt"), b"x").unwrap();
        std::fs::create_dir(s.root.join("sub")).unwrap();
        let entries = list(&s.jail, &s.root.display().to_string()).unwrap();
        let kinds: Vec<(&str, &str)> = entries
            .iter()
            .map(|e| (e.name.as_str(), e.kind.as_str()))
            .collect();
        assert!(kinds.contains(&("sub", "dir")));
        assert!(kinds.contains(&("a.txt", "file")));
        // 目录排在最前
        assert_eq!(entries[0].kind, "dir");
    }

    #[test]
    fn 上传必须摘要与大小都相符才就位() {
        let s = Sandbox::new();
        let file = s.root.join("out.bin");
        let path = file.display().to_string();
        let staged = staging_path(&s.jail, &path).unwrap();
        // 尚未传输时任务必须失败，而不是建出空文件
        assert!(put_staged(&s.jail, &path, &opsd::protocol::digest(b""), 0).is_err());
        assert!(!file.exists(), "失败的上传不得留下目标文件");

        std::fs::write(&staged, b"hello world").unwrap();
        // 摘要不符：拒绝
        assert!(put_staged(&s.jail, &path, &opsd::protocol::digest(b"other"), 11).is_err());
        // 大小不符：拒绝
        assert!(put_staged(&s.jail, &path, &opsd::protocol::digest(b"hello world"), 5).is_err());
        assert!(!file.exists(), "校验不通过时不得覆盖目标");
        // 校验失败不会删掉暂存内容，因此同一个任务可以重试并成功
        assert!(staged.exists(), "失败后暂存内容应保留以便重试");
        assert_eq!(
            put_staged(&s.jail, &path, &opsd::protocol::digest(b"hello world"), 11).unwrap(),
            11
        );
        assert_eq!(std::fs::read(&file).unwrap(), b"hello world");
        assert!(!staged.exists(), "就位后不应留下暂存文件");
    }

    #[test]
    fn 上传失败不会破坏已有文件() {
        let s = Sandbox::new();
        let file = s.root.join("keep.txt");
        std::fs::write(&file, b"original").unwrap();
        let path = file.display().to_string();
        let staged = staging_path(&s.jail, &path).unwrap();
        std::fs::write(&staged, b"tampered").unwrap();
        // 摘要与实际内容不符
        assert!(put_staged(&s.jail, &path, &opsd::protocol::digest(b"expected"), 8).is_err());
        assert_eq!(
            std::fs::read(&file).unwrap(),
            b"original",
            "校验失败时原文件必须原样保留"
        );
        // 显式丢弃暂存内容
        discard_staged(&s.jail, &path).unwrap();
        assert!(!staged.exists());
    }

    #[test]
    fn 受保护路径不会生成暂存文件() {
        let jail = Jail::new(vec![PathBuf::from("/")], Vec::new());
        assert!(staging_path(&jail, "/proc/self/environ").is_err());
        assert!(staging_path(&jail, "/run/docker.sock").is_err());
        assert!(
            put_staged(&jail, "/proc/self/environ", "x", 0).is_err(),
            "受保护路径在就位阶段同样要被拒绝"
        );
    }

    #[test]
    fn 读取受单次上限约束() {
        let s = Sandbox::new();
        let file = s.root.join("big.bin");
        std::fs::write(&file, vec![b'x'; (MAX_READ_BYTES + 100) as usize]).unwrap();
        let data = read(&s.jail, &file.display().to_string(), 0, u64::MAX).unwrap();
        assert_eq!(data.len() as u64, MAX_READ_BYTES, "单次读取必须截断到上限");
    }

    #[test]
    fn 递归删除需要显式确认() {
        let s = Sandbox::new();
        let dir = s.root.join("tree");
        std::fs::create_dir_all(dir.join("nested")).unwrap();
        std::fs::write(dir.join("nested/file"), b"x").unwrap();
        let path = dir.display().to_string();
        // 没有确认时拒绝
        assert!(remove(&s.jail, &path, true, false).is_err());
        // 声明递归但目标其实是非空目录、未确认 —— 仍然拒绝
        assert!(remove(&s.jail, &path, false, true).is_err());
        // 明确确认后才删除
        assert!(remove(&s.jail, &path, true, true).unwrap() >= 2);
        assert!(!dir.exists());
    }

    #[test]
    fn 空目录无需递归确认即可删除() {
        let s = Sandbox::new();
        let dir = s.root.join("empty");
        std::fs::create_dir(&dir).unwrap();
        assert_eq!(remove(&s.jail, &dir.display().to_string(), false, false).unwrap(), 1);
        assert!(!dir.exists());
    }

    #[test]
    fn 拒绝删除根目录() {
        let jail = Jail::new(vec![PathBuf::from("/")], Vec::new());
        assert!(remove(&jail, "/", true, true).is_err());
    }

    #[test]
    fn 重命名两端都要过闸门() {
        let s = Sandbox::new();
        let source = s.root.join("from.txt");
        std::fs::write(&source, b"x").unwrap();
        // 目标越界
        assert!(
            rename(
                &s.jail,
                &source.display().to_string(),
                "/tmp/elsewhere.txt"
            )
            .is_err()
        );
        // 源越界
        assert!(
            rename(
                &s.jail,
                "/etc/hosts",
                &s.root.join("to.txt").display().to_string()
            )
            .is_err()
        );
        assert!(
            rename(
                &s.jail,
                &source.display().to_string(),
                &s.root.join("to.txt").display().to_string()
            )
            .is_ok()
        );
    }

    #[test]
    fn 创建目录不会覆盖既有内容() {
        let s = Sandbox::new();
        let dir = s.root.join("once");
        let path = dir.display().to_string();
        mkdir(&s.jail, &path).unwrap();
        assert!(mkdir(&s.jail, &path).is_err(), "重复创建应失败而不是静默通过");
    }

    #[test]
    fn stat_能区分类型并给出大小() {
        let s = Sandbox::new();
        let file = s.root.join("s.txt");
        std::fs::write(&file, b"12345").unwrap();
        let info = stat(&s.jail, &file.display().to_string()).unwrap();
        assert_eq!(info.kind, "file");
        assert_eq!(info.size, 5);
        assert_eq!(info.name, "s.txt");
    }
}
