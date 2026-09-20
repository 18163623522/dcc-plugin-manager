//! 一次性预置：把用户既有插件（GitHub 仓库 + 本地目录）批量登记进真实 registry。
//! 运行：cargo test --test seed -- --ignored --nocapture
//! 语义与 add_git_source / add_local_source 完全一致（含已装记录保留）。

use dcc_plugin_manager_lib::registry::{self, PluginEntry, PluginSource};
use dcc_plugin_manager_lib::sources::git::ensure_repo;
use dcc_plugin_manager_lib::sources::local::inspect_local;
use dcc_plugin_manager_lib::update::local_digest;
use std::path::{Path, PathBuf};

const GIT_SOURCES: &[&str] = &[
    "https://github.com/18163623522/TrueGlow",
    "https://github.com/18163623522/houdini-graph-tools",
    "https://github.com/18163623522/NiagaraAttributeInspector",
    "https://github.com/18163623522/WireShakeBreak",
    "https://github.com/18163623522/MaterialWireStyle",
    "https://github.com/18163623522/NamedRerouteBackport",
    "https://github.com/18163623522/SimInspector",
    "https://github.com/18163623522/BetterHLSL",
    "https://github.com/18163623522/BetterMaterialWires",
    "https://github.com/18163623522/ue426-advanced-comments",
];

const LOCAL_SOURCES: &[&str] = &[
    r"D:\001_Archive\AI\MaterialLayoutPro",
    r"D:\001_Archive\AI\NodeFitComments",
    r"D:\001_Archive\AI\AssetLinkInspector",
    r"D:\001_Archive\AI\UEPlugin-ShortcutAsset-426",
];

#[test]
#[ignore = "写入真实 registry（%APPDATA%）的一次性预置"]
fn seed_user_plugins() {
    let data = PathBuf::from(
        std::env::var("APPDATA").expect("APPDATA"),
    )
    .join("dcc-plugin-manager");
    let mut reg = registry::load(&data);
    let before = reg.plugins.len();
    let (mut ok, mut fail) = (0, 0);

    for url in GIT_SOURCES {
        match seed_git(&mut reg, url, &data) {
            Ok(id) => {
                println!("✓ [git] {id}");
                ok += 1;
            }
            Err(e) => {
                println!("✗ [git] {url}: {e}");
                fail += 1;
            }
        }
    }
    for path in LOCAL_SOURCES {
        match seed_local(&mut reg, Path::new(path)) {
            Ok(id) => {
                println!("✓ [local] {id}");
                ok += 1;
            }
            Err(e) => {
                println!("✗ [local] {path}: {e}");
                fail += 1;
            }
        }
    }

    registry::save(&data, &reg).expect("registry 写入失败");
    println!(
        "完成：{ok} 成功 / {fail} 失败（registry {} → {} 条）",
        before,
        reg.plugins.len()
    );
}

fn seed_git(reg: &mut registry::Registry, url: &str, data: &Path) -> Result<String, String> {
    let url = url.trim().to_string();
    let state = ensure_repo(&url, data, &mut |l| println!("    {l}")).map_err(|e| e.to_string())?;
    let inspected = inspect_local(&state.path).map_err(|e| e.to_string())?;
    let installed = reg.get(&inspected.id).map(|e| e.installed.clone()).unwrap_or_default();
    let id = inspected.id.clone();
    reg.upsert(PluginEntry {
        id,
        kind: inspected.kind,
        source: PluginSource::Git {
            url,
            default_ref: Some(state.default_ref.clone()),
        },
        version: inspected.version,
        commit: Some(state.head.clone()),
        local_digest: None,
        installed,
    });
    Ok(inspected.id)
}

fn seed_local(reg: &mut registry::Registry, path: &Path) -> Result<String, String> {
    let inspected = inspect_local(path).map_err(|e| e.to_string())?;
    let installed = reg.get(&inspected.id).map(|e| e.installed.clone()).unwrap_or_default();
    let id = inspected.id.clone();
    reg.upsert(PluginEntry {
        id,
        kind: inspected.kind,
        source: PluginSource::Local {
            path: inspected.plugin_root.clone(),
        },
        version: inspected.version,
        commit: None,
        local_digest: Some(local_digest(&inspected.plugin_root).to_string()),
        installed,
    });
    Ok(inspected.id)
}
