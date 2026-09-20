//! 安装前诊断（设计 §3.8）：回答"为什么不能装"。
//!
//! 五项检查 → 可操作的原因列表；`blocking = true` 的项让前端禁用安装按钮
//! 并以 tooltip 展示原因；非阻塞项（未验证/磁盘紧张）照装但给出提示。

use crate::compat::CompatStatus;
use crate::detect::ue::UeEngine;
use crate::install::copy::find_locked_file;
use crate::registry::PluginEntry;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PreflightKey {
    CompatSource,
    VsToolchain,
    EngineComplete,
    FileLock,
    DiskSpace,
}

#[derive(Debug, Clone, Serialize)]
pub struct PreflightItem {
    pub key: PreflightKey,
    pub ok: bool,
    /// 阻塞项：安装按钮禁用的原因
    pub blocking: bool,
    pub message: String,
}

/// 默认 vswhere 路径。
pub const VSWHERE: &str = r"C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe";

/// 全量预检。`compat` 是该引擎的兼容状态（调用方算好传入，避免此处联网）。
pub fn check_all(
    entry: &PluginEntry,
    engine: &UeEngine,
    compat: Option<&CompatStatus>,
    cache_dir: &Path,
) -> Vec<PreflightItem> {
    vec![
        compat_source(compat),
        vs_toolchain(),
        engine_complete(engine),
        file_lock(entry, engine),
        disk_space(entry, engine, cache_dir),
    ]
}

// 1. 兼容源存在（Unverified 不阻塞：试编译即裁决，§3.7）
fn compat_source(compat: Option<&CompatStatus>) -> PreflightItem {
    let (ok, blocking, message) = match compat {
        None => (true, false, "本地源：版本由你自行确认".into()),
        Some(CompatStatus::Installed) => (true, false, "已有安装记录（将覆盖重装）".into()),
        Some(CompatStatus::Installable { git_ref, .. }) => {
            (true, false, format!("将构建分支 {git_ref}"))
        }
        Some(CompatStatus::Unverified) => (
            true,
            false,
            "未验证引擎——安装即试编译，成败为最终裁决".into(),
        ),
        Some(CompatStatus::Incompatible { reason }) => (false, true, reason.clone()),
    };
    PreflightItem { key: PreflightKey::CompatSource, ok, blocking, message }
}

// 2. VS C++ 工具链（源码构建可用性；Release 包不受影响——非阻塞降级提示）
// 安全约束：可执行文件只允许编译期字面量路径，不接受任何调用方传入值。
pub fn vs_toolchain() -> PreflightItem {
    if !Path::new(VSWHERE).is_file() {
        return PreflightItem {
            key: PreflightKey::VsToolchain,
            ok: false,
            blocking: false,
            message: "未找到 vswhere——无法确认 VS 工具链，源码构建可用性未知".into(),
        };
    }
    match vswhere_status() {
        Some(true) => PreflightItem {
            key: PreflightKey::VsToolchain,
            ok: true,
            blocking: false,
            message: "VS C++ 工具链可用".into(),
        },
        Some(false) => PreflightItem {
            key: PreflightKey::VsToolchain,
            ok: false,
            blocking: false,
            message: "未检测到 VS C++ 工具链（vswhere 无 VC.Tools 匹配），源码构建不可用——仅可装 Release 预编译包".into(),
        },
        None => PreflightItem {
            key: PreflightKey::VsToolchain,
            ok: false,
            blocking: false,
            message: "vswhere 查询失败——无法确认 VS 工具链".into(),
        },
    }
}

/// 运行 vswhere（编译期字面量路径 + 参数列表，无 shell、无调用方输入）。
fn vswhere_status() -> Option<bool> {
    let out = no_window(Command::new(
        r"C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe",
    ))
    .args([
        "-products",
        "*",
        "-requires",
        "Microsoft.VisualStudio.Component.VC.Tools.x86.x64",
        "-property",
        "installationName",
    ])
    .output()
    .ok()?;
    Some(out.status.success() && !String::from_utf8_lossy(&out.stdout).trim().is_empty())
}

// 3. 引擎完整性（RunUAT 存在）
fn engine_complete(engine: &UeEngine) -> PreflightItem {
    let ok = engine.runuat.is_file();
    PreflightItem {
        key: PreflightKey::EngineComplete,
        ok,
        blocking: !ok,
        message: if ok {
            "RunUAT.bat 就位".into()
        } else {
            format!("{} 缺少 Engine\\Build\\BatchFiles\\RunUAT.bat——引擎安装不完整", engine.root.display())
        },
    }
}

// 4. 文件锁（目标目录现存 DLL 被占用 → 列出运行中的 UnrealEditor 辅助定位）
fn file_lock(entry: &PluginEntry, engine: &UeEngine) -> PreflightItem {
    let dest = engine
        .root
        .join("Engine")
        .join("Plugins")
        .join("Marketplace")
        .join(&entry.id);
    if !dest.exists() {
        return PreflightItem {
            key: PreflightKey::FileLock,
            ok: true,
            blocking: false,
            message: "目标目录空闲".into(),
        };
    }
    match find_locked_file(&dest) {
        None => PreflightItem {
            key: PreflightKey::FileLock,
            ok: true,
            blocking: false,
            message: "旧安装无文件锁（将整目录替换）".into(),
        },
        Some(locked) => {
            let editors = running_unreal_editors();
            let hint = if editors.is_empty() {
                String::new()
            } else {
                format!("（运行中：{}）", editors.join("、"))
            };
            PreflightItem {
                key: PreflightKey::FileLock,
                ok: false,
                blocking: true,
                message: format!(
                    "{} 被占用{hint}——请先关闭对应 UE 后重试",
                    locked.display()
                ),
            }
        }
    }
}

/// tasklist 枚举 UnrealEditor 进程（CSV 名单，纯解析供单测）。
pub fn running_unreal_editors() -> Vec<String> {
    let out = match no_window(Command::new("tasklist"))
        .args(["/FI", "IMAGENAME eq UnrealEditor.exe", "/FO", "CSV", "/NH"])
        .output()
    {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).into_owned(),
        _ => return Vec::new(),
    };
    parse_tasklist_editors(&out)
}

/// CSV 行 `"UnrealEditor.exe","1234","Console",...` → `UnrealEditor.exe#1234`。
fn parse_tasklist_editors(csv: &str) -> Vec<String> {
    csv.lines()
        .filter(|l| l.to_ascii_lowercase().contains("unrealeditor"))
        .filter_map(|l| {
            let fields: Vec<&str> = l.split("\",\"").collect();
            let name = fields.first()?.trim_matches('"');
            let pid = fields.get(1)?.trim_matches('"');
            Some(format!("{name}#{pid}"))
        })
        .collect()
}

// 5. 磁盘空间（缓存/构建驱动 ≥ 2GB；引擎驱动 ≥ 插件体积×2 + 200MB）
fn disk_space(entry: &PluginEntry, engine: &UeEngine, cache_dir: &Path) -> PreflightItem {
    let src_size = dir_size(&match &entry.source {
        crate::registry::PluginSource::Local { path } => path.clone(),
        crate::registry::PluginSource::Git { .. } => PathBuf::new(), // git 源体积由 clone 决定，用保守常量
    });
    let need_cache: u64 = 2 * 1024 * 1024 * 1024;
    let need_engine: u64 = src_size.saturating_mul(2).max(200 * 1024 * 1024);

    let free_engine = disk_free_bytes(&engine.root);
    let free_cache = disk_free_bytes(cache_dir);
    disk_space_item(free_engine, free_cache, need_engine, need_cache)
}

/// 决策纯函数（供单测）。
fn disk_space_item(free_engine: u64, free_cache: u64, need_engine: u64, need_cache: u64) -> PreflightItem {
    let cache_short = free_cache < need_cache;
    let engine_short = free_engine < need_engine;
    let gb = |b: u64| format!("{:.1} GB", b as f64 / 1_073_741_824.0);
    if !cache_short && !engine_short {
        return PreflightItem {
            key: PreflightKey::DiskSpace,
            ok: true,
            blocking: false,
            message: format!("磁盘充足（构建 {} / 引擎 {}）", gb(free_cache), gb(free_engine)),
        };
    }
    PreflightItem {
        key: PreflightKey::DiskSpace,
        ok: false,
        blocking: false,
        message: if cache_short && engine_short {
            format!(
                "磁盘空间不足：构建盘剩 {}（需 {}）、引擎盘剩 {}（需 {}）",
                gb(free_cache),
                gb(need_cache),
                gb(free_engine),
                gb(need_engine)
            )
        } else if cache_short {
            format!(
                "构建缓存盘空间不足：剩 {}（需 {}）——清理缓存或换缓存目录",
                gb(free_cache),
                gb(need_cache)
            )
        } else {
            format!(
                "引擎盘空间不足：剩 {}（需 {}）",
                gb(free_engine),
                gb(need_engine)
            )
        },
    }
}

/// GetDiskFreeSpaceExW 可用字节数（失败返回 u64::MAX = 不误报）。
pub fn disk_free_bytes(any_path: &Path) -> u64 {
    #[cfg(windows)]
    {
        let wide: Vec<u16> = any_path
            .to_string_lossy()
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let mut free: u64 = 0;
        let ok = unsafe {
            windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW(
                wide.as_ptr(),
                &mut free,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        if ok != 0 {
            free
        } else {
            u64::MAX
        }
    }
    #[cfg(not(windows))]
    {
        let _ = any_path;
        u64::MAX
    }
}

pub(crate) fn dir_size(dir: &Path) -> u64 {
    let mut total = 0u64;
    if let Ok(it) = std::fs::read_dir(dir) {
        for e in it.flatten() {
            let p = e.path();
            if p.is_dir() {
                total += dir_size(&p);
            } else if let Ok(m) = e.metadata() {
                total += m.len();
            }
        }
    }
    total
}

pub(crate) fn no_window(mut cmd: Command) -> Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compat_source_states() {
        assert!(compat_source(None).ok);
        assert!(compat_source(Some(&CompatStatus::Installed)).ok);
        assert!(compat_source(Some(&CompatStatus::Unverified)).ok);
        assert!(!compat_source(Some(&CompatStatus::Unverified)).blocking);
        let bad = compat_source(Some(&CompatStatus::Incompatible {
            reason: "仓库仅声明 4.26".into(),
        }));
        assert!(!bad.ok && bad.blocking && bad.message.contains("4.26"));
        let inst = compat_source(Some(&CompatStatus::Installable {
            git_ref: "ue5.7".into(),
            confirmed: true,
        }));
        assert!(inst.ok && inst.message.contains("ue5.7"));
    }

    #[test]
    fn tasklist_parse() {
        let csv = "\"UnrealEditor.exe\",\"1234\",\"Console\",\"1\",\"2,000 K\"\r\n\"explorer.exe\",\"99\",\"Console\",\"1\",\"50 K\"\r\n\"UnrealEditor.exe\",\"5678\",\"Console\",\"1\",\"3,000 K\"\r\n";
        let editors = parse_tasklist_editors(csv);
        assert_eq!(editors, vec!["UnrealEditor.exe#1234", "UnrealEditor.exe#5678"]);
        assert!(parse_tasklist_editors("INFO: No tasks are running").is_empty());
    }

    #[test]
    fn disk_decisions() {
        let gb = 1024 * 1024 * 1024u64;
        let ok = disk_space_item(50 * gb, 10 * gb, gb, 2 * gb);
        assert!(ok.ok);
        let cache_short = disk_space_item(50 * gb, gb, gb, 2 * gb);
        assert!(!cache_short.ok && !cache_short.blocking);
        assert!(cache_short.message.contains("构建缓存盘"));
        let engine_short = disk_space_item(gb, 10 * gb, 5 * gb, 2 * gb);
        assert!(engine_short.message.contains("引擎盘"));
        let both = disk_space_item(gb, gb, 5 * gb, 2 * gb);
        assert!(both.message.contains("构建盘") && both.message.contains("引擎盘"));
    }

    #[test]
    fn vs_toolchain_item_is_non_blocking() {
        // 无论本机有无 vswhere/工具链，该项都不阻塞安装（Release 路径可用性提示）
        let item = vs_toolchain();
        assert!(!item.blocking);
        assert!(!item.message.is_empty());
    }

    /// 真机：C 盘可用空间应可读且巨大。
    #[test]
    fn disk_free_on_c() {
        assert!(disk_free_bytes(Path::new(r"C:\")) > 1024 * 1024 * 1024);
    }
}
