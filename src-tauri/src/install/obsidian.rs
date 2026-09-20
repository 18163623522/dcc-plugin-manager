//! Obsidian 安装（M5）：把构建产物三件套落位到 `<vault>\.obsidian\plugins\<id>\`。
//!
//! 产物来源（commands 层编排）：
//! - Release 散件：main.js + manifest.json（必需）+ styles.css（可选）
//! - 源码构建：仓库 `pnpm install && pnpm run build`（产物在仓库根，Obsidian 社区惯例）
//! - 本地目录：已构建过的开发目录（含 main.js）
//!
//! 不代写 community-plugins.json（Obsidian 运行中会回写，避免竞态）——
//! 安装完成提示用户在 Obsidian 设置里启用。

use crate::detect::obsidian::ObsidianVault;
use crate::install::copy::InstallError;
use crate::registry::{obsidian_engine_label, InstallMethod, InstalledTarget, now_rfc3339};
use serde_json::Value;
use std::path::Path;

/// 必需产物：main.js、manifest.json；可选：styles.css。
pub const REQUIRED_FILES: [&str; 2] = ["main.js", "manifest.json"];
pub const OPTIONAL_FILES: [&str; 1] = ["styles.css"];

/// 校验目录是否含 Obsidian 插件产物。
pub fn has_built_files(dir: &Path) -> bool {
    REQUIRED_FILES.iter().all(|f| dir.join(f).is_file())
}

/// 读产物 manifest.json 的 minAppVersion（兼容提示用）。
pub fn min_app_version(files_dir: &Path) -> Option<String> {
    let text = std::fs::read_to_string(files_dir.join("manifest.json")).ok()?;
    let v: Value = serde_json::from_str(&text).ok()?;
    v.get("minAppVersion")?.as_str().map(str::to_string)
}

/// 版本比较："1.4.0" vs "1.12.7"（数值逐段）。
pub fn version_lte(a: &str, b: &str) -> bool {
    let parse = |s: &str| -> Vec<u64> {
        s.split('.')
            .map(|p| p.trim().parse().unwrap_or(0))
            .collect()
    };
    let (va, vb) = (parse(a), parse(b));
    let n = va.len().max(vb.len());
    for i in 0..n {
        let x = va.get(i).copied().unwrap_or(0);
        let y = vb.get(i).copied().unwrap_or(0);
        if x != y {
            return x < y;
        }
    }
    true
}

/// 安装到 vault：只拷三件套（不拷整仓库），整目录替换幂等。
pub fn install_obsidian(
    files_src: &Path,
    vault: &ObsidianVault,
    method: InstallMethod,
    on_line: &mut dyn FnMut(&str),
) -> Result<InstalledTarget, InstallError> {
    if !has_built_files(files_src) {
        return Err(InstallError::CopyFailed(
            files_src.to_path_buf(),
            std::io::Error::other("缺少 main.js / manifest.json（先构建或选 Release）"),
        ));
    }
    let plugin_id = manifest_id(files_src).unwrap_or_else(|| "plugin".into());
    let dest = vault.path.join(".obsidian").join("plugins").join(&plugin_id);

    if dest.exists() {
        std::fs::remove_dir_all(&dest)
            .map_err(|e| InstallError::RemoveFailed(dest.clone(), e))?;
    }
    // 暂存目录拷三件套后改名落位（避免半拷状态被 Obsidian 看到）
    let staging = vault.path.join(".obsidian").join("plugins").join(format!(".{plugin_id}.staging"));
    if staging.exists() {
        let _ = std::fs::remove_dir_all(&staging);
    }
    std::fs::create_dir_all(&staging)
        .map_err(|e| InstallError::CopyFailed(staging.clone(), e))?;
    for f in REQUIRED_FILES.iter().chain(OPTIONAL_FILES.iter()) {
        let from = files_src.join(f);
        if from.is_file() {
            std::fs::copy(&from, staging.join(f))
                .map_err(|e| InstallError::CopyFailed(staging.join(f), e))?;
        }
    }
    std::fs::rename(&staging, &dest).map_err(|e| InstallError::CopyFailed(dest.clone(), e))?;

    on_line(&format!(
        "已安装到 {}（在 Obsidian 设置 → 第三方插件 中启用）",
        dest.display()
    ));
    Ok(InstalledTarget {
        engine: obsidian_engine_label(&vault.name),
        path: dest,
        sidecar: None,
        method,
        installed_at: now_rfc3339(),
    })
}

/// 产物 manifest.json 里的插件 id（落位目录名的权威来源）。
pub fn manifest_id(files_dir: &Path) -> Option<String> {
    let text = std::fs::read_to_string(files_dir.join("manifest.json")).ok()?;
    let v: Value = serde_json::from_str(&text).ok()?;
    v.get("id")?.as_str().map(str::to_string)
}

/// 源码构建：`pnpm install && pnpm run build`（产物在仓库根）。
/// 流式输出经 on_line；失败返回 stderr 尾部。
pub fn build_obsidian(repo: &Path, on_line: &mut dyn FnMut(&str)) -> Result<(), String> {
    for step in ["install", "run build"] {
        on_line(&format!("pnpm {step}"));
        let mut cmd = crate::preflight::no_window(std::process::Command::new("pnpm"));
        cmd.args(step.split(' ')).current_dir(repo);
        let out = cmd.output().map_err(|e| format!("无法启动 pnpm：{e}（未装 Node？）"))?;
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            on_line(line);
        }
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            let mut tail: Vec<&str> = err.lines().rev().take(5).collect();
            tail.reverse();
            let detail = if tail.is_empty() { err.trim().to_string() } else { tail.join("\n") };
            return Err(format!("pnpm {step} 失败：{detail}"));
        }
    }
    if !has_built_files(repo) {
        return Err("构建完成但仓库根没有 main.js/manifest.json（检查 esbuild 输出配置）".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "dpm-obs-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn fake_files(base: &Path, with_css: bool) -> PathBuf {
        let d = base.join("built");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("main.js"), "(()=>{})();").unwrap();
        std::fs::write(
            d.join("manifest.json"),
            r#"{"id":"quick-sticky","name":"Quick Sticky","version":"1.2.0","minAppVersion":"1.4.0"}"#,
        )
        .unwrap();
        if with_css {
            std::fs::write(d.join("styles.css"), "body{}").unwrap();
        }
        d
    }

    fn fake_vault(base: &Path) -> ObsidianVault {
        ObsidianVault {
            id: "abc123".into(),
            name: "笔记整理".into(),
            path: base.join("笔记整理"),
            open: true,
        }
    }

    #[test]
    fn installs_three_files_staging_then_rename() {
        let base = temp("ok");
        let files = fake_files(&base, true);
        let vault = fake_vault(&base);
        std::fs::create_dir_all(&vault.path).unwrap();

        let t = install_obsidian(&files, &vault, InstallMethod::Release, &mut |_| {}).unwrap();
        assert_eq!(t.engine, "Obsidian@笔记整理");
        let dest = t.path.clone();
        assert!(dest.ends_with(r"笔记整理\.obsidian\plugins\quick-sticky"));
        assert!(dest.join("main.js").is_file());
        assert!(dest.join("styles.css").is_file());
        // staging 不残留
        assert!(!vault.path.join(".obsidian").join("plugins").join(".quick-sticky.staging").exists());
        // 无 styles.css 的产物也能装
        let files2 = fake_files(&base.join("nocss"), false);
        let t2 = install_obsidian(&files2, &vault, InstallMethod::Release, &mut |_| {}).unwrap();
        assert!(t2.path.join("main.js").is_file());
        assert!(!t2.path.join("styles.css").exists());
    }

    #[test]
    fn missing_main_js_rejected() {
        let base = temp("bad");
        let d = base.join("empty");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("manifest.json"), r#"{"id":"x"}"#).unwrap();
        let vault = fake_vault(&base);
        std::fs::create_dir_all(&vault.path).unwrap();
        let err = install_obsidian(&d, &vault, InstallMethod::Build, &mut |_| {}).unwrap_err();
        assert!(err.to_string().contains("main.js"));
    }

    #[test]
    fn version_compare() {
        assert!(version_lte("1.4.0", "1.12.7"));
        assert!(!version_lte("1.13.0", "1.12.7"));
        assert!(version_lte("1.12.7", "1.12.7"));
        assert!(version_lte("1.0", "2.0"));
    }
}
