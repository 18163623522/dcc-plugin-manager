//! 卸载器：按引擎粒度删除 registry 记录过的精确路径。
//!
//! 铁律（设计 §3.6）：
//! - 只删 `InstalledTarget.path` 记录过的精确路径——本地源目录永不被删；
//! - 删除前列出路径清单（前端确认框展示）；
//! - 文件锁 → 中止该项并报告，不产生半删状态；
//! - registry 里记录不存在（missing）→ 报告而非崩溃。

use crate::install::copy::find_locked_file;
use crate::registry::{Registry, InstalledTarget};
use serde::Serialize;
use std::path::PathBuf;

/// 单次卸载的报告（前端逐项展示）。
#[derive(Debug, Default, Clone, Serialize)]
pub struct UninstallReport {
    /// 成功删除的 { 引擎, 路径 }
    pub removed: Vec<RemovedTarget>,
    /// registry 记录了但磁盘上不存在（视为已卸载，registry 照样清理）
    pub missing: Vec<RemovedTarget>,
    /// 被文件锁中止（保持安装原状，registry 保留记录）
    pub locked: Vec<RemovedTarget>,
    /// 其他删除错误
    pub failed: Vec<FailedTarget>,
    /// 本条目是否已无任何安装（可选联动删除缓存，M2 git 缓存接入后生效）
    pub entry_exhausted: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RemovedTarget {
    pub engine: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
pub struct FailedTarget {
    pub engine: String,
    pub path: PathBuf,
    pub reason: String,
}

/// 列出将被删除的路径（确认框用）。`engines` 为空 = 该条目全部安装。
pub fn plan_targets(registry: &Registry, entry_id: &str, engines: &[String]) -> Vec<RemovedTarget> {
    match registry.get(entry_id) {
        Some(entry) => entry
            .installed
            .iter()
            .filter(|t| engines.is_empty() || engines.contains(&t.engine))
            .map(|t| RemovedTarget { engine: t.engine.clone(), path: t.path.clone() })
            .collect(),
        None => Vec::new(),
    }
}

/// 执行卸载并同步清理 registry（调用方负责 save 落盘）。
/// `keep_cache`：git 缓存仓库保留开关（M1 无 git 缓存，仅透传语义）。
pub fn uninstall(
    registry: &mut Registry,
    entry_id: &str,
    engines: &[String],
    _keep_cache: bool,
) -> UninstallReport {
    let mut report = UninstallReport::default();

    let Some(entry) = registry.plugins.iter_mut().find(|p| p.id == entry_id) else {
        return report;
    };

    let mut keep: Vec<InstalledTarget> = Vec::new();
    for t in std::mem::take(&mut entry.installed) {
        let matched = engines.is_empty() || engines.contains(&t.engine);
        if !matched {
            keep.push(t);
            continue;
        }

        let target = RemovedTarget { engine: t.engine.clone(), path: t.path.clone() };
        if !t.path.exists() {
            // 目录已不在（手工删除等）也要清掉 sidecar（Houdini packages json），不留孤儿
            if let Some(sidecar) = &t.sidecar {
                if sidecar.is_file() {
                    let _ = std::fs::remove_file(sidecar);
                }
            }
            report.missing.push(target);
            continue; // registry 记录清理（不回填 keep）
        }
        if let Some(locked) = find_locked_file(&t.path) {
            let _ = locked; // 报告粒度：整个目标被锁
            report.locked.push(target);
            keep.push(t);
            continue;
        }
        match std::fs::remove_dir_all(&t.path) {
            Ok(()) => {
                // 附带产物（Houdini packages json）连带删除
                if let Some(sidecar) = &t.sidecar {
                    if sidecar.is_file() {
                        let _ = std::fs::remove_file(sidecar);
                    }
                }
                report.removed.push(target);
            }
            Err(e) => {
                report.failed.push(FailedTarget {
                    engine: t.engine.clone(),
                    path: t.path.clone(),
                    reason: e.to_string(),
                });
                keep.push(t);
            }
        }
    }
    entry.installed = keep;
    report.entry_exhausted = entry.installed.is_empty();
    report
}
