//! Task 1.6 集成测试：文件锁场景——DLL 被独占句柄持有时安装中止，解锁后成功。

use dcc_plugin_manager_lib::detect::ue::UeEngine;
use dcc_plugin_manager_lib::install::copy::{install_binary, InstallError};
use std::fs;
use std::path::PathBuf;
use windows_sys::Win32::Foundation::{CloseHandle, GENERIC_READ, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{CreateFileW, FILE_SHARE_NONE, OPEN_EXISTING};

fn temp(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "dpm-lock-{tag}-{}-{}",
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

/// 独占句柄（share = 0）：模拟 UnrealEditor 持有插件 DLL。
struct ExclusiveFile(HANDLE);
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
        assert!(h != INVALID_HANDLE_VALUE, "独占打开失败: {}", path.display());
        ExclusiveFile(h)
    }
}
impl Drop for ExclusiveFile {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

#[test]
fn locked_dll_blocks_reinstall_until_released() {
    let base = temp("lock");
    // 假引擎 + 假插件（含嵌套目录与 DLL）
    let plugin = base.join("TrueGlow");
    fs::create_dir_all(plugin.join("Binaries").join("Win64")).unwrap();
    fs::write(plugin.join("TrueGlow.uplugin"), "{}").unwrap();
    fs::write(plugin.join("Binaries").join("Win64").join("UE4Editor-TrueGlow.dll"), "dll").unwrap();

    let engine = UeEngine {
        version: "5.8.1".into(),
        root: base.clone(),
        runuat: base.join("Engine").join("Build").join("BatchFiles").join("RunUAT.bat"),
    };

    // 第一次安装成功
    let t1 = install_binary(&plugin, &engine).unwrap();
    let dll = t1.path.join("Binaries").join("Win64").join("UE4Editor-TrueGlow.dll");

    // 独占 DLL → 重装被拒，且旧安装不被破坏
    {
        let _hold = ExclusiveFile::open(&dll);
        let err = install_binary(&plugin, &engine).unwrap_err();
        match &err {
            InstallError::Locked(p) => assert!(p.ends_with("UE4Editor-TrueGlow.dll")),
            other => panic!("期望 Locked，得到 {other:?}"),
        }
        assert!(dll.exists(), "锁场景不得产生半删状态");
    } // 句柄释放

    // 解锁后重装成功
    install_binary(&plugin, &engine).unwrap();
    assert!(dll.exists());
}
