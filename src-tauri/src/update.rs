//! 更新检查（设计 §3.5）。
//!
//! - git 源：Release tag 优先比对（版本一致即最新，commit 推进不打扰）；
//!   无 Release → ensure_repo 拉远端后比 HEAD commit。
//! - 本地源：目录内容摘要（rel_path + size + mtime）对比登记时的基线，
//!   变化即"可更新"（重装语义）。

use crate::install::release::latest_release;
use crate::registry::{PluginEntry, PluginSource};
use crate::sources::git::{ensure_repo, repo_full_name};
use serde::Serialize;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum UpdateState {
    Latest,
    /// { kind, newVersion }（新 tag 或新 commit 短哈希 / "目录内容已变化"）
    Available { new_version: String },
    Failed { reason: String },
}

/// 检查一个条目（按 source 类型分发）。
pub fn check(entry: &PluginEntry, data_dir: &Path) -> UpdateState {
    match &entry.source {
        PluginSource::Git { url, .. } => check_git(entry, url, data_dir),
        PluginSource::Local { path } => check_local(entry, path),
    }
}

fn check_git(entry: &PluginEntry, url: &str, data_dir: &Path) -> UpdateState {
    // 1. Release tag 通道（有 Release 的仓库以版本号为准）
    if let Some(full) = repo_full_name(url) {
        if let Ok(rel) = latest_release(&full) {
            let tag_ver = rel.tag.trim_start_matches(['v', 'V']);
            return if tag_ver == entry.version {
                UpdateState::Latest
            } else {
                UpdateState::Available { new_version: rel.tag }
            };
        }
        // NoRelease → 走 commit 通道；其他查询错误也退回 commit 通道
    }

    // 2. commit 通道：pull 后比 HEAD
    match ensure_repo(url, data_dir, &mut |_| {}) {
        Ok(state) => {
            if entry.commit.as_deref() == Some(state.head.as_str()) {
                UpdateState::Latest
            } else {
                UpdateState::Available { new_version: state.head }
            }
        }
        Err(e) => UpdateState::Failed { reason: e.to_string() },
    }
}

fn check_local(entry: &PluginEntry, dir: &Path) -> UpdateState {
    let Some(stored) = entry.local_digest.as_deref() else {
        return UpdateState::Latest; // 无基线（老数据）不打扰
    };
    let current = local_digest(dir).to_string();
    if current == stored {
        UpdateState::Latest
    } else {
        UpdateState::Available { new_version: "目录内容已变化".into() }
    }
}

/// 目录内容摘要：递归收集 (相对路径小写, 大小, mtime 纳秒) 排序后哈希。
/// mtime 参与 → 内容回滚也会被标"可更新"（重装语义，无害）。
pub fn local_digest(dir: &Path) -> u64 {
    let mut items: Vec<(String, u64, u128)> = Vec::new();
    collect(dir, dir, &mut items);
    items.sort();
    let mut hasher = DefaultHasher::new();
    for item in &items {
        item.hash(&mut hasher);
    }
    hasher.finish()
}

fn collect(root: &Path, dir: &Path, out: &mut Vec<(String, u64, u128)>) {
    let Ok(it) = std::fs::read_dir(dir) else { return };
    for e in it.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect(root, &p, out);
        } else {
            let Ok(meta) = e.metadata() else { continue };
            let rel = p
                .strip_prefix(root)
                .unwrap_or(&p)
                .to_string_lossy()
                .to_lowercase();
            let mtime = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            out.push((rel, meta.len(), mtime));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_tree(tag: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!(
            "dpm-upd-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
        ));
        for (rel, content) in files {
            let p = d.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, content).unwrap();
        }
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn digest_stable_and_sensitive() {
        let a = temp_tree("a", &[("x/Binaries/dll", "bin"), ("x/x.uplugin", "{}")]);
        // 同一棵树重复计算 → 稳定
        assert_eq!(local_digest(&a), local_digest(&a));

        // 内容/大小变化 → 敏感（mtime 语义：重写必变）
        let changed = temp_tree("c", &[("x/Binaries/dll", "bin2"), ("x/x.uplugin", "{}")]);
        assert_ne!(local_digest(&a), local_digest(&changed));

        // 文件增减 → 敏感
        let extra = temp_tree("d", &[("x/Binaries/dll", "bin"), ("x/x.uplugin", "{}"), ("x/Content/a", "1")]);
        assert_ne!(local_digest(&a), local_digest(&extra));
    }

    #[test]
    fn local_check_states() {
        let dir = temp_tree("e", &[("x.uplugin", "{}")]);
        let mut entry = PluginEntry {
            id: "X".into(),
            kind: crate::registry::PluginKind::Ue,
            source: PluginSource::Local { path: dir.clone() },
            version: "1.0".into(),
            commit: None,
            local_digest: None,
            installed: vec![],
        };
        // 无基线 → Latest
        assert_eq!(check(&entry, Path::new("Z:\\none")), UpdateState::Latest);
        // 有基线且一致 → Latest
        entry.local_digest = Some(local_digest(&dir).to_string());
        assert_eq!(check(&entry, Path::new("Z:\\none")), UpdateState::Latest);
        // 基线过期 → Available
        entry.local_digest = Some("0".into());
        assert!(matches!(check(&entry, Path::new("Z:\\none")), UpdateState::Available { .. }));
    }

    /// 真机网络：TrueGlow 无 Release → commit 通道（登记假 commit → Available）。
    #[test]
    #[ignore = "真机网络 + git 依赖"]
    fn live_git_commit_channel() {
        let entry = PluginEntry {
            id: "TrueGlow".into(),
            kind: crate::registry::PluginKind::Ue,
            source: PluginSource::Git {
                url: "https://github.com/18163623522/TrueGlow".into(),
                default_ref: Some("main".into()),
            },
            version: "0.0.0".into(),
            commit: Some("deadbeef".into()),
            local_digest: None,
            installed: vec![],
        };
        let cache = std::env::temp_dir().join(format!("dpm-upd-live-{}", std::process::id()));
        match check(&entry, &cache) {
            UpdateState::Available { new_version } => {
                assert_ne!(new_version, "deadbeef");
                assert!(!new_version.is_empty());
            }
            other => panic!("期望 Available，得到 {other:?}"),
        }
        std::fs::remove_dir_all(&cache).ok();
    }
}
