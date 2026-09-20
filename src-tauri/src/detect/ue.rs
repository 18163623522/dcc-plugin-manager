//! UE 引擎检测：解析 Epic Launcher 安装清单（LauncherInstalled.dat），
//! 叠加设置页追加的自定义根目录，统一校验 RunUAT.bat 存在性。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// 检测到的一个 UE 引擎安装。
#[derive(Debug, Clone, Serialize)]
pub struct UeEngine {
    /// 形如 "5.7.4"
    pub version: String,
    /// 引擎根目录（Launcher 的 InstallLocation）
    pub root: PathBuf,
    /// `<root>\Engine\Build\BatchFiles\RunUAT.bat`（M3 构建流水线入口）
    pub runuat: PathBuf,
}

/// LauncherInstalled.dat 固定路径。
pub const LAUNCHER_DAT: &str = r"C:\ProgramData\Epic\UnrealEngineLauncher\LauncherInstalled.dat";

#[derive(Debug, Deserialize)]
pub struct LauncherDat {
    #[serde(rename = "InstallationList")]
    pub installation_list: Vec<InstallationEntry>,
}

#[derive(Debug, Deserialize)]
pub struct InstallationEntry {
    #[serde(rename = "InstallLocation")]
    pub install_location: String,
    #[serde(rename = "AppVersion", default)]
    pub app_version: String,
}

/// 检测本机全部 UE 引擎：Launcher 清单 + 自定义根目录（去重、按版本降序）。
///
/// 两种来源都要求 `Engine\Build\BatchFiles\RunUAT.bat` 存在，
/// 残缺安装（缺构建脚本）被过滤，由 preflight（M3）负责解释原因。
pub fn detect_ue_engines(extra_roots: &[PathBuf]) -> Vec<UeEngine> {
    let mut engines: Vec<UeEngine> = Vec::new();

    if let Ok(text) = fs::read_to_string(LAUNCHER_DAT) {
        if let Some(list) = engines_from_dat(&text) {
            engines.extend(list);
        }
    }

    for root in extra_roots {
        if let Some(e) = engine_from_root(root) {
            if !engines.iter().any(|x| same_path(&x.root, &e.root)) {
                engines.push(e);
            }
        }
    }

    engines.retain(|e| e.runuat.is_file());
    engines.sort_by_key(|e| std::cmp::Reverse(version_key(&e.version)));
    engines
}

/// 纯解析：dat 文本 → 去重后的引擎列表（不触碰磁盘，供单测）。
///
/// 同一 InstallLocation 的多个条目（引擎本体 / Marketplace 插件 / Fab）
/// 取版本最高者；Windows 路径按不区分大小写去重，保留首次出现的大小写。
pub fn engines_from_dat(text: &str) -> Option<Vec<UeEngine>> {
    let text = text.trim_start_matches('\u{feff}');
    let dat: LauncherDat = serde_json::from_str(text).ok()?;

    // key = 小写 InstallLocation → (版本键, 版本串, 原始路径)
    let mut best: HashMap<String, (Vec<u64>, String, String)> = HashMap::new();
    for entry in &dat.installation_list {
        if entry.install_location.is_empty() || entry.app_version.is_empty() {
            continue;
        }
        let ver_str = app_version_triplet(&entry.app_version);
        let ver_key = version_key(&ver_str);
        match best.get(&entry.install_location.to_lowercase()) {
            Some((old_key, _, _)) if *old_key >= ver_key => {}
            _ => {
                best.insert(
                    entry.install_location.to_lowercase(),
                    (ver_key, ver_str, entry.install_location.clone()),
                );
            }
        }
    }

    Some(
        best.into_values()
            .map(|(_, ver, loc)| {
                let root = PathBuf::from(loc);
                let runuat = runuat_of(&root);
                UeEngine { version: ver, root, runuat }
            })
            .collect(),
    )
}

/// 自定义根目录 → 引擎（版本优先取 Build.version，回退目录名推断）。
fn engine_from_root(root: &Path) -> Option<UeEngine> {
    let runuat = runuat_of(root);
    if !root.is_dir() || !runuat.is_file() {
        return None;
    }
    let version = version_from_build_version(&root.join("Engine").join("Build").join("Build.version"))
        .or_else(|| version_from_dir_name(root))?;
    Some(UeEngine { version, root: root.to_path_buf(), runuat })
}

fn runuat_of(root: &Path) -> PathBuf {
    root.join("Engine").join("Build").join("BatchFiles").join("RunUAT.bat")
}

/// "5.7.4-51494982+++UE5+Release-5.7-Windows" → "5.7.4"；
/// 段数不足补零（"5.7-x" → "5.7.0"）；解析失败回退 "0.0.0"。
pub(crate) fn app_version_triplet(app_version: &str) -> String {
    let prefix = app_version.split('-').next().unwrap_or(app_version);
    let mut parts: Vec<String> = prefix
        .split('.')
        .map(|p| p.trim().parse::<u64>().unwrap_or(0).to_string())
        .take(3)
        .collect();
    while parts.len() < 3 {
        parts.push("0".to_string());
    }
    parts.join(".")
}

/// 版本排序键（"5.10" > "5.8" 按数值比较，避免字典序坑）。
pub(crate) fn version_key(version: &str) -> Vec<u64> {
    version
        .split('.')
        .map(|p| p.trim().parse::<u64>().unwrap_or(0))
        .collect()
}

fn version_from_build_version(path: &Path) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
    let major = tag_number(&text, "MajorVersion")?;
    let minor = tag_number(&text, "MinorVersion")?;
    let patch = tag_number(&text, "PatchVersion").unwrap_or(0);
    Some(format!("{major}.{minor}.{patch}"))
}

fn tag_number(text: &str, tag: &str) -> Option<u64> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let i = text.find(&open)? + open.len();
    let j = text[i..].find(&close)? + i;
    text[i..j].trim().parse().ok()
}

/// 目录名推断版本："UE_5.7" / "UE4.26" / "5.8" → "x.y.0"。
fn version_from_dir_name(root: &Path) -> Option<String> {
    let name = root.file_name()?.to_str()?;
    let chars: Vec<char> = name.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_ascii_digit() {
            // 收集数字串
            let start = i;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            let major: String = chars[start..i].iter().collect();
            if i < chars.len() && chars[i] == '.' && i + 1 < chars.len() && chars[i + 1].is_ascii_digit() {
                let minor_start = i + 1;
                let mut j = minor_start;
                while j < chars.len() && chars[j].is_ascii_digit() {
                    j += 1;
                }
                let minor: String = chars[minor_start..j].iter().collect();
                return Some(format!("{major}.{minor}.0"));
            }
        } else {
            i += 1;
        }
    }
    None
}

/// Windows 路径等价判断（大小写不敏感；不做分量规范化）。
fn same_path(a: &Path, b: &Path) -> bool {
    a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triplet_extracts_and_pads() {
        assert_eq!(app_version_triplet("5.7.4-51494982+++UE5+Release-5.7-Windows"), "5.7.4");
        assert_eq!(app_version_triplet("5.7-x"), "5.7.0");
        assert_eq!(app_version_triplet("4.26.2-15973114+++UE4+Release-4.26-2023.1-Windows"), "4.26.2");
    }

    #[test]
    fn version_key_numeric_order() {
        assert!(version_key("5.10.0") > version_key("5.8.1"));
        assert!(version_key("5.8.1") > version_key("4.26.2"));
    }

    #[test]
    fn dir_name_version() {
        assert_eq!(version_from_dir_name(Path::new(r"E:\UE\UE_5.7")).unwrap(), "5.7.0");
        assert_eq!(version_from_dir_name(Path::new(r"D:\UE4.26")).unwrap(), "4.26.0");
        assert_eq!(version_from_dir_name(Path::new(r"E:\engines\5.8")).unwrap(), "5.8.0");
        assert!(version_from_dir_name(Path::new(r"E:\engines\source-build")).is_none());
    }

    #[test]
    fn malformed_json_is_none() {
        assert!(engines_from_dat("not json").is_none());
        assert!(engines_from_dat("{\"InstallationList\": []}").unwrap().is_empty());
    }

    #[test]
    fn dedupes_case_insensitive_max_version() {
        let json = r#"{
            "InstallationList": [
                {"InstallLocation": "E:\\UE\\UE_5.7", "AppVersion": "5.7.0-a"},
                {"InstallLocation": "e:\\ue\\ue_5.7", "AppVersion": "5.7.4-b"},
                {"InstallLocation": "E:\\UE\\UE_5.8", "AppVersion": "5.8.1-c"}
            ]
        }"#;
        let engines = engines_from_dat(json).unwrap();
        assert_eq!(engines.len(), 2);
        assert!(engines.iter().any(|e| e.version == "5.7.4"));
    }
}
