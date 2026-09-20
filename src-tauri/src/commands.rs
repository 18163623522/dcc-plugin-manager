//! Tauri command 层：薄转发 + DTO（业务全在模块里，这里只做编排）。

use crate::detect::houdini::{detect_houdini, HoudiniInstall};
use crate::detect::ue::{detect_ue_engines, UeEngine};
use crate::registry::{
    self, InstallMethod, InstalledTarget, PluginEntry, PluginKind, PluginSource, Registry,
};
use crate::sources::git;
use crate::sources::local::inspect_local;
use crate::uninstall::{plan_targets, uninstall as run_uninstall, RemovedTarget, UninstallReport};
use serde::Serialize;
use std::path::PathBuf;
use tauri::{AppHandle, Emitter};

/// 注册表目录：`%APPDATA%\dcc-plugin-manager`（设计 §3.3）。
fn data_dir() -> PathBuf {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("dcc-plugin-manager")
}

fn load_registry() -> Registry {
    registry::load(&data_dir())
}

fn save_registry(reg: &Registry) -> Result<(), String> {
    registry::save(&data_dir(), reg).map_err(|e| format!("注册表写入失败：{e}"))
}

// ————————————————— 引擎检测 —————————————————

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnginesDto {
    pub ue: Vec<UeEngine>,
    pub houdini: Vec<HoudiniInstall>,
}

#[tauri::command]
pub fn detect_engines() -> EnginesDto {
    EnginesDto {
        ue: detect_ue_engines(&[]),
        houdini: detect_houdini(),
    }
}

// ————————————————— 插件列表 —————————————————

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginDto {
    pub id: String,
    pub name: String,
    pub host: String,
    pub version: String,
    pub latest: String,
    /// 引擎短标签（"5.8.1" / "22.0"）
    pub engines: Vec<String>,
    pub status: String,
    pub origin: String,
    pub source: String,
    pub desc: String,
}

fn to_dto(e: &PluginEntry) -> PluginDto {
    let (host, source, origin) = match &e.source {
        PluginSource::Git { url, .. } => ("UE", "github", url.clone()),
        PluginSource::Local { path } => (match e.kind { PluginKind::Ue => "UE", PluginKind::Houdini => "Houdini" }, "local", path.to_string_lossy().into_owned()),
    };
    // M1 无更新检查器：latest = version；状态= 已装/未装（可更新/失败 M2+）
    let installed = !e.installed.is_empty();
    PluginDto {
        id: e.id.clone(),
        name: e.id.clone(),
        host: host.into(),
        version: e.version.clone(),
        latest: e.version.clone(),
        engines: e
            .installed
            .iter()
            .map(|t| t.engine.split_once('-').map(|(_, v)| v.to_string()).unwrap_or_else(|| t.engine.clone()))
            .collect(),
        status: if installed { "installed" } else { "idle" }.into(),
        origin,
        source: source.into(),
        desc: if source == "local" { "本地目录源".into() } else { "GitHub 源".into() },
    }
}

#[tauri::command]
pub fn list_plugins() -> Vec<PluginDto> {
    load_registry().plugins.iter().map(to_dto).collect()
}

// ————————————————— 添加本地源 —————————————————

#[tauri::command]
pub fn add_local_source(path: String) -> Result<PluginDto, String> {
    let inspected = inspect_local(std::path::Path::new(&path)).map_err(|e| e.to_string())?;
    let id = inspected.id.clone();

    let mut reg = load_registry();
    // 重加同 id：保留已安装记录，版本取最新识别结果
    let installed = reg.get(&id).map(|e| e.installed.clone()).unwrap_or_default();
    reg.upsert(PluginEntry {
        id,
        kind: inspected.kind,
        source: PluginSource::Local {
            path: inspected.plugin_root.clone(),
        },
        version: inspected.version,
        commit: None,
        local_digest: Some(crate::update::local_digest(&inspected.plugin_root).to_string()),
        installed,
    });
    save_registry(&reg)?;
    Ok(to_dto(reg.get(&inspected.id).expect("刚 upsert")))
}

// ————————————————— 添加 GitHub 源（M2）—————————————————

/// clone/pull 缓存仓库 → 识别 → 登记。过程日志经 `install-log` 事件流式到 UI。
#[tauri::command]
pub fn add_git_source(app: AppHandle, url: String) -> Result<PluginDto, String> {
    let url = url.trim().to_string();
    let dir = data_dir();

    let state = git::ensure_repo(&url, &dir, &mut |line| {
        let _ = app.emit("install-log", line);
    })
    .map_err(|e| e.to_string())?;

    let inspected =
        inspect_local(&state.path).map_err(|e| format!("{e}（clone 成功但未识别为插件仓库）"))?;

    let id = inspected.id.clone();
    let mut reg = load_registry();
    let installed = reg.get(&id).map(|e| e.installed.clone()).unwrap_or_default();
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
    save_registry(&reg)?;
    Ok(to_dto(reg.get(&inspected.id).expect("刚 upsert")))
}

/// git 源的引用枚举（compat 徽标用）。
#[tauri::command]
pub fn list_git_refs(url: String) -> Result<Vec<git::RefInfo>, String> {
    git::list_refs(&url).map_err(|e| e.to_string())
}

// ————————————————— 更新检查（M2）—————————————————

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateDto {
    pub id: String,
    pub state: crate::update::UpdateState,
}

/// 全量检查更新（git 源联网、本地源读盘）。前端列表出黄点用。
#[tauri::command]
pub fn check_updates() -> Vec<UpdateDto> {
    let reg = load_registry();
    let dir = data_dir();
    reg.plugins
        .iter()
        .map(|e| UpdateDto {
            id: e.id.clone(),
            state: crate::update::check(e, &dir),
        })
        .collect()
}

// ————————————————— 兼容矩阵 + 预检（M2/M3）—————————————————

/// git 源 → engine → CompatStatus（compat_for 与 preflight_for 共用；本地源空表）。
fn compat_map(
    app: &AppHandle,
    entry: &PluginEntry,
) -> Result<std::collections::HashMap<String, crate::compat::CompatStatus>, String> {
    let PluginSource::Git { url, .. } = &entry.source else {
        return Ok(std::collections::HashMap::new());
    };
    let dir = data_dir();
    // 确保缓存仓库在（add 时已 clone；被清缓存也能自愈）
    let state = git::ensure_repo(url, &dir, &mut |l| {
        let _ = app.emit("install-log", l);
    })
    .map_err(|e| e.to_string())?;

    let refs = git::list_refs(url).map_err(|e| e.to_string())?;
    let engines = crate::detect::ue::detect_ue_engines(&[]);
    let installed: Vec<String> = entry
        .installed
        .iter()
        .map(|t| t.engine.split_once('-').map(|(_, v)| v.to_string()).unwrap_or_else(|| t.engine.clone()))
        .collect();

    Ok(crate::compat::compute(&crate::compat::CompatInput {
        repo_path: &state.path,
        default_ref: &state.default_ref,
        repo_name: &git::repo_name(url).unwrap_or_default(),
        refs: &refs,
        engines: &engines,
        installed: &installed,
    })
    .into_iter()
    .map(|ec| (ec.engine, ec.status))
    .collect())
}

#[tauri::command]
pub fn compat_for(app: AppHandle, id: String) -> Result<Vec<crate::compat::EngineCompat>, String> {
    let reg = load_registry();
    let Some(entry) = reg.plugins.iter().find(|p| p.id == id) else {
        return Err(format!("条目不存在：{id}"));
    };
    let map = compat_map(&app, entry)?;
    Ok(map.into_iter().map(|(engine, status)| crate::compat::EngineCompat { engine, status }).collect())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnginePreflight {
    pub engine: String,
    pub items: Vec<crate::preflight::PreflightItem>,
}

/// 每个引擎的五项预检（安装对话框徽标/tooltip 用）。
#[tauri::command]
pub fn preflight_for(app: AppHandle, id: String) -> Result<Vec<EnginePreflight>, String> {
    let reg = load_registry();
    let Some(entry) = reg.plugins.iter().find(|p| p.id == id) else {
        return Err(format!("条目不存在：{id}"));
    };
    let compat = compat_map(&app, entry)?;
    let engines = crate::detect::ue::detect_ue_engines(&[]);
    let data = data_dir();
    Ok(engines
        .iter()
        .map(|e| EnginePreflight {
            engine: e.version.clone(),
            items: crate::preflight::check_all(entry, e, compat.get(&e.version), &data),
        })
        .collect())
}

// ————————————————— 安装（本地拷贝 / git：Release 优先 → 源码构建兜底）—————————————————

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallResultDto {
    pub engine: String,
    pub ok: bool,
    pub error: Option<String>,
}

/// 安装：`refs`/`compat` 来自前端 compat_for 的结果；`token` 用于取消构建。
#[tauri::command]
pub fn install_local(
    app: AppHandle,
    id: String,
    engines: Vec<String>,
    refs: Option<std::collections::HashMap<String, String>>,
    compat: Option<std::collections::HashMap<String, crate::compat::CompatStatus>>,
    token: String,
) -> Vec<InstallResultDto> {
    let mut reg = load_registry();
    let Some(entry) = reg.plugins.iter().find(|p| p.id == id).cloned() else {
        return vec![InstallResultDto {
            engine: "*".into(),
            ok: false,
            error: Some(format!("条目不存在：{id}")),
        }];
    };

    let detected = crate::detect::ue::detect_ue_engines(&[]);
    let data = data_dir();
    let mut results = Vec::new();

    for label in &engines {
        let Some(engine) = detected.iter().find(|e| &e.version == label) else {
            results.push(InstallResultDto {
                engine: label.clone(),
                ok: false,
                error: Some(format!("未检测到 UE {label}")),
            });
            continue;
        };

        // 预检门禁：阻塞项（不兼容 / 引擎残缺 / 文件锁）直接拒绝该引擎
        let compat_status = compat.as_ref().and_then(|m| m.get(label));
        let blockers: Vec<String> = crate::preflight::check_all(&entry, engine, compat_status, &data)
            .into_iter()
            .filter(|i| i.blocking && !i.ok)
            .map(|i| i.message)
            .collect();
        if !blockers.is_empty() {
            results.push(InstallResultDto {
                engine: label.clone(),
                ok: false,
                error: Some(format!("预检未通过：{}", blockers.join("；"))),
            });
            continue;
        }

        let result = install_one(&app, &entry, engine, refs.as_ref(), &token, &data);
        results.push(apply_result(&mut reg, &entry.id, label, result));
    }

    let _ = save_registry(&reg);
    results
}

/// 单引擎安装：Local → 拷贝；Git → Release 优先，NoRelease/NoZipAsset → 源码构建兜底。
fn install_one(
    app: &AppHandle,
    entry: &PluginEntry,
    engine: &UeEngine,
    refs: Option<&std::collections::HashMap<String, String>>,
    token: &str,
    data_dir: &std::path::Path,
) -> Result<(InstalledTarget, Option<String>, InstallMethod), String> {
    let emit = |l: &str| {
        let _ = app.emit("install-log", l);
    };
    match &entry.source {
        PluginSource::Local { path } => {
            crate::install::copy::install_binary(path, engine)
                .map(|t| (t, None, InstallMethod::BinaryCopy))
                .map_err(|e| e.to_string())
        }
        PluginSource::Git { url, default_ref } => {
            // 1. 确保缓存并 checkout 兼容分支（compat 给的 ref，缺省默认分支）
            let state =
                git::ensure_repo(url, data_dir, &mut |l| emit(l)).map_err(|e| e.to_string())?;
            let git_ref = refs
                .and_then(|m| m.get(&engine.version))
                .cloned()
                .or_else(|| default_ref.clone())
                .unwrap_or_else(|| state.default_ref.clone());
            git::checkout(&state.path, &git_ref)
                .map_err(|e| format!("checkout {git_ref} 失败：{e}"))?;
            emit(&format!("构建分支：{git_ref}"));

            // 2. Release 附件优先
            match crate::install::release::install_release(url, engine, data_dir, &mut |l| emit(l)) {
                Ok((target, tag)) => Ok((target, Some(tag), InstallMethod::Release)),
                Err(rel_err) => {
                    let fallback_build = matches!(
                        rel_err,
                        crate::install::release::ReleaseError::NoRelease(_) | crate::install::release::ReleaseError::NoZipAsset { .. }
                    );
                    if !fallback_build {
                        return Err(rel_err.to_string());
                    }
                    // 3. 无 Release → 源码构建（需 VS 工具链）
                    if !crate::preflight::vs_toolchain().ok {
                        return Err(format!(
                            "无 Release 附件且 VS C++ 工具链不可用——无法源码构建（{rel_err}）"
                        ));
                    }
                    let built = crate::install::build::build_plugin(token, &state.path, engine, data_dir, &mut |l| emit(l))
                        .map_err(|e| e.to_string())?;
                    crate::install::copy::install_binary(&built, engine)
                        .map(|t| (t, None, InstallMethod::Build))
                        .map_err(|e| e.to_string())
                }
            }
        }
    }
}

/// 取消进行中的构建（杀进程树）。
#[tauri::command]
pub fn cancel_build(token: String) -> bool {
    crate::install::build::cancel_build(&token)
}

/// 单引擎安装结果落 registry（Release 安装附带 tag 版本）并返回 DTO。
fn apply_result(
    reg: &mut Registry,
    id: &str,
    label: &str,
    result: Result<(InstalledTarget, Option<String>, InstallMethod), String>,
) -> InstallResultDto {
    match result {
        Ok((mut target, tag, method)) => {
            target.method = method;
            if let Some(e) = reg.plugins.iter_mut().find(|p| p.id == id) {
                if let Some(tag) = tag.as_deref() {
                    let v = tag.trim_start_matches(['v', 'V']);
                    if !v.is_empty() {
                        e.version = v.to_string();
                    }
                }
                e.installed.retain(|t| t.engine != target.engine);
                e.installed.push(target);
            }
            InstallResultDto { engine: label.to_string(), ok: true, error: None }
        }
        Err(err) => InstallResultDto { engine: label.to_string(), ok: false, error: Some(err) },
    }
}

// ————————————————— 卸载 —————————————————

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PathDto {
    pub engine: String,
    pub path: String,
}

impl From<RemovedTarget> for PathDto {
    fn from(t: RemovedTarget) -> Self {
        PathDto { engine: t.engine, path: t.path.to_string_lossy().into_owned() }
    }
}

#[tauri::command]
pub fn plan_uninstall(id: String, engines: Vec<String>) -> Vec<PathDto> {
    let reg = load_registry();
    plan_targets(&reg, &id, &engines)
        .into_iter()
        .map(Into::into)
        .collect()
}

#[tauri::command]
pub fn do_uninstall(id: String, engines: Vec<String>) -> UninstallReport {
    let mut reg = load_registry();
    let report = run_uninstall(&mut reg, &id, &engines, true);
    let _ = save_registry(&reg);
    report
}
