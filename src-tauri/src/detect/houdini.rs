//! Houdini 检测。
//!
//! 本机实测的注册表布局（两套并存，均要覆盖）：
//! - 布局 A（权威）：`HKLM\SOFTWARE\Side Effects Software\Houdini <version>`
//!   子键，内含 `InstallPath`、`Version`（如 22.0.368）。
//! - 布局 B（兜底）：`HKLM\SOFTWARE\Side Effects Software\Houdini` 键下的
//!   值，值名为四段版本串（如 "21.0.0.440"），值数据为安装路径；
//!   同键还混有 LicenseServer 等非版本值。
//!
//! packages 目录：默认 `<USERPROFILE>\Documents\houdini<major.minor>\packages`，
//! 尊重 `HOUDINI_USER_PREF_DIR` 覆盖。

use serde::Serialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_64KEY};
use winreg::RegKey;

/// 检测到的一个 Houdini 安装。
#[derive(Debug, Clone, Serialize)]
pub struct HoudiniInstall {
    /// 形如 "22.0.368"
    pub version: String,
    /// InstallPath（如 D:\004_App\P_Softwave\houdini22_143）
    pub root: PathBuf,
    /// 用户 packages 目录（不保证存在，安装时创建）
    pub packages_dir: PathBuf,
}

/// 检测本机全部 Houdini 安装（布局 A + B 合并，按根目录去重，版本降序）。
pub fn detect_houdini() -> Vec<HoudiniInstall> {
    let mut out: Vec<HoudiniInstall> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    let ses = match RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey_with_flags(
        r"SOFTWARE\Side Effects Software",
        KEY_READ | KEY_WOW64_64KEY,
    ) {
        Ok(k) => k,
        Err(_) => return out, // 未安装
    };

    // 布局 A：子键 "Houdini <version>" → InstallPath
    for subkey in ses.enum_keys().flatten() {
        let Some(version) = versioned_houdini_subkey(&subkey) else { continue };
        let Ok(sk) = ses.open_subkey(&subkey) else { continue };
        let Ok(path) = sk.get_value::<String, _>("InstallPath") else { continue };
        let root = PathBuf::from(path.trim_end_matches('\\'));
        if !root.is_dir() {
            continue;
        }
        if seen.insert(key_of(&root)) {
            out.push(HoudiniInstall {
                packages_dir: packages_dir_for(&version),
                root,
                version,
            });
        }
    }

    // 布局 B：Houdini 键下 值名=版本串 → 安装路径
    if let Ok(houdini_key) = ses.open_subkey("Houdini") {
        for (name, value) in houdini_key.enum_values().flatten() {
            let Some(version) = normalize_value_version(&name) else { continue };
            let root = PathBuf::from(value.to_string().trim_end_matches('\\'));
            if !root.is_dir() {
                continue;
            }
            if seen.insert(key_of(&root)) {
                out.push(HoudiniInstall {
                    packages_dir: packages_dir_for(&version),
                    root,
                    version,
                });
            }
        }
    }

    out.sort_by(|a, b| b.version.cmp(&a.version));
    out
}

fn key_of(root: &Path) -> String {
    root.to_string_lossy().to_lowercase()
}

/// "Houdini 22.0.368" → "22.0.368"；"Houdini"/"HoudiniCurrent" 等 → None。
fn versioned_houdini_subkey(name: &str) -> Option<String> {
    let rest = name.strip_prefix("Houdini ")?.trim();
    version_subkey(rest)
}

/// 四段值名 "21.0.0.440" → 三段 "21.0.440"（第三段恒 0 的安装器编号习惯）；
/// 三段直接通过；其他（LicenseServer 等）→ None。
fn normalize_value_version(name: &str) -> Option<String> {
    let parts: Vec<&str> = name.split('.').collect();
    match parts.as_slice() {
        [a, b, "0", d] => {
            let joined = format!("{a}.{b}.{d}");
            version_subkey(&joined).map(|_| joined)
        }
        [_, _, _, _, ..] => None, // 其他四段及以上：不猜
        _ => version_subkey(name),
    }
}

/// 版本串校验：至少两段、全数字。
fn version_subkey(name: &str) -> Option<String> {
    let mut segments = 0;
    for part in name.split('.') {
        if part.is_empty() || !part.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        segments += 1;
    }
    if segments >= 2 {
        Some(name.to_string())
    } else {
        None
    }
}

/// packages 目录（读真实环境变量）。
pub fn packages_dir_for(version: &str) -> PathBuf {
    let pref_override = std::env::var("HOUDINI_USER_PREF_DIR").ok();
    let user_profile = std::env::var_os("USERPROFILE").map(PathBuf::from);
    packages_dir_with(version, user_profile.as_deref(), pref_override.as_deref())
}

/// packages 目录推导（纯函数，供单测）：
/// 默认 `<USERPROFILE>\Documents\houdini<major.minor>\packages`；
/// HOUDINI_USER_PREF_DIR 设置时视为偏好目录本体，packages 拼在其下。
/// （环境变量值按字面路径处理，$HOME 展开不支持——本机场景未用到）
pub fn packages_dir_with(
    version: &str,
    user_profile: Option<&Path>,
    pref_override: Option<&str>,
) -> PathBuf {
    if let Some(pref) = pref_override.map(str::trim).filter(|s| !s.is_empty()) {
        return PathBuf::from(pref).join("packages");
    }
    let major_minor: String = version
        .split('.')
        .take(2)
        .collect::<Vec<_>>()
        .join(".");
    let home = user_profile.unwrap_or_else(|| Path::new(r"C:\Users\Default"));
    home.join("Documents")
        .join(format!("houdini{major_minor}"))
        .join("packages")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subkey_name_parsing() {
        assert_eq!(
            versioned_houdini_subkey("Houdini 22.0.368").as_deref(),
            Some("22.0.368")
        );
        assert_eq!(
            versioned_houdini_subkey("Houdini 21.0").as_deref(),
            Some("21.0")
        );
        assert!(versioned_houdini_subkey("Houdini").is_none());
        assert!(versioned_houdini_subkey("HoudiniCurrent").is_none());
        assert!(versioned_houdini_subkey("Something 1.2.3").is_none());
    }

    #[test]
    fn value_name_normalization() {
        assert_eq!(
            normalize_value_version("21.0.0.440").as_deref(),
            Some("21.0.440")
        );
        assert_eq!(
            normalize_value_version("22.0.368").as_deref(),
            Some("22.0.368")
        );
        assert!(normalize_value_version("LicenseServer").is_none());
        assert!(normalize_value_version("21.1.2.440").is_none(), "第三段非 0 的四段串不猜");
    }

    #[test]
    fn packages_dir_default_shape() {
        let dir = packages_dir_with("22.0.368", Some(Path::new(r"C:\Users\pengxiwei")), None);
        assert_eq!(
            dir,
            PathBuf::from(r"C:\Users\pengxiwei\Documents\houdini22.0\packages")
        );
    }

    #[test]
    fn packages_dir_pref_override_wins() {
        let dir = packages_dir_with(
            "22.0.368",
            Some(Path::new(r"C:\Users\pengxiwei")),
            Some(r"E:\prefs\houdini22.0"),
        );
        assert_eq!(dir, PathBuf::from(r"E:\prefs\houdini22.0\packages"));
        // 空白覆盖视同未设置
        let fallback =
            packages_dir_with("22.0.368", Some(Path::new(r"C:\Users\pengxiwei")), Some("  "));
        assert!(fallback.starts_with(r"C:\Users\pengxiwei"));
    }

    /// 真机冒烟：本机装有 21.0.440 / 21.0.631 / 22.0.368（cargo test -- --ignored）。
    #[test]
    #[ignore = "真机注册表依赖"]
    fn live_detect_smoke() {
        let installs = detect_houdini();
        println!("Houdini installs: {installs:#?}");
        assert!(installs.iter().any(|i| i.version == "22.0.368"));
    }
}
