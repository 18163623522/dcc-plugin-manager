//! 本地目录源识别：目录内递归（深度 ≤2）找 `*.uplugin` → UE 插件；
//! 无 uplugin 但含 Houdini 约定文件（otls/hda/otl/hdalc 或 hda/*.dll）→ Houdini 包；
//! 都不满足 → 未识别。

use crate::registry::PluginKind;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

/// 识别结果。
#[derive(Debug, Clone, Serialize)]
pub struct LocalPlugin {
    pub kind: PluginKind,
    /// UE：uplugin 文件名去后缀；Houdini：目录名
    pub id: String,
    /// UE：FriendlyName（缺省回退 id）；Houdini：目录名
    pub friendly_name: String,
    /// UE：VersionName；Houdini：暂空（包内 package.json 的 M4 再读）
    pub version: String,
    /// UE：EngineVersion 字段（可选，compat 用）
    pub engine_version: Option<String>,
    /// uplugin 的 Description 字段（可选；git 源会被仓库描述覆盖）
    pub desc: Option<String>,
    /// uplugin 所在目录（UE 安装拷贝源）或包根目录（Houdini）
    pub plugin_root: PathBuf,
}

#[derive(Debug)]
pub enum LocalInspectError {
    NotADirectory(PathBuf),
    /// 找到 uplugin 但 JSON 损坏
    BadUplugin { path: PathBuf, reason: String },
    Unrecognized(PathBuf),
}

impl std::fmt::Display for LocalInspectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LocalInspectError::NotADirectory(p) => write!(f, "路径不是目录：{}", p.display()),
            LocalInspectError::BadUplugin { path, reason } => {
                write!(f, "uplugin 解析失败（{}）：{reason}", path.display())
            }
            LocalInspectError::Unrecognized(p) => write!(
                f,
                "未识别插件结构：{}（深度 ≤2 内无 .uplugin，也无 otls/hda/otl/hdalc 或 hda/*.dll）",
                p.display()
            ),
        }
    }
}

impl std::error::Error for LocalInspectError {}

/// Houdini 约定文件扩展名。
const HOUDINI_EXTS: [&str; 4] = ["otls", "hda", "otl", "hdalc"];

/// 识别本地插件目录。
pub fn inspect_local(path: &Path) -> Result<LocalPlugin, LocalInspectError> {
    if !path.is_dir() {
        return Err(LocalInspectError::NotADirectory(path.to_path_buf()));
    }

    // UE：深度 ≤2 找 .uplugin（本层 → 一层子目录，常见仓库布局 Repo/<Plugin>/<Plugin>.uplugin）
    if let Some(uplugin) = find_uplugin(path) {
        let id = uplugin.file_stem().unwrap_or_default().to_string_lossy().into_owned();
        let (friendly_name, version, engine_version, desc) = parse_uplugin(&uplugin)?;
        return Ok(LocalPlugin {
            kind: PluginKind::Ue,
            friendly_name: if friendly_name.is_empty() { id.clone() } else { friendly_name },
            id,
            version,
            engine_version,
            desc,
            plugin_root: uplugin.parent().unwrap_or(path).to_path_buf(),
        });
    }

    // Houdini：约定文件存在性
    if has_houdini_markers(path) {
        let id = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
        return Ok(LocalPlugin {
            kind: PluginKind::Houdini,
            friendly_name: id.clone(),
            id,
            version: String::new(),
            engine_version: None,
            desc: None,
            plugin_root: path.to_path_buf(),
        });
    }

    Err(LocalInspectError::Unrecognized(path.to_path_buf()))
}

/// 找 .uplugin：本层文件优先，再逐层下探子目录（最深第三层——
/// 覆盖 UE 工程式仓库布局 `Repo/Plugins/<Name>/<Name>.uplugin`）。
fn find_uplugin(dir: &Path) -> Option<PathBuf> {
    find_uplugin_at(dir, 0)
}

fn find_uplugin_at(dir: &Path, depth: u32) -> Option<PathBuf> {
    if depth > 2 {
        return None;
    }
    let entries = sorted_entries(dir)?;
    for e in &entries {
        if e.is_file() && e.extension().is_some_and(|x| x.eq_ignore_ascii_case("uplugin")) {
            return Some(e.clone());
        }
    }
    for e in &entries {
        if e.is_dir() {
            if let Some(found) = find_uplugin_at(e, depth + 1) {
                return Some(found);
            }
        }
    }
    None
}

fn sorted_entries(dir: &Path) -> Option<Vec<PathBuf>> {
    let mut v: Vec<PathBuf> = fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).collect();
    v.sort();
    Some(v)
}

/// 读 uplugin 的 FriendlyName / VersionName / EngineVersion / Description（容错 BOM 与宽松 JSON）。
fn parse_uplugin(
    uplugin: &Path,
) -> Result<(String, String, Option<String>, Option<String>), LocalInspectError> {
    let text = fs::read_to_string(uplugin).map_err(|e| LocalInspectError::BadUplugin {
        path: uplugin.to_path_buf(),
        reason: e.to_string(),
    })?;
    let text = text.trim_start_matches('\u{feff}');
    let v: serde_json::Value = serde_json::from_str(text).map_err(|e| LocalInspectError::BadUplugin {
        path: uplugin.to_path_buf(),
        reason: e.to_string(),
    })?;
    let get = |k: &str| v.get(k).and_then(|x| x.as_str()).map(str::to_string);
    Ok((
        get("FriendlyName").unwrap_or_default(),
        get("VersionName").unwrap_or_default(),
        get("EngineVersion"),
        get("Description"),
    ))
}

/// Houdini 标记：深度 ≤2 内有约定扩展名文件，或 hda/ 子目录含 dll。
fn has_houdini_markers(dir: &Path) -> bool {
    let Some(entries) = sorted_entries(dir) else { return false };
    let is_marker = |p: &Path| {
        p.extension()
            .is_some_and(|x| HOUDINI_EXTS.iter().any(|m| x.eq_ignore_ascii_case(m)))
    };
    for e in &entries {
        if e.is_file() && is_marker(e) {
            return true;
        }
        if e.is_dir() {
            if e.file_name().is_some_and(|n| n.eq_ignore_ascii_case("hda")) {
                if let Some(sub) = sorted_entries(e) {
                    if sub.iter().any(|s| s.extension().is_some_and(|x| x.eq_ignore_ascii_case("dll"))) {
                        return true;
                    }
                }
            } else if let Some(sub) = sorted_entries(e) {
                if sub.iter().any(|s| s.is_file() && is_marker(s)) {
                    return true;
                }
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_tree(tag: &str, files: &[(&str, &str)]) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "dpm-local-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
        ));
        for (rel, content) in files {
            let p = d.join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, content).unwrap();
        }
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn recognizes_nested_ue_plugin() {
        let dir = temp_tree(
            "ue-nested",
            &[
                ("README.md", "# x"),
                ("TrueGlow/TrueGlow.uplugin", r#"{"FriendlyName":"TrueGlow","VersionName":"0.8.2","EngineVersion":"4.26"}"#),
                ("TrueGlow/Resources/Icon128.png", "png"),
            ],
        );
        let p = inspect_local(&dir).unwrap();
        assert_eq!(p.kind, PluginKind::Ue);
        assert_eq!(p.id, "TrueGlow");
        assert_eq!(p.version, "0.8.2");
        assert_eq!(p.engine_version.as_deref(), Some("4.26"));
        assert!(p.plugin_root.ends_with("TrueGlow"));
    }

    #[test]
    fn recognizes_project_layout_repo() {
        // UE 工程式仓库：Repo/Plugins/<Name>/<Name>.uplugin（第三层）
        let dir = temp_tree(
            "ue-project-layout",
            &[
                ("BetterHLSL.uproject", "{}"),
                ("Source/BH.Target.cs", "x"),
                ("Plugins/BetterHLSL/BetterHLSL.uplugin", r#"{"VersionName":"2.1.0"}"#),
                ("Plugins/BetterHLSL/Source/b.cpp", "x"),
            ],
        );
        let p = inspect_local(&dir).unwrap();
        assert_eq!(p.id, "BetterHLSL");
        assert_eq!(p.version, "2.1.0");
        assert!(p.plugin_root.ends_with(r"Plugins\BetterHLSL"), "{:?}", p.plugin_root);
    }

    #[test]
    fn uplugin_bom_and_missing_fields() {
        // BOM + 缺 FriendlyName/EngineVersion
        let dir = temp_tree(
            "ue-bom",
            &[("Solo.uplugin", "\u{feff}{\"VersionName\":\"1.0\"}")],
        );
        let p = inspect_local(&dir).unwrap();
        assert_eq!(p.id, "Solo");
        assert_eq!(p.friendly_name, "Solo"); // 回退 id
        assert_eq!(p.version, "1.0");
        assert_eq!(p.engine_version, None);
    }

    #[test]
    fn recognizes_houdini_package() {
        let dir = temp_tree(
            "hou",
            &[
                ("tools/mp_sdf.otls", "bin"),
                ("hda/sop_foo.dll", "bin"),
                ("notes.txt", "x"),
            ],
        );
        let p = inspect_local(&dir).unwrap();
        assert_eq!(p.kind, PluginKind::Houdini);
        assert!(!p.id.is_empty());
        assert_eq!(p.plugin_root, dir);
    }

    #[test]
    fn empty_dir_is_unrecognized() {
        let dir = temp_tree("empty", &[]);
        let err = inspect_local(&dir).unwrap_err();
        assert!(err.to_string().contains("未识别插件结构"), "{err}");
    }

    #[test]
    fn not_a_directory() {
        let err = inspect_local(Path::new(r"Z:\definitely\not\here")).unwrap_err();
        assert!(matches!(err, LocalInspectError::NotADirectory(_)));
    }
}
