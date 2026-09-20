//! git 源管理：clone / pull --ff-only 到缓存目录，ls-remote 枚举引用。
//!
//! 全部 shell 出 `git`（本机已装且凭证管理器已登录）；所有子进程带
//! CREATE_NO_WINDOW 防控制台黑窗。gh CLI 仅作为可选加速（Release 查询用，
//! 见 install::release），引用枚举不依赖 gh。

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

#[derive(Debug)]
pub enum GitError {
    GitMissing,
    RepoNameInvalid(String),
    RunFailed { what: &'static str, stderr: String },
    NotFound { what: &'static str },
}

impl std::fmt::Display for GitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GitError::GitMissing => write!(f, "未找到 git（请安装 Git for Windows）"),
            GitError::RepoNameInvalid(u) => write!(f, "无法从 URL 解析仓库名：{u}"),
            GitError::RunFailed { what, stderr } => {
                let brief = stderr.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("");
                write!(f, "git {what} 失败：{}", if brief.is_empty() { &stderr } else { brief })
            }
            GitError::NotFound { what } => write!(f, "未找到 {what}"),
        }
    }
}

impl std::error::Error for GitError {}

/// clone/pull 后的仓库状态。
#[derive(Debug, Clone, Serialize)]
pub struct RepoState {
    pub path: PathBuf,
    /// HEAD commit（短哈希 12 位，registry.commit 记录用）
    pub head: String,
    /// 默认分支名（如 "main"）
    pub default_ref: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RefKind {
    Branch,
    Tag,
}

#[derive(Debug, Clone, Serialize)]
pub struct RefInfo {
    pub name: String,
    pub kind: RefKind,
    pub commit: String,
}

/// 从 URL 解析仓库名：`.../TrueGlow.git` / `.../TrueGlow` → `TrueGlow`。
/// 只剩 host（无路径）→ None。
pub fn repo_name(url: &str) -> Option<String> {
    let trimmed = url.trim().trim_end_matches('/');
    // 剥掉 scheme，剩 host/path；scp 风格 git@host:owner/repo 按 ':' 剥 host
    let path = if let Some((_, rest)) = trimmed.split_once("://") {
        rest.split_once('/')?.1
    } else if let Some((_, after)) = trimmed.split_once(':') {
        after
    } else {
        trimmed
    };
    let last = path.rsplit('/').find(|s| !s.is_empty())?;
    let name = last.strip_suffix(".git").unwrap_or(last);
    if name.is_empty() || !name.chars().next().is_some_and(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    Some(name.to_string())
}

/// 缓存目录：`<data_dir>/cache/repos/<name>`。
pub fn repo_cache_path(data_dir: &Path, url: &str) -> Result<PathBuf, GitError> {
    let name = repo_name(url).ok_or_else(|| GitError::RepoNameInvalid(url.to_string()))?;
    Ok(data_dir.join("cache").join("repos").join(name))
}

/// 仓库名弱提示之外：完整 owner/repo（Release API 路径用）。
/// `https://github.com/18163623522/TrueGlow.git` → `18163623522/TrueGlow`。
pub fn repo_full_name(url: &str) -> Option<String> {
    let trimmed = url.trim().trim_end_matches('/');
    let path = if let Some((_, rest)) = trimmed.split_once("://") {
        let (_, p) = rest.split_once('/')?;
        p
    } else if let Some((_, after)) = trimmed.split_once(':') {
        after
    } else {
        trimmed
    };
    let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if parts.len() < 2 {
        return None;
    }
    let repo = parts[parts.len() - 1].strip_suffix(".git").unwrap_or(parts[parts.len() - 1]);
    if repo.is_empty() {
        return None;
    }
    Some(format!("{}/{}", parts[parts.len() - 2], repo))
}

fn git(args: &[&str], cwd: Option<&Path>) -> Result<Output, GitError> {
    let mut cmd = Command::new("git");
    cmd.args(args);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    cmd.output().map_err(|_| GitError::GitMissing)
}

fn ok_or(o: Output, what: &'static str) -> Result<String, GitError> {
    if o.status.success() {
        Ok(String::from_utf8_lossy(&o.stdout).into_owned())
    } else {
        Err(GitError::RunFailed {
            what,
            stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
        })
    }
}

/// 确保缓存仓库最新：已存在 → `pull --ff-only`；否则 clone。
/// `on_line` 收到命令输出行（前端事件流）。
pub fn ensure_repo(url: &str, cache_dir: &Path, on_line: &mut dyn FnMut(&str)) -> Result<RepoState, GitError> {
    let path = repo_cache_path(cache_dir, url)?;
    let exists = path.join(".git").is_dir();
    if exists {
        on_line(&format!("git pull --ff-only（{}）", path.display()));
        ok_or(git(&["pull", "--ff-only"], Some(&path))?, "pull")?;
    } else {
        std::fs::create_dir_all(path.parent().expect("cache/repos 一定有父目录")).ok();
        on_line(&format!("git clone {url}"));
        ok_or(git(&["clone", url, &path.to_string_lossy()], None)?, "clone")?;
    }
    repo_state(&path)
}

/// 切换工作区到指定分支/标签（构建前选 ref 用）。
pub fn checkout(path: &Path, git_ref: &str) -> Result<(), GitError> {
    ok_or(git(&["checkout", "-q", git_ref], Some(path))?, "checkout")?;
    Ok(())
}

/// 读仓库当前状态（HEAD + 默认分支）。
pub fn repo_state(path: &Path) -> Result<RepoState, GitError> {
    let head_full = ok_or(git(&["rev-parse", "HEAD"], Some(path))?, "rev-parse")?;
    let head_full = head_full.trim().to_string();
    // origin/HEAD 符号引用 → 默认分支；裸仓库无则回退 main
    let default_ref = match git(&["symbolic-ref", "--short", "refs/remotes/origin/HEAD"], Some(path)) {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .trim()
            .trim_start_matches("origin/")
            .to_string(),
        _ => "main".to_string(),
    };
    Ok(RepoState {
        path: path.to_path_buf(),
        head: head_full.chars().take(12).collect(),
        default_ref,
    })
}

/// 远端引用枚举（heads + tags）。`ls-remote` 走凭证管理器，私有仓可用。
pub fn list_refs(url: &str) -> Result<Vec<RefInfo>, GitError> {
    let out = ok_or(git(&["ls-remote", "--heads", "--tags", url], None)?, "ls-remote")?;
    Ok(parse_ls_remote(&out))
}

/// ls-remote 输出解析（纯函数，供单测）。
/// 行形如 `<sha>\trefs/heads/ue5.7`；tag 的 `<sha>^{}` 解包行被忽略。
pub fn parse_ls_remote(text: &str) -> Vec<RefInfo> {
    text.lines()
        .filter_map(|line| {
            let (commit, r#ref) = line.split_once('\t')?;
            if r#ref.ends_with("^{}") {
                return None; // annotated tag 解包行
            }
            let (kind, name) = if let Some(n) = r#ref.strip_prefix("refs/heads/") {
                (RefKind::Branch, n.to_string())
            } else if let Some(n) = r#ref.strip_prefix("refs/tags/") {
                (RefKind::Tag, n.to_string())
            } else {
                return None;
            };
            Some(RefInfo {
                name,
                kind,
                commit: commit.to_string(),
            })
        })
        .collect()
}

/// gh CLI 是否可用（Release 查询加速用；不可用回退匿名 REST）。
pub fn gh_available() -> bool {
    Command::new("gh")
        .args(["--version"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// 读某引用下的第一个 .uplugin 内容（compat 权威确认用，本地 git show，零网络）。
/// 返回 (uplugin 相对路径, 文件内容)。
pub fn read_uplugin_at(path: &Path, git_ref: &str) -> Result<(String, String), GitError> {
    let tree = ok_or(git(&["ls-tree", "-r", "--name-only", git_ref], Some(path))?, "ls-tree")?;
    let uplugin = tree
        .lines()
        .find(|l| l.to_ascii_lowercase().ends_with(".uplugin"))
        .ok_or(GitError::NotFound { what: "该分支下没有 .uplugin" })?
        .trim()
        .to_string();
    let content = ok_or(git(&["show", &format!("{git_ref}:{uplugin}")], Some(path))?, "show")?;
    Ok((uplugin, content))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_name_variants() {
        assert_eq!(repo_name("https://github.com/18163623522/TrueGlow").as_deref(), Some("TrueGlow"));
        assert_eq!(repo_name("https://github.com/18163623522/TrueGlow.git").as_deref(), Some("TrueGlow"));
        assert_eq!(repo_name("https://github.com/18163623522/TrueGlow/").as_deref(), Some("TrueGlow"));
        assert_eq!(repo_name("git@github.com:18163623522/houdini-graph-tools.git").as_deref(), Some("houdini-graph-tools"));
        assert!(repo_name("https://github.com/").is_none());
        assert!(repo_name("").is_none());
    }

    #[test]
    fn repo_full_name_variants() {
        assert_eq!(
            repo_full_name("https://github.com/18163623522/TrueGlow").as_deref(),
            Some("18163623522/TrueGlow")
        );
        assert_eq!(
            repo_full_name("https://github.com/18163623522/houdini-graph-tools.git").as_deref(),
            Some("18163623522/houdini-graph-tools")
        );
        assert_eq!(
            repo_full_name("git@github.com:18163623522/BetterHLSL.git").as_deref(),
            Some("18163623522/BetterHLSL")
        );
        assert!(repo_full_name("https://github.com/18163623522").is_none());
    }

    #[test]
    fn ls_remote_parsing() {
        let text = "\
abc1230000000000000000000000000000000000\trefs/heads/main
def4560000000000000000000000000000000000\trefs/heads/ue5.7
789abc0000000000000000000000000000000000\trefs/tags/v0.8.2
789abc0000000000000000000000000000000000\trefs/tags/v0.8.2^{}
HEADREF\trefs/remotes/origin/HEAD
";
        let refs = parse_ls_remote(text);
        assert_eq!(refs.len(), 3);
        assert_eq!(refs[0].name, "main");
        assert_eq!(refs[0].kind, RefKind::Branch);
        assert_eq!(refs[1].name, "ue5.7");
        assert_eq!(refs[2].name, "v0.8.2");
        assert_eq!(refs[2].kind, RefKind::Tag);
        assert!(refs[2].commit.starts_with("789abc"));
    }

    /// 真机网络依赖：clone 公开仓库 TrueGlow → 二次 ensure 走 pull 秒回。
    #[test]
    #[ignore = "真机网络 + git 依赖"]
    fn live_ensure_repo_roundtrip() {
        let dir = std::env::temp_dir().join(format!("dpm-git-live-{}", std::process::id()));
        let url = "https://github.com/18163623522/TrueGlow";
        let state = ensure_repo(url, &dir, &mut |_| {}).unwrap();
        assert!(state.path.join(".git").is_dir());
        assert!(!state.head.is_empty());
        let t0 = std::time::Instant::now();
        ensure_repo(url, &dir, &mut |_| {}).unwrap();
        assert!(t0.elapsed().as_secs() < 60, "二次 pull 应显著快于 clone");
        let refs = list_refs(url).unwrap();
        assert!(refs.iter().any(|r| r.name == "main"), "refs: {refs:?}");
        std::fs::remove_dir_all(&dir).ok();
    }
}
