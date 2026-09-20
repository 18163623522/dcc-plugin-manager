//! 插件注册表：`%APPDATA%\dcc-plugin-manager\registry.json` 读写与数据类型。
//!
//! 唯一持久层（缓存目录除外）。所有模块依赖此处签名（计划文档锁定的契约）。

use chrono::Local;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const REGISTRY_FILE: &str = "registry.json";

/// 注册表本体。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Registry {
    #[serde(default)]
    pub plugins: Vec<PluginEntry>,
}

/// 一个受管插件。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginEntry {
    /// uplugin Name 或目录名
    pub id: String,
    /// 序列化为 "type"（设计文档 §3.3 布局）
    #[serde(rename = "type")]
    pub kind: PluginKind,
    pub source: PluginSource,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    /// 本地源内容摘要基线（update 比对用）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_digest: Option<String>,
    #[serde(default)]
    pub installed: Vec<InstalledTarget>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PluginKind {
    /// 序列化为 "UE"（设计 §3.3）
    #[serde(rename = "UE")]
    Ue,
    /// 变体名即 "Houdini"
    Houdini,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum PluginSource {
    /// `{"kind":"git","url":...,"ref":...}`
    Git {
        url: String,
        /// 跟踪分支/标签；缺省 = 仓库默认分支
        #[serde(rename = "ref", default, skip_serializing_if = "Option::is_none")]
        default_ref: Option<String>,
    },
    /// `{"kind":"local","path":...}`（本地源目录永不被删除）
    Local { path: PathBuf },
}

/// 一个引擎上的安装记录。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstalledTarget {
    /// "UE-5.7.3" / "Houdini-22.0"（引擎标签 = 卸载与徽标匹配键）
    pub engine: String,
    /// 安装落位路径（卸载只删这里记录的精确路径）
    pub path: PathBuf,
    #[serde(default)]
    pub method: InstallMethod,
    /// RFC3339 本地时区
    #[serde(rename = "installedAt")]
    pub installed_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InstallMethod {
    #[default]
    #[serde(rename = "binary-copy")]
    BinaryCopy,
    #[serde(rename = "release")]
    Release,
    #[serde(rename = "build")]
    Build,
}

impl Registry {
    /// 按 id 新增或整体替换。
    pub fn upsert(&mut self, entry: PluginEntry) {
        match self.plugins.iter().position(|p| p.id == entry.id) {
            Some(i) => self.plugins[i] = entry,
            None => self.plugins.push(entry),
        }
    }

    /// 删除指定安装目标（精确路径匹配，大小写不敏感）。
    pub fn remove_target(&mut self, path: &Path) -> bool {
        let key = path.to_string_lossy().to_lowercase();
        let mut removed = false;
        for p in &mut self.plugins {
            let before = p.installed.len();
            p.installed.retain(|t| t.path.to_string_lossy().to_lowercase() != key);
            removed |= p.installed.len() != before;
        }
        removed
    }

    pub fn get(&self, id: &str) -> Option<&PluginEntry> {
        self.plugins.iter().find(|p| p.id == id)
    }
}

/// 读取注册表；文件缺失 → 空表；JSON 损坏 → 备份为
/// `registry.json.bad.<ts>` 后重建空表（设计 §5）。
pub fn load(app_dir: &Path) -> Registry {
    let file = app_dir.join(REGISTRY_FILE);
    let Ok(text) = fs::read_to_string(&file) else {
        return Registry::default();
    };
    match serde_json::from_str(&text) {
        Ok(r) => r,
        Err(_) => {
            let backup = app_dir.join(format!(
                "{REGISTRY_FILE}.bad.{}",
                Local::now().format("%Y%m%d%H%M%S")
            ));
            let _ = fs::copy(&file, &backup);
            Registry::default()
        }
    }
}

/// 写回注册表（临时文件 + 原子改名，避免写一半损坏）。
pub fn save(app_dir: &Path, registry: &Registry) -> std::io::Result<()> {
    fs::create_dir_all(app_dir)?;
    let file = app_dir.join(REGISTRY_FILE);
    let tmp = app_dir.join(format!("{REGISTRY_FILE}.tmp"));
    let text = serde_json::to_string_pretty(registry)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    fs::write(&tmp, text)?;
    // Windows 上 rename 目标存在时失败，先删旧文件（tmp 写成功后才动旧表）
    if file.exists() {
        fs::remove_file(&file)?;
    }
    fs::rename(&tmp, &file)
}

/// RFC3339 本地时区时间戳。
pub fn now_rfc3339() -> String {
    Local::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, false)
}

/// 引擎标签（registry 里的 engine 字段取值）。
pub fn ue_engine_label(version: &str) -> String {
    format!("UE-{version}")
}

pub fn houdini_engine_label(version: &str) -> String {
    format!("Houdini-{version}")
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn sample() -> Registry {
        Registry {
            plugins: vec![PluginEntry {
                id: "TrueGlow".into(),
                kind: PluginKind::Ue,
                source: PluginSource::Git {
                    url: "https://github.com/18163623522/TrueGlow".into(),
                    default_ref: Some("main".into()),
                },
                version: "0.8.2".into(),
                commit: Some("8ada112".into()),
                local_digest: None,
                installed: vec![InstalledTarget {
                    engine: "UE-5.8.1".into(),
                    path: PathBuf::from(r"E:\UE\UE_5.8\Engine\Plugins\Marketplace\TrueGlow"),
                    method: InstallMethod::BinaryCopy,
                    installed_at: "2026-09-20T11:00:00+08:00".into(),
                }],
            }],
        }
    }

    #[test]
    fn upsert_replaces_by_id() {
        let mut r = sample();
        let mut entry = r.plugins[0].clone();
        entry.version = "0.9.0".into();
        r.upsert(entry);
        assert_eq!(r.plugins.len(), 1);
        assert_eq!(r.plugins[0].version, "0.9.0");

        r.upsert(PluginEntry {
            id: "SimInspector".into(),
            kind: PluginKind::Ue,
            source: PluginSource::Local {
                path: PathBuf::from(r"D:\AI\SimInspector"),
            },
            version: "0.3.0".into(),
            commit: None,
            local_digest: None,
            installed: vec![],
        });
        assert_eq!(r.plugins.len(), 2);
    }

    #[test]
    fn remove_target_exact_case_insensitive() {
        let mut r = sample();
        assert!(r.remove_target(Path::new(r"e:\ue\ue_5.8\engine\plugins\marketplace\TrueGlow")));
        assert!(r.plugins[0].installed.is_empty());
        // 不存在的路径
        assert!(!r.remove_target(Path::new(r"E:\nope")));
        // 条目本身保留（源仍在管理中）
        assert_eq!(r.plugins.len(), 1);
    }

    #[test]
    fn json_shape_matches_spec() {
        let text = serde_json::to_string(&sample()).unwrap();
        assert!(text.contains(r#""type":"Ue""#) || text.contains(r#""type":"UE""#));
        assert!(text.contains(r#""kind":"git""#));
        assert!(text.contains(r#""installedAt""#));
        assert!(text.contains(r#""method":"binary-copy""#));
        // 字段缺省兼容：老版本 JSON 无 commit/method/installed
        let legacy = r#"{"plugins":[{"id":"X","type":"UE","source":{"kind":"local","path":"D:\\X"},"version":"1.0"}]}"#;
        let r: Registry = serde_json::from_str(legacy).unwrap();
        assert_eq!(r.plugins[0].installed.len(), 0);
        assert_eq!(r.plugins[0].commit, None);
    }
}
