//! Release 附件安装：gh api 优先（私仓/免限流）→ curl 匿名 REST 回退；
//! 下载 zip → PowerShell Expand-Archive 解压 → 探测 .uplugin（根或一层嵌套）
//! → 复用 install::copy 落位。
//!
//! Windows-only 务实选型：不引 HTTP/zip crate，shell 出 curl.exe（Win10+ 自带）
//! 与 powershell.exe。

use crate::detect::ue::UeEngine;
use crate::install::copy::{install_binary, InstallError};
use crate::registry::InstalledTarget;
use crate::sources::git;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReleaseAsset {
    pub name: String,
    /// browser_download_url
    pub url: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReleaseInfo {
    pub tag: String,
    pub assets: Vec<ReleaseAsset>,
}

#[derive(Debug)]
pub enum ReleaseError {
    /// 仓库没有任何 Release（gh/REST 404 等）
    NoRelease(String),
    NoZipAsset { tag: String, assets: Vec<String> },
    Download { asset: String, detail: String },
    Extract { detail: String },
    NoUpluginInZip(PathBuf),
    Install(InstallError),
    CurlMissing,
}

impl std::fmt::Display for ReleaseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReleaseError::NoRelease(repo) => write!(f, "{repo} 没有 Release（可走源码构建，里程碑 3）"),
            ReleaseError::NoZipAsset { tag, assets } => write!(
                f,
                "Release {tag} 没有 zip 附件（现有：{}）——源码构建在里程碑 3 提供",
                assets.join("、")
            ),
            ReleaseError::Download { asset, detail } => write!(f, "下载 {asset} 失败：{detail}"),
            ReleaseError::Extract { detail } => write!(f, "解压失败：{detail}"),
            ReleaseError::NoUpluginInZip(p) => {
                write!(f, "zip 内未探测到 .uplugin（根或一层嵌套）：{}", p.display())
            }
            ReleaseError::Install(e) => write!(f, "{e}"),
            ReleaseError::CurlMissing => write!(f, "系统缺少 curl 且未安装 gh CLI"),
        }
    }
}

impl std::error::Error for ReleaseError {}

impl From<InstallError> for ReleaseError {
    fn from(e: InstallError) -> Self {
        ReleaseError::Install(e)
    }
}

fn no_window(mut cmd: Command) -> Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    cmd
}

/// 最新 Release 查询（gh 优先，curl 匿名回退）。
pub fn latest_release(repo_full: &str) -> Result<ReleaseInfo, ReleaseError> {
    let json = if git::gh_available() {
        let o = no_window(Command::new("gh"))
            .args(["api", &format!("repos/{repo_full}/releases/latest"), "--jq", "."])
            .output()
            .map_err(|_| ReleaseError::CurlMissing)?;
        if !o.status.success() {
            let err = String::from_utf8_lossy(&o.stderr);
            if err.contains("404") || err.contains("Not Found") {
                return Err(ReleaseError::NoRelease(repo_full.to_string()));
            }
            return Err(ReleaseError::Download {
                asset: format!("gh api {repo_full}"),
                detail: err.trim().to_string(),
            });
        }
        String::from_utf8_lossy(&o.stdout).into_owned()
    } else {
        let o = no_window(Command::new("curl"))
            .args([
                "-sSL",
                "-H",
                "Accept: application/vnd.github+json",
                "-H",
                "User-Agent: dcc-plugin-manager",
                &format!("https://api.github.com/repos/{repo_full}/releases/latest"),
            ])
            .output()
            .map_err(|_| ReleaseError::CurlMissing)?;
        let text = String::from_utf8_lossy(&o.stdout).into_owned();
        // REST 404 返回 JSON {"message":"Not Found",...}
        if text.contains("\"message\"") && text.contains("Not Found") {
            return Err(ReleaseError::NoRelease(repo_full.to_string()));
        }
        text
    };
    parse_release_json(&json).ok_or(ReleaseError::NoRelease(repo_full.to_string()))
}

/// Release JSON 解析（纯函数，供单测）。
pub fn parse_release_json(text: &str) -> Option<ReleaseInfo> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    let tag = v.get("tag_name")?.as_str()?.to_string();
    let assets = v
        .get("assets")?
        .as_array()?
        .iter()
        .filter_map(|a| {
            Some(ReleaseAsset {
                name: a.get("name")?.as_str()?.to_string(),
                url: a.get("browser_download_url")?.as_str()?.to_string(),
                size: a.get("size").and_then(|s| s.as_u64()).unwrap_or(0),
            })
        })
        .collect();
    Some(ReleaseInfo { tag, assets })
}

/// 选安装用 zip 附件：优先名含 win64/windows，其次首个 .zip。
pub fn pick_zip_asset(assets: &[ReleaseAsset]) -> Option<&ReleaseAsset> {
    let is_zip = |a: &ReleaseAsset| a.name.to_ascii_lowercase().ends_with(".zip");
    assets
        .iter()
        .find(|a| is_zip(a) && contains_any(&a.name, &["win64", "windows"]))
        .or_else(|| assets.iter().find(|a| is_zip(a)))
}

fn contains_any(name: &str, keys: &[&str]) -> bool {
    let lower = name.to_ascii_lowercase();
    keys.iter().any(|k| lower.contains(k))
}

/// 解压后探测插件目录：根目录有 .uplugin → 根；否则一层子目录含 → 该子目录。
pub fn probe_uplugin_dir(dir: &Path) -> Option<PathBuf> {
    let has_uplugin = |d: &Path| {
        std::fs::read_dir(d)
            .map(|it| {
                it.flatten()
                    .any(|e| e.path().is_file() && e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("uplugin")))
            })
            .unwrap_or(false)
    };
    if has_uplugin(dir) {
        return Some(dir.to_path_buf());
    }
    if let Ok(it) = std::fs::read_dir(dir) {
        for e in it.flatten() {
            let p = e.path();
            if p.is_dir() && has_uplugin(&p) {
                return Some(p);
            }
        }
    }
    None
}

/// PowerShell Expand-Archive（路径单引号转义）。
pub fn extract_zip(zip: &Path, dest: &Path) -> Result<(), ReleaseError> {
    std::fs::create_dir_all(dest).ok();
    let q = |p: &Path| format!("'{}'", p.to_string_lossy().replace('\'', "''"));
    let script = format!(
        "Expand-Archive -LiteralPath {} -DestinationPath {} -Force",
        q(zip),
        q(dest)
    );
    let o = no_window(Command::new("powershell"))
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .map_err(|e| ReleaseError::Extract { detail: e.to_string() })?;
    if !o.status.success() {
        return Err(ReleaseError::Extract {
            detail: String::from_utf8_lossy(&o.stderr).trim().to_string(),
        });
    }
    Ok(())
}

/// Release 安装全流程。成功返回 (安装记录, Release tag)。
pub fn install_release(
    repo_url: &str,
    engine: &UeEngine,
    data_dir: &Path,
    on_line: &mut dyn FnMut(&str),
) -> Result<(InstalledTarget, String), ReleaseError> {
    let full = git::repo_full_name(repo_url)
        .ok_or_else(|| ReleaseError::NoRelease(repo_url.to_string()))?;
    let rel = latest_release(&full)?;
    let asset =
        pick_zip_asset(&rel.assets).ok_or_else(|| ReleaseError::NoZipAsset {
            tag: rel.tag.clone(),
            assets: rel.assets.iter().map(|a| a.name.clone()).collect(),
        })?;

    let repo_name = git::repo_name(repo_url).unwrap_or_else(|| "repo".into());
    let release_dir = data_dir.join("cache").join("releases").join(repo_name);
    std::fs::create_dir_all(&release_dir).map_err(|e| ReleaseError::Extract { detail: e.to_string() })?;
    let zip_path = release_dir.join(&asset.name);
    let extract_dir = release_dir.join(format!(
        "{}.extracted",
        asset.name.trim_end_matches(".zip")
    ));

    on_line(&format!("下载 {}（{:.1} MB）", asset.name, asset.size as f64 / 1048576.0));
    let o = no_window(Command::new("curl"))
        .args(["-sSL", "-o"])
        .arg(&zip_path)
        .arg(&asset.url)
        .output()
        .map_err(|_| ReleaseError::CurlMissing)?;
    if !o.status.success() || !zip_path.is_file() || std::fs::metadata(&zip_path).map(|m| m.len()).unwrap_or(0) == 0
    {
        return Err(ReleaseError::Download {
            asset: asset.name.clone(),
            detail: String::from_utf8_lossy(&o.stderr).trim().to_string(),
        });
    }

    on_line(&format!("解压 {}", asset.name));
    extract_zip(&zip_path, &extract_dir)?;

    let plugin_dir = probe_uplugin_dir(&extract_dir).ok_or_else(|| {
        ReleaseError::NoUpluginInZip(extract_dir.clone())
    })?;

    on_line("拷贝安装到引擎");
    let target = install_binary(&plugin_dir, engine)?;
    Ok((target, rel.tag))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
        "tag_name": "v0.9.0",
        "assets": [
            {"name": "TrueGlow-v0.9.0-win64.zip", "browser_download_url": "https://example.com/a.zip", "size": 1048576},
            {"name": "TrueGlow-v0.9.0-mac.zip", "browser_download_url": "https://example.com/b.zip", "size": 2},
            {"name": "notes.txt", "browser_download_url": "https://example.com/c.txt", "size": 3}
        ]
    }"#;

    #[test]
    fn parses_release_json() {
        let rel = parse_release_json(SAMPLE).unwrap();
        assert_eq!(rel.tag, "v0.9.0");
        assert_eq!(rel.assets.len(), 3);
        assert_eq!(rel.assets[0].size, 1048576);
        // 404 message 体 → None
        assert!(parse_release_json(r#"{"message":"Not Found","documentation_url":"x"}"#).is_none());
    }

    #[test]
    fn picks_win64_zip_first() {
        let rel = parse_release_json(SAMPLE).unwrap();
        let picked = pick_zip_asset(&rel.assets).unwrap();
        assert_eq!(picked.name, "TrueGlow-v0.9.0-win64.zip");
        // 只剩非 win zip → 取首个 zip
        let only_zip = [ReleaseAsset {
            name: "plug.zip".into(),
            url: "u".into(),
            size: 1,
        }];
        assert_eq!(pick_zip_asset(&only_zip).unwrap().name, "plug.zip");
        // 无 zip → None
        let no_zip = [ReleaseAsset { name: "a.txt".into(), url: "u".into(), size: 1 }];
        assert!(pick_zip_asset(&no_zip).is_none());
    }

    #[test]
    fn probes_uplugin_root_and_nested() {
        let base = std::env::temp_dir().join(format!("dpm-rel-{}", std::process::id()));
        let root = base.join("probe-root");
        let nested = base.join("probe-nested");
        std::fs::create_dir_all(root.join("Content")).unwrap();
        std::fs::write(root.join("Solo.uplugin"), "{}").unwrap();
        std::fs::create_dir_all(nested.join("TrueGlow").join("Content")).unwrap();
        std::fs::write(nested.join("TrueGlow").join("TrueGlow.uplugin"), "{}").unwrap();
        let empty = base.join("probe-empty");
        std::fs::create_dir_all(&empty).unwrap();

        assert_eq!(probe_uplugin_dir(&root).unwrap(), root);
        assert!(probe_uplugin_dir(&nested).unwrap().ends_with("TrueGlow"));
        assert!(probe_uplugin_dir(&empty).is_none());
        std::fs::remove_dir_all(&base).ok();
    }

    /// 本地造 zip → 解压 → 探测（真实 powershell，无网络）。
    #[test]
    fn extracts_and_probes_local_zip() {
        let base = std::env::temp_dir().join(format!(
            "dpm-zip-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
        ));
        let src = base.join("src").join("TrueGlow");
        std::fs::create_dir_all(src.join("Binaries")).unwrap();
        std::fs::write(src.join("TrueGlow.uplugin"), "{}").unwrap();
        std::fs::write(src.join("Binaries").join("x.dll"), "dll").unwrap();
        let zip = base.join("pkg.zip");

        let q = |p: &Path| format!("'{}'", p.to_string_lossy().replace('\'', "''"));
        let script = format!(
            "Compress-Archive -Path {} -DestinationPath {}",
            q(&src),
            q(&zip)
        );
        let o = no_window(Command::new("powershell"))
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .output()
            .unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));

        let out_dir = base.join("extracted");
        extract_zip(&zip, &out_dir).unwrap();
        let probed = probe_uplugin_dir(&out_dir).expect("应探测到嵌套插件目录");
        assert!(probed.ends_with("TrueGlow"));
        assert!(probed.join("TrueGlow.uplugin").is_file());
        std::fs::remove_dir_all(&base).ok();
    }

    /// 真机网络：用户仓库当前无 Release → NoRelease（行为本身即验收点）。
    #[test]
    #[ignore = "真机网络依赖"]
    fn live_no_release_for_trueglow() {
        match latest_release("18163623522/TrueGlow") {
            Err(ReleaseError::NoRelease(_)) => {}
            other => panic!("期望 NoRelease，得到 {other:?}"),
        }
    }
}
