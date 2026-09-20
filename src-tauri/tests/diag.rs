//! 诊断：完整复刻 install_one 的 git-UE 安装路径，逐步打印，定位用户实测失败点。
//! cargo test --test diag -- --ignored --nocapture

use dcc_plugin_manager_lib::detect::ue::{detect_ue_engines, UeEngine};
use dcc_plugin_manager_lib::install::build::build_plugin;
use dcc_plugin_manager_lib::install::copy::install_binary;
use dcc_plugin_manager_lib::install::release::install_release;
use dcc_plugin_manager_lib::sources::git::{checkout, ensure_repo};
use std::path::PathBuf;

#[test]
#[ignore = "真机完整安装诊断（可能触发真实 RunUAT 构建）"]
fn diag_git_ue_install() {
    let url = std::env::var("DIAG_URL")
        .unwrap_or_else(|_| "https://github.com/18163623522/BetterHLSL".into());
    let ver_prefix = std::env::var("DIAG_ENGINE").unwrap_or_else(|_| "4.26".into());
    let data = PathBuf::from(
        std::env::var("APPDATA").expect("APPDATA"),
    )
    .join("dcc-plugin-manager");

    println!("== 1. 引擎检测（自定义根目录 = settings） ==");
    let roots = dcc_plugin_manager_lib::settings::load(&data).extra_ue_roots;
    println!("extra roots: {roots:?}");
    let engines = detect_ue_engines(&roots);
    for e in &engines {
        println!("  {} → {}", e.version, e.root.display());
    }
    let engine: &UeEngine = engines
        .iter()
        .find(|e| e.version.starts_with(&ver_prefix))
        .unwrap_or_else(|| panic!("未找到 {ver_prefix}"));

    println!("== 2. ensure_repo ==");
    let state = ensure_repo(&url, &data, &mut |l| println!("  [git] {l}")).unwrap();
    println!("  cache: {}", state.path.display());
    println!("== 3. checkout 指定分支（DIAG_REF，缺省默认分支） ==");
    let git_ref = std::env::var("DIAG_REF").unwrap_or_else(|_| state.default_ref.clone());
    checkout(&state.path, &git_ref).unwrap();
    println!("  分支：{git_ref}");

    println!("== 4. Release 尝试 ==");
    match install_release(&url, engine, &data, &mut |l| println!("  [rel] {l}")) {
        Ok((t, tag)) => {
            println!("  RELEASE 成功 tag={tag} → {}", t.path.display());
            return;
        }
        Err(e) => println!("  [rel] 走构建兜底：{e}"),
    }

    println!("== 5. RunUAT 构建 ==");
    match build_plugin("diag-token", &state.path, engine, &data, &mut |l| {
        if l.starts_with('✗') || l.contains("RunUAT") || l.to_ascii_lowercase().contains("error") {
            println!("  [uat] {l}");
        }
    }) {
        Ok(built) => {
            println!("  构建产物 → {}", built.display());
            println!("== 6. 落位 ==");
            match install_binary(&built, engine) {
                Ok(t) => println!("  安装成功 → {}", t.path.display()),
                Err(e) => println!("  落位失败：{e}"),
            }
        }
        Err(e) => println!("  构建失败：{e}"),
    }
}
