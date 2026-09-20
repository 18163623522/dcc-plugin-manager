//! Task 1.2 集成测试：用本机真实 LauncherInstalled.dat 快照验证解析与去重。

use dcc_plugin_manager_lib::detect::ue::engines_from_dat;

fn fixture_text() -> String {
    std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        r"\tests\fixtures\LauncherInstalled.dat"
    ))
    .unwrap()
}

/// 本机快照含 6 个引擎根目录；每个根目录同时存在 Marketplace 条目
/// （如 QuixelBridge_5.6 / FabPlugin_5.6，版本 5.6.0）与引擎本体条目
/// （UE_5.6 = 5.6.1），去重必须保留引擎本体的更高版本。
#[test]
fn real_snapshot_parses_and_dedupes_to_max_version() {
    let engines = engines_from_dat(&fixture_text()).unwrap();
    assert_eq!(engines.len(), 6, "快照应去重为 6 个引擎: {engines:#?}");

    let by_root = |frag: &str| {
        engines
            .iter()
            .find(|e| e.root.to_string_lossy().contains(frag))
            .unwrap_or_else(|| panic!("找不到含 {frag} 的引擎"))
    };

    assert_eq!(by_root(r"UnrealEngine\UE_5.6").version, "5.6.1");
    assert_eq!(by_root(r"UE\UE_5.4").version, "5.4.4");
    assert_eq!(by_root("UE_4.26").version, "4.26.2");
    assert_eq!(by_root("UE_5.7").version, "5.7.4");
    assert_eq!(by_root("UE_5.3").version, "5.3.2");
    assert_eq!(by_root("UE_5.8").version, "5.8.1");
}

/// RunUAT 路径形状（解析层只构造路径，不做存在性断言——存在性由
/// detect_ue_engines 过滤，见 live 检测）。
#[test]
fn runuat_path_shape() {
    let engines = engines_from_dat(&fixture_text()).unwrap();
    assert!(!engines.is_empty());
    for e in &engines {
        let s = e.runuat.to_string_lossy();
        assert!(
            s.ends_with(r"Engine\Build\BatchFiles\RunUAT.bat"),
            "RunUAT 路径形状错误: {s}"
        );
        assert!(s.starts_with(e.root.to_string_lossy().as_ref()));
    }
}

/// 版本串必须是 x.y.z 三段（补零逻辑的回归保障）。
#[test]
fn versions_are_triplets() {
    let engines = engines_from_dat(&fixture_text()).unwrap();
    for e in &engines {
        assert_eq!(e.version.split('.').count(), 3, "{}", e.version);
    }
}
