//! Task 4.1/4.2 集成测试：Houdini 装→json 生成→卸载连带清理→重装幂等。

use dcc_plugin_manager_lib::detect::houdini::HoudiniInstall;
use dcc_plugin_manager_lib::install::houdini::install_houdini;
use dcc_plugin_manager_lib::registry::{
    self, InstallMethod, InstalledTarget, PluginEntry, PluginKind, PluginSource, Registry,
};
use dcc_plugin_manager_lib::uninstall::uninstall;
use std::fs;
use std::path::{Path, PathBuf};

fn temp(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "dpm-m4-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos()
    ));
    fs::create_dir_all(&d).unwrap();
    d
}

fn fake_hou(base: &PathBuf) -> HoudiniInstall {
    HoudiniInstall {
        version: "21.0.440".into(),
        root: base.join("Houdini21"),
        packages_dir: base.join("houdini21.0").join("packages"),
    }
}

fn source_pkg(base: &Path) -> PathBuf {
    let src = base.join("mp_sdf");
    fs::create_dir_all(src.join("otls")).unwrap();
    fs::write(src.join("otls").join("mp_sdf.otls"), "bin").unwrap();
    fs::write(src.join("toolbar.json"), "{}").unwrap();
    src
}

#[test]
fn install_uninstall_reinstall_roundtrip() {
    let base = temp("rt");
    let src = source_pkg(&base);
    let hou = fake_hou(&base);

    // 装
    let t = install_houdini(&src, "mp_sdf", &hou).unwrap();
    assert_eq!(t.engine, "Houdini-21.0.440");
    let plugin_dir = t.path.clone();
    let json = t.sidecar.clone().unwrap();
    assert!(plugin_dir.join("otls").join("mp_sdf.otls").is_file());
    assert!(json.is_file());

    // registry 登记 → 卸载
    let mut reg = Registry::default();
    reg.upsert(PluginEntry {
        id: "mp_sdf".into(),
        kind: PluginKind::Houdini,
        source: PluginSource::Local { path: src.clone() },
        version: "1.1".into(),
        desc: None,
        commit: None,
        local_digest: None,
        installed: vec![t],
    });
    let report = uninstall(&mut reg, "mp_sdf", &[], true);
    assert_eq!(report.removed.len(), 1);
    assert!(!plugin_dir.exists(), "插件目录应删除");
    assert!(!json.exists(), "packages json 应连带删除");
    assert!(reg.get("mp_sdf").unwrap().installed.is_empty(), "条目保留、安装清空");

    // 重装（目录已删，从零开始）→ 幂等
    let t2 = install_houdini(&src, "mp_sdf", &hou).unwrap();
    assert_eq!(t2.path, plugin_dir);
    assert!(t2.sidecar.unwrap().is_file());
}

#[test]
fn missing_json_sidecar_tolerated_on_uninstall() {
    // registry 记录存在但 json 已被手工删掉：目录删除不受影响
    let base = temp("orphan");
    let dir = base.join("houdini21.0").join("plugins").join("ghost");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("x.otls"), "bin").unwrap();

    let mut reg = Registry::default();
    reg.upsert(PluginEntry {
        id: "ghost".into(),
        kind: PluginKind::Houdini,
        source: PluginSource::Local { path: base.join("ghost-src") },
        version: "1.0".into(),
        desc: None,
        commit: None,
        local_digest: None,
        installed: vec![InstalledTarget {
            engine: "Houdini-21.0.440".into(),
            path: dir.clone(),
            sidecar: Some(base.join("houdini21.0").join("packages").join("ghost.json")),
            method: InstallMethod::BinaryCopy,
            installed_at: "t".into(),
        }],
    });
    let report = uninstall(&mut reg, "ghost", &[], true);
    assert_eq!(report.removed.len(), 1);
    assert!(!dir.exists());
}
