//! RunUAT BuildPlugin 构建流水线（M3）。
//!
//! `<runuat> BuildPlugin -Plugin=<uplugin> -Package=<cache>\build\<id>-<引擎>
//! -TargetPlatforms=Win64 -NoHostPlatformHeaders`
//!
//! - stdout/stderr 逐行经 `on_line` 回调（command 层转发 install-log 事件）
//! - 失败：收集 error 行返回，registry 不写（调用方职责）
//! - 取消：token 注册 PID，`cancel_build` 用 `taskkill /T /F` 杀整棵进程树
//!   （cmd → AutomationTool → UBT → MSBuild）

use crate::detect::ue::UeEngine;
use crate::preflight::no_window;
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{LazyLock, Mutex};

#[derive(Debug)]
pub enum BuildError {
    NoUplugin(PathBuf),
    Spawn(String),
    /// exit code ≠ 0：收集到的 error 行（前端高亮）
    RunUatFailed { errors: Vec<String> },
    Cancelled,
    /// 构建成功但 Package 目录里探测不到插件产物
    NoOutput(PathBuf),
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuildError::NoUplugin(p) => write!(f, "构建源目录无 .uplugin：{}", p.display()),
            BuildError::Spawn(e) => write!(f, "无法启动 RunUAT：{e}"),
            BuildError::RunUatFailed { errors } => {
                if errors.is_empty() {
                    write!(f, "RunUAT 构建失败（无 error 行，详见日志）")
                } else {
                    write!(f, "RunUAT 构建失败：{}", errors.join(" | "))
                }
            }
            BuildError::Cancelled => write!(f, "已取消（进程树已终止）"),
            BuildError::NoOutput(p) => write!(f, "构建成功但产物缺失：{}", p.display()),
        }
    }
}

impl std::error::Error for BuildError {}

/// 运行中的构建：token → 进程 PID（取消用）。
static RUNNING: LazyLock<Mutex<HashMap<String, u32>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// 取消一个构建（杀整棵进程树）。
pub fn cancel_build(token: &str) -> bool {
    let pid = RUNNING.lock().ok().and_then(|m| m.get(token).copied());
    let Some(pid) = pid else { return false };
    let o = no_window(Command::new("taskkill"))
        .args(["/T", "/F", "/PID", &pid.to_string()])
        .output();
    matches!(o, Ok(out) if out.status.success())
}

/// 构建插件源码，返回产物插件目录（`<Package>\<Name>`，含 .uplugin 与 Binaries）。
pub fn build_plugin(
    token: &str,
    plugin_root: &Path,
    engine: &UeEngine,
    data_dir: &Path,
    on_line: &mut dyn FnMut(&str),
) -> Result<PathBuf, BuildError> {
    let uplugin = find_uplugin(plugin_root).ok_or_else(|| BuildError::NoUplugin(plugin_root.to_path_buf()))?;
    let id = uplugin
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();

    let package = data_dir
        .join("cache")
        .join("build")
        .join(format!("{id}-{}", engine.version.replace('.', "_")));
    if package.exists() {
        std::fs::remove_dir_all(&package).map_err(|e| BuildError::Spawn(e.to_string()))?;
    }
    std::fs::create_dir_all(package.parent().unwrap_or(data_dir))
        .map_err(|e| BuildError::Spawn(e.to_string()))?;

    on_line(&format!(
        "RunUAT BuildPlugin：{} → UE {}",
        uplugin.display(),
        engine.version
    ));

    let mut cmd = no_window(Command::new("cmd"));
    cmd.args([
        "/C",
        &engine.runuat.to_string_lossy(),
        "BuildPlugin",
        &format!("-Plugin={}", uplugin.to_string_lossy()),
        &format!("-Package={}", package.to_string_lossy()),
        "-TargetPlatforms=Win64",
        "-NoHostPlatformHeaders",
    ])
    // 产物目录同时经环境变量传递（RunUAT 忽略它；测试假 bat 用，避开 cmd 引号地狱）
    .env("DPM_PACKAGE_DIR", &package)
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| BuildError::Spawn(e.to_string()))?;
    if let Ok(mut map) = RUNNING.lock() {
        map.insert(token.to_string(), child.id());
    }

    // 双管道逐行读（线程 → channel → 回调），error 行收集
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let mut pipes: Vec<Box<dyn std::io::Read + Send>> = Vec::new();
    if let Some(s) = child.stdout.take() {
        pipes.push(Box::new(s));
    }
    if let Some(s) = child.stderr.take() {
        pipes.push(Box::new(s));
    }
    for stream in pipes {
        let tx = tx.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stream).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
    }
    drop(tx);

    let mut errors: Vec<String> = Vec::new();
    for line in rx {
        let lower = line.to_ascii_lowercase();
        // UBT/clang "error:"、MSVC "file.cpp(98): error C2065"、链接器 "fatal error LNKxxxx"、裸 error 行
        let is_error = lower.contains("error:")
            || lower.contains("error c") // MSVC C 编译错误码
            || lower.contains("error l") // MSVC 链接器错误码 LNK
            || lower.starts_with("error ")
            || lower.contains("fatal error");
        if is_error {
            errors.push(line.clone());
            on_line(&format!("✗ {line}"));
        } else {
            on_line(&line);
        }
    }

    let status = child.wait().map_err(|e| BuildError::Spawn(e.to_string()))?;
    if let Ok(mut map) = RUNNING.lock() {
        map.remove(token);
    }

    if !status.success() {
        // 区分取消（被 taskkill 杀，exit code 1/无 error 行）与真失败
        if errors.is_empty() && status.code().is_none_or(|c| c == 1) {
            return Err(BuildError::Cancelled);
        }
        return Err(BuildError::RunUatFailed { errors });
    }
    if !errors.is_empty() {
        return Err(BuildError::RunUatFailed { errors });
    }

    // 产物探测：<Package>\<Name> 或 <Package> 本身
    let nested = package.join(&id);
    if nested.join(&format!("{id}.uplugin")).is_file() {
        return Ok(nested);
    }
    if uplugin_in_dir(&package) {
        return Ok(package);
    }
    // 宽松：任一一层子目录含 uplugin
    if let Ok(it) = std::fs::read_dir(&package) {
        for e in it.flatten() {
            if e.path().is_dir() && uplugin_in_dir(&e.path()) {
                return Ok(e.path());
            }
        }
    }
    Err(BuildError::NoOutput(package))
}

fn uplugin_in_dir(dir: &Path) -> bool {
    std::fs::read_dir(dir)
        .map(|it| {
            it.flatten().any(|e| {
                e.path().is_file()
                    && e.path()
                        .extension()
                        .is_some_and(|x| x.eq_ignore_ascii_case("uplugin"))
            })
        })
        .unwrap_or(false)
}

fn find_uplugin(dir: &Path) -> Option<PathBuf> {
    if let Ok(it) = std::fs::read_dir(dir) {
        let mut files: Vec<PathBuf> = it
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.is_file()
                    && p.extension().is_some_and(|x| x.eq_ignore_ascii_case("uplugin"))
            })
            .collect();
        files.sort();
        if let Some(p) = files.into_iter().next() {
            return Some(p);
        }
    }
    if let Ok(it) = std::fs::read_dir(dir) {
        for e in it.flatten() {
            let p = e.path();
            if p.is_dir() {
                if let Some(found) = find_uplugin(&p) {
                    return Some(found);
                }
            }
        }
    }
    None
}
