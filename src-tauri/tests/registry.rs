//! Task 1.4 集成测试：registry 磁盘读写往返 / 文件缺失 / 损坏恢复备份。

use dcc_plugin_manager_lib::registry::{
    self, InstallMethod, InstalledTarget, PluginEntry, PluginKind, PluginSource, Registry,
};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_dir(tag: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .subsec_nanos();
    let d = std::env::temp_dir().join(format!("dpm-registry-{tag}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&d).unwrap();
    d
}

fn sample() -> Registry {
    let mut r = Registry::default();
    r.upsert(PluginEntry {
        id: "TrueGlow".into(),
        kind: PluginKind::Ue,
        source: PluginSource::Git {
            url: "https://github.com/18163623522/TrueGlow".into(),
            default_ref: Some("main".into()),
        },
        version: "0.8.2".into(),
        desc: None,
        commit: Some("8ada112".into()),
        local_digest: None,
        installed: vec![InstalledTarget {
            engine: "UE-5.8.1".into(),
            path: PathBuf::from(r"E:\UE\UE_5.8\Engine\Plugins\Marketplace\TrueGlow"),
            method: InstallMethod::BinaryCopy,
            installed_at: "2026-09-20T11:00:00+08:00".into(),
        }],
    });
    r
}

#[test]
fn save_load_roundtrip() {
    let dir = temp_dir("roundtrip");
    let r = sample();
    registry::save(&dir, &r).unwrap();
    let loaded = registry::load(&dir);
    assert_eq!(loaded, r);
}

#[test]
fn missing_file_loads_empty() {
    let dir = temp_dir("missing");
    assert_eq!(registry::load(&dir), Registry::default());
}

#[test]
fn corrupt_json_backs_up_and_rebuilds() {
    let dir = temp_dir("corrupt");
    fs::write(dir.join(registry::REGISTRY_FILE), "{ broken json !!!").unwrap();

    assert_eq!(registry::load(&dir), Registry::default());

    let backups: Vec<String> = fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("registry.json.bad."))
        .collect();
    assert_eq!(backups.len(), 1, "损坏文件应恰好备份一份: {backups:?}");
    assert_eq!(
        fs::read_to_string(dir.join(&backups[0])).unwrap(),
        "{ broken json !!!"
    );
}

#[test]
fn overwrite_roundtrip_twice() {
    let dir = temp_dir("twice");
    registry::save(&dir, &sample()).unwrap();
    let mut r2 = sample();
    r2.plugins[0].version = "0.9.0".into();
    registry::save(&dir, &r2).unwrap();
    assert_eq!(registry::load(&dir), r2);
    // 不留 tmp 残留
    let leftovers: Vec<String> = fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".tmp"))
        .collect();
    assert!(leftovers.is_empty());
}
