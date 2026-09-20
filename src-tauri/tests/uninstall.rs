//! Task 1.7 集成测试：安装→卸载往返 / missing 容错 / 锁中止 / 引擎粒度过滤。

use dcc_plugin_manager_lib::detect::ue::UeEngine;
use dcc_plugin_manager_lib::install::copy::install_binary;
use dcc_plugin_manager_lib::registry::{
    InstallMethod, InstalledTarget, PluginEntry, PluginKind, PluginSource, Registry,
};
use dcc_plugin_manager_lib::uninstall::{plan_targets, uninstall};
use std::fs;
use std::path::PathBuf;
use windows_sys::Win32::Foundation::{CloseHandle, GENERIC_READ, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{CreateFileW, FILE_SHARE_NONE, OPEN_EXISTING};

fn temp(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "dpm-uninst-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos()
    ));
    fs::create_dir_all(&d).unwrap();
    d
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

struct ExclusiveFile(windows_sys::Win32::Foundation::HANDLE);
impl ExclusiveFile {
    fn open(path: &std::path::Path) -> Self {
        let h = unsafe {
            CreateFileW(
                to_wide(&path.to_string_lossy()).as_ptr(),
                GENERIC_READ,
                FILE_SHARE_NONE,
                std::ptr::null(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            )
        };
        assert!(h != INVALID_HANDLE_VALUE);
        ExclusiveFile(h)
    }
}
impl Drop for ExclusiveFile {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

fn local_entry(id: &str, installed: Vec<InstalledTarget>) -> PluginEntry {
    PluginEntry {
        id: id.into(),
        kind: PluginKind::Ue,
        source: PluginSource::Local {
            path: PathBuf::from(r"D:\sources\{id}"),
        },
        version: "1.0".into(),
        commit: None,
        installed,
    }
}

#[test]
fn install_uninstall_roundtrip() {
    let base = temp("roundtrip");
    let plugin = base.join("TrueGlow");
    fs::create_dir_all(plugin.join("Binaries").join("Win64")).unwrap();
    fs::write(plugin.join("TrueGlow.uplugin"), "{}").unwrap();
    fs::write(plugin.join("Binaries").join("Win64").join("UE4Editor-TrueGlow.dll"), "dll").unwrap();

    let engine = UeEngine { version: "5.8.1".into(), root: base.clone(), runuat: base.join("x.bat") };
    let target = install_binary(&plugin, &engine).unwrap();

    let mut reg = Registry::default();
    reg.upsert(local_entry("TrueGlow", vec![target.clone()]));

    // 确认框数据
    let plan = plan_targets(&reg, "TrueGlow", &[]);
    assert_eq!(plan.len(), 1);
    assert!(plan[0].path.ends_with(r"Marketplace\TrueGlow"));

    let report = uninstall(&mut reg, "TrueGlow", &[], true);
    assert_eq!(report.removed.len(), 1);
    assert!(!target.path.exists(), "磁盘应删除");
    assert!(report.entry_exhausted);
    // 条目保留（源仍在管理），安装清空
    assert!(reg.get("TrueGlow").is_some());
    assert!(reg.get("TrueGlow").unwrap().installed.is_empty());
}

#[test]
fn missing_path_reported_not_crash() {
    let mut reg = Registry::default();
    reg.upsert(local_entry(
        "Ghost",
        vec![InstalledTarget {
            engine: "UE-5.7.4".into(),
            path: PathBuf::from(r"Z:\ghost\Marketplace\Ghost"),
            method: InstallMethod::BinaryCopy,
            installed_at: "2026-09-20T00:00:00+08:00".into(),
        }],
    ));
    let report = uninstall(&mut reg, "Ghost", &["UE-5.7.4".into()], true);
    assert_eq!(report.missing.len(), 1);
    assert!(report.removed.is_empty());
    assert!(reg.get("Ghost").unwrap().installed.is_empty(), "missing 也要清 registry");
}

#[test]
fn engine_filter_uninstalls_only_selected() {
    let mut reg = Registry::default();
    reg.upsert(local_entry(
        "Dual",
        vec![
            InstalledTarget {
                engine: "UE-5.7.4".into(),
                path: PathBuf::from(r"Z:\e57\Dual"),
                method: InstallMethod::BinaryCopy,
                installed_at: "t".into(),
            },
            InstalledTarget {
                engine: "UE-5.8.1".into(),
                path: PathBuf::from(r"Z:\e58\Dual"),
                method: InstallMethod::BinaryCopy,
                installed_at: "t".into(),
            },
        ],
    ));
    // 两个都缺盘 → 只按引擎卸 5.7：missing 1 条，5.8 记录保留
    let report = uninstall(&mut reg, "Dual", &["UE-5.7.4".into()], true);
    assert_eq!(report.missing.len(), 1);
    assert_eq!(report.missing[0].engine, "UE-5.7.4");
    let left = reg.get("Dual").unwrap();
    assert_eq!(left.installed.len(), 1);
    assert_eq!(left.installed[0].engine, "UE-5.8.1");
}

#[test]
fn locked_target_aborts_cleanly() {
    let base = temp("locked");
    let plugin = base.join("LockMe");
    fs::create_dir_all(plugin.join("Binaries").join("Win64")).unwrap();
    fs::write(plugin.join("LockMe.uplugin"), "{}").unwrap();
    fs::write(plugin.join("Binaries").join("Win64").join("UE4Editor-LockMe.dll"), "dll").unwrap();

    let engine = UeEngine { version: "5.6.1".into(), root: base.clone(), runuat: base.join("x.bat") };
    let target = install_binary(&plugin, &engine).unwrap();
    let dll = target.path.join("Binaries").join("Win64").join("UE4Editor-LockMe.dll");

    let mut reg = Registry::default();
    reg.upsert(local_entry("LockMe", vec![target.clone()]));

    {
        let _hold = ExclusiveFile::open(&dll);
        let report = uninstall(&mut reg, "LockMe", &[], true);
        assert_eq!(report.locked.len(), 1);
        assert!(report.removed.is_empty());
        assert!(dll.exists(), "锁场景不产生半删");
        assert_eq!(reg.get("LockMe").unwrap().installed.len(), 1, "registry 保留");
    }

    let report = uninstall(&mut reg, "LockMe", &[], true);
    assert_eq!(report.removed.len(), 1);
    assert!(!target.path.exists());
}
