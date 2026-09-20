//! Task 3.2 集成测试：假 RunUAT（.bat）覆盖 成功产物探测 / 失败 error 行收集 / 取消杀进程树。

use dcc_plugin_manager_lib::detect::ue::UeEngine;
use dcc_plugin_manager_lib::install::build::{build_plugin, cancel_build, BuildError};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

fn temp(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "dpm-build-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos()
    ));
    fs::create_dir_all(&d).unwrap();
    d
}

fn write_bat(path: &Path, lines: &[&str]) {
    let text = lines.join("\r\n") + "\r\n";
    fs::write(path, text).unwrap();
}

/// 假 RunUAT：从 DPM_PACKAGE_DIR 环境变量取产物目录 → 建产物（FakePlug/FakePlug.uplugin）→ 成功。
const OK_BAT: &[&str] = &[
    "@echo off",
    "setlocal",
    "set \"PKG=%DPM_PACKAGE_DIR%\"",
    "if \"%PKG%\"==\"\" (",
    "  echo ERROR: no package env >&2",
    "  exit /b 9",
    ")",
    "mkdir \"%PKG%\\FakePlug\" 2>nul",
    "type nul > \"%PKG%\\FakePlug\\FakePlug.uplugin\"",
    "echo [FakeUAT] compiling module...",
    "echo [FakeUAT] Build successful",
    "exit /b 0",
];

const FAIL_BAT: &[&str] = &[
    "@echo off",
    "echo [FakeUAT] something happened",
    "echo Fatal error: fake build failure >&2",
    "exit /b 3",
];

const SLOW_BAT: &[&str] = &[
    "@echo off",
    "echo [FakeUAT] started",
    "ping -n 20 127.0.0.1 >nul",
    "exit /b 0",
];

fn engine_with(runuat: &PathBuf) -> UeEngine {
    UeEngine {
        version: "0.0.0".into(),
        root: runuat.parent().unwrap().to_path_buf(),
        runuat: runuat.clone(),
    }
}

fn fake_source(root: &Path) -> PathBuf {
    let src = root.join("FakePlug");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("FakePlug.uplugin"), "{}").unwrap();
    fs::create_dir_all(src.join("Source")).unwrap();
    fs::write(src.join("Source").join("a.cs"), "x").unwrap();
    src
}

#[test]
fn success_probes_nested_output() {
    let root = temp("ok");
    let bat = root.join("ok.bat");
    write_bat(&bat, OK_BAT);
    let src = fake_source(&root);
    let mut lines: Vec<String> = Vec::new();

    let out = build_plugin("t-ok", &src, &engine_with(&bat), &root, &mut |l| {
        lines.push(l.to_string());
    })
    .unwrap();

    assert!(out.ends_with("FakePlug"), "{:?}", out);
    assert!(out.join("FakePlug.uplugin").is_file());
    assert!(lines.iter().any(|l| l.contains("Build successful")));
    assert!(lines.iter().any(|l| l.contains("RunUAT BuildPlugin")));
}

#[test]
fn failure_collects_error_lines() {
    let root = temp("fail");
    let bat = root.join("fail.bat");
    write_bat(&bat, FAIL_BAT);
    let src = fake_source(&root);

    let err = build_plugin("t-fail", &src, &engine_with(&bat), &root, &mut |_| {}).unwrap_err();
    match err {
        BuildError::RunUatFailed { errors } => {
            assert!(errors.iter().any(|e| e.contains("fake build failure")), "{errors:?}");
        }
        other => panic!("期望 RunUatFailed，得到 {other:?}"),
    }
}

#[test]
fn cancel_kills_process_tree() {
    let root = temp("cancel");
    let bat = root.join("slow.bat");
    write_bat(&bat, SLOW_BAT);
    let src = fake_source(&root);

    let seen = Arc::new(AtomicBool::new(false));
    let seen_flag = seen.clone();
    let root2 = root.clone();
    let handle = std::thread::spawn(move || {
        let mut r = build_plugin("t-cancel", &src, &engine_with(&root2.join("slow.bat")), &root2, &mut |l| {
            if l.contains("started") {
                seen_flag.store(true, Ordering::SeqCst);
            }
        });
        if let Ok(p) = &r {
            // 慢构建被取消时不应走到成功分支；若进程树杀失败会 20 秒后成功——测试断言取消
            let _ = p;
        }
        r
    });

    // 等 started 行出现（最多 10 秒）
    for _ in 0..100 {
        if seen.load(Ordering::SeqCst) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(seen.load(Ordering::SeqCst), "假 RunUAT 未启动");

    assert!(cancel_build("t-cancel"));
    let result = handle.join().unwrap();
    match result {
        Err(BuildError::Cancelled) => {}
        other => panic!("期望 Cancelled，得到 {other:?}"),
    }
}
