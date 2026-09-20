//! Tauri command 层：薄转发 + DTO（业务全在模块里，这里只做编排）。

use crate::detect::houdini::{detect_houdini, HoudiniInstall};
use crate::detect::ue::{detect_ue_engines, UeEngine};
use crate::registry::{
    self, InstalledTarget, PluginEntry, PluginKind, PluginSource, Registry,
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

// ————————————————— 兼容矩阵（M2）—————————————————

#[tauri::command]
pub fn compat_for(app: AppHandle, id: String) -> Result<Vec<crate::compat::EngineCompat>, String> {
    let reg = load_registry();
    let Some(entry) = reg.plugins.iter().find(|p| p.id == id) else {
        return Err(format!("条目不存在：{id}"));
    };
    let PluginSource::Git { url, .. } = &entry.source else {
        return Ok(vec![]); // 本地源无矩阵（用户自行确认版本）
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
    }))
}

// ————————————————— 安装（本地源拷贝；git 源 Release 优先）—————————————————

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallResultDto {
    pub engine: String,
    pub ok: bool,
    pub error: Option<String>,
}

#[tauri::command]
pub fn install_local(app: AppHandle, id: String, engines: Vec<String>) -> Vec<InstallResultDto> {
    let mut reg = load_registry();
    let Some(entry) = reg.plugins.iter().find(|p| p.id == id).cloned() else {
        return vec![InstallResultDto {
            engine: "*".into(),
            ok: false,
            error: Some(format!("条目不存在：{id}")),
        }];
    };

    let detected = crate::detect::ue::detect_ue_engines(&[]);
    let mut results = Vec::new();

    match entry.source.clone() {
        PluginSource::Local { path } => {
            for label in &engines {
                let result = detected
                    .iter()
                    .find(|e| &e.version == label)
                    .ok_or_else(|| format!("未检测到 UE {label}"))
                    .and_then(|engine| {
                        crate::install::copy::install_binary(&path, engine).map(|t| (t, None)).map_err(|e| e.to_string())
                    });
                results.push(apply_result(&mut reg, &entry.id, label, result));
            }
        }
        PluginSource::Git { url, .. } => {
            // Release 附件优先；无 Release/无 zip → 报错引导 M3 构建
            let data = data_dir();
            for label in &engines {
                let result = detected
                    .iter()
                    .find(|e| &e.version == label)
                    .ok_or_else(|| format!("未检测到 UE {label}"))
                    .and_then(|engine| {
                        crate::install::release::install_release(&url, engine, &data, &mut |l| {
                            let _ = app.emit("install-log", l);
                        })
                        .map(|(t, tag)| (t, Some(tag)))
                        .map_err(|e| e.to_string())
                    });
                results.push(apply_result(&mut reg, &entry.id, label, result));
            }
        }
    }

    let _ = save_registry(&reg);
    results
}

/// 单引擎安装结果落 registry（Release 安装附带 tag 版本）并返回 DTO。
fn apply_result(
    reg: &mut Registry,
    id: &str,
    label: &str,
    result: Result<(InstalledTarget, Option<String>), String>,
) -> InstallResultDto {
    match result {
        Ok((target, tag)) => {
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
