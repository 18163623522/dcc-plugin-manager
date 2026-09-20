//! 应用设置：`<data_dir>/settings.json`（损坏容错同 registry：备份后重建默认）。

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// 自定义 UE 引擎根目录（源码引擎等非 Launcher 安装，RunUAT 存在性校验）
    #[serde(default)]
    pub extra_ue_roots: Vec<PathBuf>,
}

pub const SETTINGS_FILE: &str = "settings.json";

pub fn load(app_dir: &std::path::Path) -> Settings {
    let file = app_dir.join(SETTINGS_FILE);
    let Ok(text) = std::fs::read_to_string(&file) else {
        return Settings::default();
    };
    match serde_json::from_str(&text) {
        Ok(s) => s,
        Err(_) => {
            let backup = app_dir.join(format!(
                "{SETTINGS_FILE}.bad.{}",
                chrono::Local::now().format("%Y%m%d%H%M%S")
            ));
            let _ = std::fs::copy(&file, &backup);
            Settings::default()
        }
    }
}

pub fn save(app_dir: &std::path::Path, s: &Settings) -> std::io::Result<()> {
    std::fs::create_dir_all(app_dir)?;
    let text = serde_json::to_string_pretty(s)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(app_dir.join(SETTINGS_FILE), text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_corruption() {
        let dir = std::env::temp_dir().join(format!("dpm-set-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let s = Settings {
            extra_ue_roots: vec![PathBuf::from(r"E:\UE\UE_5.7.4")],
        };
        save(&dir, &s).unwrap();
        assert_eq!(load(&dir).extra_ue_roots, s.extra_ue_roots);
        // 损坏 → 备份 + 默认
        std::fs::write(dir.join(SETTINGS_FILE), "{ broken").unwrap();
        assert!(load(&dir).extra_ue_roots.is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }
}
