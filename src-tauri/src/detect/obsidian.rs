//! Obsidian 检测（M5）：`%APPDATA%\obsidian\obsidian.json` 枚举 vault，
//! 应用版本取运行中进程的文件版本（Obsidian 不落注册表版本键）。

use serde::Serialize;
use std::path::PathBuf;

/// 一个 vault = 一个安装目标。
#[derive(Debug, Clone, Serialize)]
pub struct ObsidianVault {
    /// obsidian.json 里的 vault id（稳定标识，安装目标匹配键）
    pub id: String,
    /// vault 目录名（显示用）
    pub name: String,
    pub path: PathBuf,
    /// 是否当前打开的 vault
    pub open: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObsidianInfo {
    pub vaults: Vec<ObsidianVault>,
    pub app_version: Option<String>,
}

pub fn detect_obsidian() -> ObsidianInfo {
    let obsidian_json = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_default()
        .join("obsidian")
        .join("obsidian.json");
    let text = std::fs::read_to_string(&obsidian_json).unwrap_or_default();
    let mut vaults = parse_obsidian_json(&text);
    vaults.retain(|v| v.path.is_dir());

    ObsidianInfo {
        vaults,
        app_version: app_version(),
    }
}

/// 纯解析（供单测）：`{"vaults":{"id":{"path":"D:\\...","ts":1,"open":true}}}`。
pub fn parse_obsidian_json(text: &str) -> Vec<ObsidianVault> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(text.trim_start_matches('\u{feff}')) else {
        return Vec::new();
    };
    let Some(map) = v.get("vaults").and_then(|m| m.as_object()) else {
        return Vec::new();
    };
    map.iter()
        .filter_map(|(id, entry)| {
            let path = entry.get("path")?.as_str()?;
            let name = PathBuf::from(path)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| id.clone());
            Some(ObsidianVault {
                id: id.clone(),
                name,
                path: PathBuf::from(path),
                open: entry.get("open").and_then(|o| o.as_bool()).unwrap_or(false),
            })
        })
        .collect()
}

/// 运行中进程的文件版本（无版本注册表键；未运行 → None，兼容检查降级为提示）。
pub fn app_version() -> Option<String> {
    let out = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "(Get-Process Obsidian -ErrorAction SilentlyContinue | Select-Object -First 1).Path | ForEach-Object { if ($_ -and (Test-Path $_)) { (Get-Item $_).VersionInfo.ProductVersion } }",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let ver = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if ver.is_empty() {
        None
    } else {
        Some(ver)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 本机快照（2026-09-20）：3 个 vault，一个 open。
    const SNAPSHOT: &str = r#"{"vaults":{"f0d055e5bd164924":{"path":"D:\\001_Archive\\文档\\Note\\Obsidian\\笔记整理","ts":1787058146225,"open":true},"edafd10d8dd8bf11":{"path":"D:\\001_Archive\\文档\\Note\\Note","ts":1785987363455},"e54cee3c54781180":{"path":"D:\\001_Archive\\文档\\工作日志","ts":1788429429549}},"cli":true,"frame":"native"}"#;

    #[test]
    fn parses_real_snapshot() {
        let vaults = parse_obsidian_json(SNAPSHOT);
        assert_eq!(vaults.len(), 3);
        let open = vaults.iter().find(|v| v.open).unwrap();
        assert_eq!(open.name, "笔记整理");
        assert_eq!(open.id, "f0d055e5bd164924");
        assert!(open.path.ends_with("笔记整理"));
        assert!(vaults.iter().filter(|v| !v.open).count() == 2);
    }

    #[test]
    fn tolerates_garbage() {
        assert!(parse_obsidian_json("").is_empty());
        assert!(parse_obsidian_json("{}").is_empty());
        assert!(parse_obsidian_json(r#"{"vaults":{}}"#).is_empty());
        assert!(parse_obsidian_json(r#"{"other":1}"#).is_empty());
    }
}
