//! Task 2.2 集成测试：四型仓库（真 git fixture，零网络）跑通 §3.7 四层信号。
//!
//! 四型（对应计划文档实测命名习惯）：
//! - TrueGlow 型：默认分支 + ue5.7/ue5.8 精确分支（各分支 EngineVersion 一致）
//! - WireShakeBreak 型：裸 ue5 家族分支
//! - MatHelper_UE426 型：仅默认分支，EngineVersion 唯一声明
//! - BetterHLSL 型：噪音分支（v2.0/dev）无任何版本信号

use dcc_plugin_manager_lib::compat::{compute, CompatInput, CompatStatus};
use dcc_plugin_manager_lib::detect::ue::UeEngine;
use dcc_plugin_manager_lib::sources::git::RefInfo;
use std::path::{Path, PathBuf};
use std::process::Command;

fn sh(dir: Option<&Path>, args: &[&str]) {
    let mut c = Command::new("git");
    c.args(["-c", "user.email=t@t.test", "-c", "user.name=Tester", "-c", "commit.gpgsign=false"])
        .args(args);
    if let Some(d) = dir {
        c.current_dir(d);
    }
    let o = c.output().unwrap();
    assert!(
        o.status.success(),
        "git {args:?} 失败：{}",
        String::from_utf8_lossy(&o.stderr)
    );
}

fn write_uplugin(dir: &Path, name: &str, ev: Option<&str>) {
    let ev_field = ev
        .map(|v| format!(",\"EngineVersion\":\"{v}\""))
        .unwrap_or_default();
    std::fs::create_dir_all(dir.join(name)).unwrap();
    std::fs::write(
        dir.join(name).join(format!("{name}.uplugin")),
        format!("{{\"FriendlyName\":\"{name}\"{ev_field}}}"),
    )
    .unwrap();
}

/// 建一个 fixture 仓库：默认分支 uplugin（EV 可空）+ 若干分支（EV 可空）。
fn make_repo(root: &Path, name: &str, default_ev: Option<&str>, branches: &[(&str, Option<&str>)]) -> PathBuf {
    let dir = root.join(name);
    write_uplugin(&dir, name, default_ev);
    sh(None, &["init", "-q", "-b", "main", &dir.to_string_lossy()]);
    sh(Some(&dir), &["add", "-A"]);
    sh(Some(&dir), &["commit", "-q", "-m", "init"]);
    for (br, ev) in branches {
        sh(Some(&dir), &["checkout", "-q", "-b", br]);
        write_uplugin(&dir, name, *ev);
        sh(Some(&dir), &["add", "-A"]);
        sh(Some(&dir), &["commit", "-q", "--allow-empty", "-m", br]);
    }
    sh(Some(&dir), &["checkout", "-q", "main"]);
    dir
}

/// 本地分支 → RefInfo 列表（全 Branch）。
fn local_branch_refs(dir: &Path) -> Vec<RefInfo> {
    let out = Command::new("git")
        .args(["for-each-ref", "--format=%(refname:short) %(objectname)", "refs/heads"])
        .current_dir(dir)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| {
            let (name, commit) = l.split_once(' ')?;
            Some(RefInfo {
                name: name.to_string(),
                kind: dcc_plugin_manager_lib::sources::git::RefKind::Branch,
                commit: commit[..12.min(commit.len())].to_string(),
            })
        })
        .collect()
}

fn temp_root(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "dpm-compat-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos()
    ));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn engine(version: &str) -> UeEngine {
    UeEngine {
        version: version.into(),
        root: PathBuf::from(r"Z:\fake"),
        runuat: PathBuf::from(r"Z:\fake\RunUAT.bat"),
    }
}

fn status_of(result: &[dcc_plugin_manager_lib::compat::EngineCompat], engine: &str) -> CompatStatus {
    result
        .iter()
        .find(|e| e.engine == engine)
        .unwrap_or_else(|| panic!("结果缺引擎 {engine}: {result:?}"))
        .status
        .clone()
}

#[test]
fn trueglow_type_exact_branches_confirmed() {
    let root = temp_root("tg");
    let dir = make_repo(
        &root,
        "TrueGlow",
        Some("4.26"),
        &[("ue5.7", Some("5.7")), ("ue5.8", Some("5.8"))],
    );
    let refs = local_branch_refs(&dir);
    let engines = [engine("4.26.2"), engine("5.7.4"), engine("5.8.1")];
    let out = compute(&CompatInput {
        repo_path: &dir,
        default_ref: "main",
        repo_name: "TrueGlow",
        refs: &refs,
        engines: &engines,
        installed: &[],
    });

    assert!(matches!(
        status_of(&out, "5.7.4"),
        CompatStatus::Installable { ref git_ref, confirmed: true } if git_ref == "ue5.7"
    ));
    assert!(matches!(
        status_of(&out, "5.8.1"),
        CompatStatus::Installable { ref git_ref, confirmed: true } if git_ref == "ue5.8"
    ));
    // 默认分支声明 4.26 → 同版本可装（确认）
    assert!(matches!(
        status_of(&out, "4.26.2"),
        CompatStatus::Installable { ref git_ref, confirmed: true } if git_ref == "main"
    ));
}

#[test]
fn family_branch_matches_all_minor_versions() {
    let root = temp_root("fam");
    let dir = make_repo(&root, "WireShakeBreak", None, &[("ue5", None)]);
    let refs = local_branch_refs(&dir);
    let engines = [engine("4.26.2"), engine("5.7.4")];
    let out = compute(&CompatInput {
        repo_path: &dir,
        default_ref: "main",
        repo_name: "WireShakeBreak",
        refs: &refs,
        engines: &engines,
        installed: &[],
    });

    assert!(matches!(
        status_of(&out, "5.7.4"),
        CompatStatus::Installable { ref git_ref, confirmed: false } if git_ref == "ue5"
    ));
    assert_eq!(status_of(&out, "4.26.2"), CompatStatus::Unverified);
}

#[test]
fn mat_helper_type_single_declaration() {
    let root = temp_root("mh");
    let dir = make_repo(&root, "MatHelper_UE426", Some("4.26"), &[]);
    let refs = local_branch_refs(&dir);
    let engines = [engine("4.26.2"), engine("5.7.4"), engine("5.8.1")];
    let out = compute(&CompatInput {
        repo_path: &dir,
        default_ref: "main",
        repo_name: "MatHelper_UE426",
        refs: &refs,
        engines: &engines,
        installed: &[],
    });

    assert!(matches!(
        status_of(&out, "4.26.2"),
        CompatStatus::Installable { ref git_ref, confirmed: true } if git_ref == "main"
    ));
    for e in ["5.7.4", "5.8.1"] {
        match status_of(&out, e) {
            CompatStatus::Incompatible { reason } => {
                assert!(reason.contains("4.26"), "{e} 原因应提到 4.26：{reason}")
            }
            other => panic!("{e} 应为 Incompatible，得到 {other:?}"),
        }
    }
}

#[test]
fn betterhlsl_type_noise_branches_unverified() {
    let root = temp_root("bh");
    let dir = make_repo(&root, "BetterHLSL", None, &[("v2.0", None), ("dev-material", None)]);
    let refs = local_branch_refs(&dir);
    let engines = [engine("4.26.2"), engine("5.7.4")];
    let out = compute(&CompatInput {
        repo_path: &dir,
        default_ref: "main",
        repo_name: "BetterHLSL",
        refs: &refs,
        engines: &engines,
        installed: &[],
    });

    assert_eq!(status_of(&out, "4.26.2"), CompatStatus::Unverified);
    assert_eq!(status_of(&out, "5.7.4"), CompatStatus::Unverified);
}

/// 兜底选分支：BetterHLSL 型仓库（main 无声明 + ue4.26 版本分支）→ 4.26 安装应自动选 ue4.26。
#[test]
fn pick_ref_falls_back_to_versioned_branch() {
    let root = temp_root("pick");
    let dir = make_repo(
        &root,
        "BetterHLSL",
        None,
        &[("ue4.26", Some("4.26")), ("ue5.7", Some("5.7")), ("noise", None)],
    );
    // 本地仓库伪造 remote 跟踪引用（remote_branch_names 读 refs/remotes/origin）
    for br in ["main", "ue4.26", "ue5.7", "noise"] {
        sh(
            Some(&dir),
            &[
                "update-ref",
                &format!("refs/remotes/origin/{br}"),
                &format!("refs/heads/{br}"),
            ],
        );
    }

    use dcc_plugin_manager_lib::commands::pick_ref_for_engine;
    assert_eq!(pick_ref_for_engine(&dir, "4.26.2").as_deref(), Some("ue4.26"));
    assert_eq!(pick_ref_for_engine(&dir, "5.7.4").as_deref(), Some("ue5.7"));
    assert_eq!(pick_ref_for_engine(&dir, "5.8.1"), None, "无匹配分支应返回 None（走默认分支）");
}

#[test]
fn installed_record_wins_over_all_signals() {    let root = temp_root("inst");
    let dir = make_repo(&root, "TrueGlow", Some("4.26"), &[("ue5.7", Some("5.7"))]);
    let refs = local_branch_refs(&dir);
    let engines = [engine("5.7.4")];
    let out = compute(&CompatInput {
        repo_path: &dir,
        default_ref: "main",
        repo_name: "TrueGlow",
        refs: &refs,
        engines: &engines,
        installed: &["5.7.4".to_string()],
    });
    assert_eq!(status_of(&out, "5.7.4"), CompatStatus::Installed);
}

#[test]
fn mismatched_ref_engineversion_gets_refuted() {
    // 分支叫 ue5.8 但 uplugin 声明 4.26（错配）→ 该 ref 不算数；
    // 默认分支唯一声明 4.26 → 5.8 不兼容
    let root = temp_root("mm");
    let dir = make_repo(&root, "Liar", Some("4.26"), &[("ue5.8", Some("4.26"))]);
    let refs = local_branch_refs(&dir);
    let engines = [engine("4.26.2"), engine("5.8.1")];
    let out = compute(&CompatInput {
        repo_path: &dir,
        default_ref: "main",
        repo_name: "Liar",
        refs: &refs,
        engines: &engines,
        installed: &[],
    });
    assert!(matches!(status_of(&out, "5.8.1"), CompatStatus::Incompatible { .. }));
    assert!(matches!(
        status_of(&out, "4.26.2"),
        CompatStatus::Installable { confirmed: true, .. }
    ));
}
