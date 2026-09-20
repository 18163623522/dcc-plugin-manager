//! Houdini 安装（M4）：插件目录拷到 `<偏好目录>\plugins\<id>`，
//! 并生成 `packages\<id>.json`（Houdini 标准 packages 格式）。
//!
//! registry 记录：`InstalledTarget.path` = 插件目录，
//! `sidecar` = packages json 路径（卸载时连带删除）。

use crate::detect::houdini::HoudiniInstall;
use crate::install::copy::{copy_dir_recursive, find_locked_file, InstallError};
use crate::registry::{houdini_engine_label, InstallMethod, InstalledTarget, now_rfc3339};
use std::path::{Path, PathBuf};

/// 安装 Houdini 插件目录源到指定 Houdini 版本。
pub fn install_houdini(
    src: &Path,
    plugin_id: &str,
    hou: &HoudiniInstall,
) -> Result<InstalledTarget, InstallError> {
    // 偏好目录 = packages 的父目录（Documents\houdini<major.minor>；尊重 HOUDINI_USER_PREF_DIR）
    let prefs = hou
        .packages_dir
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| hou.packages_dir.clone());
    let dest = prefs.join("plugins").join(plugin_id);
    let json_path = hou.packages_dir.join(format!("{plugin_id}.json"));

    // 旧安装锁检测（dll/so 被 Houdini 持有）——先检后删，不留半删状态
    if dest.exists() {
        if let Some(locked) = find_locked_file(&dest) {
            return Err(InstallError::Locked(locked));
        }
        std::fs::remove_dir_all(&dest).map_err(|e| InstallError::RemoveFailed(dest.clone(), e))?;
    }

    std::fs::create_dir_all(&dest)
        .map_err(|e| InstallError::CopyFailed(dest.clone(), e))?;
    copy_dir_recursive(src, &dest).map_err(|e| InstallError::CopyFailed(dest.clone(), e))?;

    // packages json：path + HOUDINI_PATH（尾部 & 并入既有路径；正斜杠免去 JSON 转义）
    std::fs::create_dir_all(&hou.packages_dir)
        .map_err(|e| InstallError::CopyFailed(hou.packages_dir.clone(), e))?;
    let fwd = |p: &PathBuf| p.to_string_lossy().replace('\\', "/");
    let json = format!(
        "{{\n  \"path\": \"{p}\",\n  \"env\": [\n    {{\"HOUDINI_PATH\": \"{p}&\"}}\n  ]\n}}",
        p = fwd(&dest)
    );
    std::fs::write(&json_path, json)
        .map_err(|e| InstallError::CopyFailed(json_path.clone(), e))?;

    Ok(InstalledTarget {
        engine: houdini_engine_label(&hou.version),
        path: dest,
        method: InstallMethod::BinaryCopy,
        installed_at: now_rfc3339(),
        sidecar: Some(json_path),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "dpm-hou-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn fake_hou(base: &Path) -> HoudiniInstall {
        // 模拟偏好目录结构：<base>/houdini22.0/packages
        let prefs = base.join("houdini22.0");
        HoudiniInstall {
            version: "22.0.368".into(),
            root: base.join("Houdini22"),
            packages_dir: prefs.join("packages"),
        }
    }

    #[test]
    fn installs_dir_and_generates_package_json() {
        let base = temp("ok");
        let src = base.join("mp_sdf");
        std::fs::create_dir_all(src.join("otls")).unwrap();
        std::fs::write(src.join("otls").join("mp_sdf.otls"), "bin").unwrap();
        std::fs::write(src.join("README.md"), "x").unwrap();

        let hou = fake_hou(&base);
        let t = install_houdini(&src, "mp_sdf", &hou).unwrap();

        assert_eq!(t.engine, "Houdini-22.0.368");
        assert!(t.path.ends_with(r"houdini22.0\plugins\mp_sdf"));
        assert!(t.path.join("otls").join("mp_sdf.otls").is_file());

        let json_path = t.sidecar.as_ref().unwrap();
        assert!(json_path.ends_with(r"packages\mp_sdf.json"));
        let text = std::fs::read_to_string(json_path).unwrap();
        assert!(text.contains("\"path\": \""), "{text}");
        assert!(text.contains("HOUDINI_PATH"));
        assert!(text.contains("/plugins/mp_sdf&\""), "HOUDINI_PATH 应带尾部 &：{text}");
        // JSON 可解析
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert!(v["env"][0]["HOUDINI_PATH"].as_str().unwrap().ends_with('&'));
    }

    #[test]
    fn reinstall_is_idempotent() {
        let base = temp("idem");
        let src = base.join("toolkit");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("toolkit.otls"), "bin").unwrap();

        let hou = fake_hou(&base);
        let t1 = install_houdini(&src, "toolkit", &hou).unwrap();
        std::fs::write(src.join("extra.hda"), "more").unwrap();
        let t2 = install_houdini(&src, "toolkit", &hou).unwrap();
        assert_eq!(t1.path, t2.path);
        assert!(t2.path.join("extra.hda").is_file(), "重装应整目录替换");
    }
}
