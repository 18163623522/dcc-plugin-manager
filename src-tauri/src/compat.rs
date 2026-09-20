//! 版本兼容矩阵（设计 §3.7）：回答"这个插件有没有对应我某个引擎的版本"。
//!
//! 信号按可信度排序：
//! 1. 已安装记录 → Installed
//! 2. 分支/标签精确匹配（ue5.7 / ue4.26，容忍后缀）→ 拉该 ref 的 .uplugin
//!    读 EngineVersion 权威确认
//! 3. 家族匹配（裸 ue5 / ue4）→ Installable（未确认）
//! 4. 仓库名弱提示（MatHelper_UE426）
//! 5. 默认分支 EngineVersion 唯一声明 → 同版本可装 / 异版本 ❌ 需移植
//! 6. 无任何信号 → Unverified（前端提供"尝试编译"，M3）

use crate::detect::ue::UeEngine;
use crate::sources::git::{read_uplugin_at, RefInfo, RefKind};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::LazyLock;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum CompatStatus {
    Installed,
    /// { kind, gitRef, confirmed }
    #[serde(rename_all = "camelCase")]
    Installable { git_ref: String, confirmed: bool },
    Unverified,
    #[serde(rename_all = "camelCase")]
    Incompatible { reason: String },
}

#[derive(Debug, Clone, Serialize)]
pub struct EngineCompat {
    /// 引擎短版本（"5.7.4"）
    pub engine: String,
    pub status: CompatStatus,
}

/// ref 名解析出的 UE 版本信号。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RefVersion {
    /// ue5.7 / ue4.26 / ue5_7（容忍后缀 ue5.7-complete）
    Exact { major: u32, minor: u32 },
    /// 裸 ue5 / ue4
    Family { major: u32 },
}

static REF_EXACT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^v?ue[-_]?(\d+)[._-](\d+)").unwrap());
static REF_FAMILY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^v?ue[-_]?(\d+)$").unwrap());
/// 仓库名弱提示：MatHelper_UE426 / Foo_UE4_26（major 固定一位，余下为 minor）
static NAME_HINT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)[-_ ]ue(\d)[_ -]?(\d+)").unwrap());

pub fn parse_ref_ue_version(name: &str) -> Option<RefVersion> {
    if let Some(c) = REF_EXACT.captures(name.trim()) {
        return Some(RefVersion::Exact {
            major: c[1].parse().ok()?,
            minor: c[2].parse().ok()?,
        });
    }
    if let Some(c) = REF_FAMILY.captures(name.trim()) {
        return Some(RefVersion::Family { major: c[1].parse().ok()? });
    }
    None
}

/// 仓库名弱提示（MatHelper_UE426 → 4.26）。
pub fn parse_repo_name_hint(name: &str) -> Option<(u32, u32)> {
    let c = NAME_HINT.captures(name)?;
    Some((c[1].parse().ok()?, c[2].parse().ok()?))
}

/// uplugin JSON 文本 → EngineVersion（"5.7" / "4.26"）。
pub fn uplugin_engine_version(text: &str) -> Option<String> {
    let text = text.trim_start_matches('\u{feff}');
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    v.get("EngineVersion")?.as_str().map(str::to_string)
}

pub(crate) fn ev_major_minor(ev: &str) -> Option<(u32, u32)> {
    let mut it = ev.split('.');
    let major = it.next()?.trim().parse().ok()?;
    let minor = it.next().unwrap_or("0").trim().parse().unwrap_or(0);
    Some((major, minor))
}

/// compat 计算输入（全部来自本地 clone，零网络）。
pub struct CompatInput<'a> {
    /// 本地 clone 的仓库路径
    pub repo_path: &'a Path,
    pub default_ref: &'a str,
    /// 仓库名（弱提示）
    pub repo_name: &'a str,
    /// ls-remote 引用列表
    pub refs: &'a [RefInfo],
    /// 本机引擎
    pub engines: &'a [UeEngine],
    /// 已安装引擎短版本（"5.7.4"）
    pub installed: &'a [String],
}

/// 四层信号计算（§3.7）。
pub fn compute(input: &CompatInput) -> Vec<EngineCompat> {
    input
        .engines
        .iter()
        .map(|e| EngineCompat {
            engine: e.version.clone(),
            status: status_for(input, e),
        })
        .collect()
}

fn status_for(input: &CompatInput, engine: &UeEngine) -> CompatStatus {
    let Some((emajor, eminor)) = ev_major_minor(&engine.version) else {
        return CompatStatus::Unverified;
    };

    // 1. 已安装
    if input.installed.iter().any(|s| s == &engine.version) {
        return CompatStatus::Installed;
    }

    // 2. 精确 ref（分支优先于标签；EngineVersion 权威确认，错配的 ref 跳过）
    let mut exacts: Vec<&RefInfo> = input
        .refs
        .iter()
        .filter(|r| {
            matches!(
                parse_ref_ue_version(&r.name),
                Some(RefVersion::Exact { major, minor }) if major == emajor && minor == eminor
            )
        })
        .collect();
    exacts.sort_by_key(|r| if r.kind == RefKind::Branch { 0 } else { 1 });
    for r in &exacts {
        match read_uplugin_at(input.repo_path, &r.name) {
            Ok((_, text)) => match uplugin_engine_version(&text) {
                Some(ev) if ev_major_minor(&ev) == Some((emajor, eminor)) => {
                    return CompatStatus::Installable { git_ref: r.name.clone(), confirmed: true };
                }
                // 声明了别的版本：该 ref 不可信，看下一个候选
                Some(_) => continue,
                None => {
                    return CompatStatus::Installable { git_ref: r.name.clone(), confirmed: false };
                }
            },
            // 该分支没有 uplugin：结构可疑，跳过
            Err(_) => continue,
        }
    }

    // 3. 家族 ref（ue5 / ue4）
    if let Some(fam) = input.refs.iter().find(|r| {
        matches!(parse_ref_ue_version(&r.name), Some(RefVersion::Family { major }) if major == emajor)
    }) {
        return CompatStatus::Installable { git_ref: fam.name.clone(), confirmed: false };
    }

    // 4. 默认分支 EngineVersion：同版本可装（确认）；异版本 = 唯一声明 → 不兼容
    if let Ok((_, text)) = read_uplugin_at(input.repo_path, input.default_ref) {
        if let Some(ev) = uplugin_engine_version(&text) {
            return if ev_major_minor(&ev) == Some((emajor, eminor)) {
                CompatStatus::Installable {
                    git_ref: input.default_ref.to_string(),
                    confirmed: true,
                }
            } else {
                CompatStatus::Incompatible {
                    reason: format!(
                        "仓库仅声明 EngineVersion {ev}（默认分支 {}）——需要移植或换引擎",
                        input.default_ref
                    ),
                }
            };
        }
    }

    // 5. 仓库名弱提示
    if let Some((major, minor)) = parse_repo_name_hint(input.repo_name) {
        if (major, minor) == (emajor, eminor) {
            return CompatStatus::Installable { git_ref: input.default_ref.to_string(), confirmed: false };
        }
    }

    // 6. 无信号
    CompatStatus::Unverified
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ref_parsing() {
        use RefVersion::*;
        assert_eq!(parse_ref_ue_version("ue5.7"), Some(Exact { major: 5, minor: 7 }));
        assert_eq!(parse_ref_ue_version("ue4.26"), Some(Exact { major: 4, minor: 26 }));
        assert_eq!(parse_ref_ue_version("ue5_7"), Some(Exact { major: 5, minor: 7 }));
        assert_eq!(parse_ref_ue_version("UE5.7-complete"), Some(Exact { major: 5, minor: 7 }));
        assert_eq!(parse_ref_ue_version("ue5.7.3"), Some(Exact { major: 5, minor: 7 }));
        assert_eq!(parse_ref_ue_version("ue5"), Some(Family { major: 5 }));
        assert_eq!(parse_ref_ue_version("UE4"), Some(Family { major: 4 }));
        // 噪音：不匹配
        assert_eq!(parse_ref_ue_version("main"), None);
        assert_eq!(parse_ref_ue_version("v2.0"), None);
        assert_eq!(parse_ref_ue_version("release-2024"), None);
        assert_eq!(parse_ref_ue_version("feature/ues-fix"), None, "子目录路径不带 ^ 锚定噪音");
    }

    #[test]
    fn repo_name_hint_parsing() {
        assert_eq!(parse_repo_name_hint("MatHelper_UE426"), Some((4, 26)));
        assert_eq!(parse_repo_name_hint("Foo_UE4_26"), Some((4, 26)));
        assert_eq!(parse_repo_name_hint("PlainRepo"), None);
    }

    #[test]
    fn uplugin_ev_extraction() {
        assert_eq!(
            uplugin_engine_version(r#"{"EngineVersion":"4.26"}"#).as_deref(),
            Some("4.26")
        );
        assert_eq!(uplugin_engine_version(r#"{"FriendlyName":"x"}"#), None);
        assert_eq!(uplugin_engine_version("\u{feff}{\"EngineVersion\":\"5.7\"}").as_deref(), Some("5.7"));
    }
}
