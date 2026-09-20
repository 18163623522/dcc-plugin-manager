//! 二进制手动安装：整目录拷贝到 `Engine\Plugins\Marketplace\<Name>`。
//!
//! 已存在目标 → 先做文件锁检测（避免半删状态），再删旧拷新；
//! 拷贝成功即返回 InstalledTarget（registry 登记由调用方落盘）。

use crate::detect::ue::UeEngine;
use crate::registry::{ue_engine_label, InstallMethod, InstalledTarget, now_rfc3339};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum InstallError {
    /// 源目录里找不到 .uplugin
    NoUplugin(PathBuf),
    /// 文件被占用（ERROR_SHARING_VIOLATION，多半是引擎编辑器持有 DLL）
    Locked(PathBuf),
    RemoveFailed(PathBuf, io::Error),
    CopyFailed(PathBuf, io::Error),
}

impl std::fmt::Display for InstallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InstallError::NoUplugin(p) => write!(f, "源目录无 .uplugin：{}", p.display()),
            InstallError::Locked(p) => write!(
                f,
                "{} 被占用（引擎编辑器可能正在使用）——请关闭对应 UE 后重试",
                p.display()
            ),
            InstallError::RemoveFailed(p, e) => write!(f, "删除旧安装失败（{}）：{e}", p.display()),
            InstallError::CopyFailed(p, e) => write!(f, "拷贝失败（{}）：{e}", p.display()),
        }
    }
}

impl std::error::Error for InstallError {}

/// 把 `src`（含 .uplugin 的插件目录）安装到引擎全局 Marketplace。
pub fn install_binary(src: &Path, engine: &UeEngine) -> Result<InstalledTarget, InstallError> {
    let id = uplugin_stem(src).ok_or_else(|| InstallError::NoUplugin(src.to_path_buf()))?;
    let dest = engine
        .root
        .join("Engine")
        .join("Plugins")
        .join("Marketplace")
        .join(&id);

    if dest.exists() {
        // 先锁检测再删除：绝不留半删状态
        if let Some(locked) = find_locked_file(&dest) {
            return Err(InstallError::Locked(locked));
        }
        fs::remove_dir_all(&dest).map_err(|e| InstallError::RemoveFailed(dest.clone(), e))?;
    }

    copy_dir_recursive(src, &dest).map_err(|e| InstallError::CopyFailed(dest.clone(), e))?;

    Ok(InstalledTarget {
        engine: ue_engine_label(&engine.version),
        path: dest,
        method: InstallMethod::BinaryCopy,
        installed_at: now_rfc3339(),
    })
}

/// 源目录本层的 .uplugin 文件名（去后缀）。
fn uplugin_stem(src: &Path) -> Option<String> {
    let first = fs::read_dir(src)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file() && p.extension().is_some_and(|x| x.eq_ignore_ascii_case("uplugin"))
        })
        .min()?;
    first.file_stem().map(|s| s.to_string_lossy().into_owned())
}

/// 遍历目录找被独占锁定的 DLL（ERROR_SHARING_VIOLATION = 32）。
/// 只探测 dll：编辑器加载的就是模块 DLL；只读属性（ACCESS_DENIED=5）不算锁。
pub(crate) fn find_locked_file(dir: &Path) -> Option<PathBuf> {
    let entries = match fs::read_dir(dir) {
        Ok(it) => it.flatten().map(|e| e.path()).collect::<Vec<_>>(),
        Err(_) => return None,
    };
    for p in entries {
        if p.is_dir() {
            if let Some(found) = find_locked_file(&p) {
                return Some(found);
            }
        } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("dll")) {
            if let Err(e) = fs::OpenOptions::new().read(true).write(true).open(&p) {
                if e.raw_os_error() == Some(32) {
                    return Some(p);
                }
            }
        }
    }
    None
}

/// 递归拷贝目录，返回拷贝的文件数。
fn copy_dir_recursive(src: &Path, dest: &Path) -> io::Result<u64> {
    fs::create_dir_all(dest)?;
    let mut count = 0u64;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let to = dest.join(entry.file_name());
        if ty.is_dir() {
            count += copy_dir_recursive(&entry.path(), &to)?;
        } else {
            fs::copy(entry.path(), &to)?;
            count += 1;
        }
    }
    Ok(count)
}

/// 递归统计文件数（测试与校验用）。
pub fn count_files(dir: &Path) -> u64 {
    let mut count = 0u64;
    if let Ok(it) = fs::read_dir(dir) {
        for entry in it.flatten() {
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                count += count_files(&entry.path());
            } else {
                count += 1;
            }
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "dpm-copy-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
        ));
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn fake_plugin(root: &Path) {
        let p = root.join("TrueGlow");
        fs::create_dir_all(p.join("Binaries").join("Win64")).unwrap();
        fs::create_dir_all(p.join("Content")).unwrap();
        fs::write(p.join("TrueGlow.uplugin"), "{}").unwrap();
        fs::write(p.join("Binaries").join("Win64").join("UE4Editor-TrueGlow.dll"), "dll").unwrap();
        fs::write(p.join("Content").join("splash.uasset"), "bin").unwrap();
    }

    fn engine_at(root: &Path) -> UeEngine {
        UeEngine {
            version: "5.8.1".into(),
            root: root.to_path_buf(),
            runuat: root.join("Engine").join("Build").join("BatchFiles").join("RunUAT.bat"),
        }
    }

    #[test]
    fn copies_and_reports() {
        let base = temp("ok");
        fake_plugin(&base);
        let t = install_binary(&base.join("TrueGlow"), &engine_at(&base)).unwrap();
        assert_eq!(t.engine, "UE-5.8.1");
        assert!(t.path.ends_with(r"Engine\Plugins\Marketplace\TrueGlow"), "{:?}", t.path);
        assert_eq!(t.method, InstallMethod::BinaryCopy);
        assert_eq!(
            count_files(&base.join("TrueGlow")),
            count_files(&t.path)
        );
        // 幂等重装
        install_binary(&base.join("TrueGlow"), &engine_at(&base)).unwrap();
        assert_eq!(count_files(&base.join("TrueGlow")), count_files(&t.path));
    }

    #[test]
    fn missing_uplugin_errors() {
        let base = temp("noplugin");
        fs::create_dir_all(base.join("empty")).unwrap();
        let err = install_binary(&base.join("empty"), &engine_at(&base)).unwrap_err();
        assert!(matches!(err, InstallError::NoUplugin(_)));
        assert!(err.to_string().contains("无 .uplugin"));
    }
}
