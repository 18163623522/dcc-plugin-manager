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
    pub obsidian: crate::detect::obsidian::ObsidianInfo,
}

#[tauri::command]
pub fn detect_engines() -> EnginesDto {
    let roots = crate::settings::load(&data_dir()).extra_ue_roots;
    EnginesDto {
        ue: detect_ue_engines(&roots),
        houdini: detect_houdini(),
        obsidian: crate::detect::obsidian::detect_obsidian(),
    }
}

// ————————————————— 设置与缓存（M6）—————————————————

#[tauri::command]
pub fn get_settings() -> crate::settings::Settings {
    crate::settings::load(&data_dir())
}

#[tauri::command]
pub fn save_settings(roots: Vec<String>) -> Result<(), String> {
    let mut s = crate::settings::load(&data_dir());
    s.extra_ue_roots = roots.into_iter().map(PathBuf::from).collect();
    crate::settings::save(&data_dir(), &s).map_err(|e| e.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheStats {
    pub repos: u64,
    pub releases: u64,
    pub build: u64,
}

fn dir_size_bytes(dir: &std::path::Path) -> u64 {
    let mut total = 0u64;
    if let Ok(it) = std::fs::read_dir(dir) {
        for e in it.flatten() {
            let p = e.path();
            if p.is_dir() {
                total += dir_size_bytes(&p);
            } else if let Ok(m) = e.metadata() {
                total += m.len();
            }
        }
    }
    total
}

#[tauri::command]
pub fn cache_stats() -> CacheStats {
    let cache = data_dir().join("cache");
    CacheStats {
        repos: dir_size_bytes(&cache.join("repos")),
        releases: dir_size_bytes(&cache.join("releases")),
        build: dir_size_bytes(&cache.join("build")),
    }
}

/// 清理缓存子目录（repos/releases/build 任选）。本地源目录不受影响（不在 cache 下）。
#[tauri::command]
pub fn clear_cache(repos: bool, releases: bool, build: bool) -> Result<u64, String> {
    let cache = data_dir().join("cache");
    let mut freed = 0u64;
    for (name, do_clear) in [("repos", repos), ("releases", releases), ("build", build)] {
        if do_clear {
            let dir = cache.join(name);
            freed += dir_size_bytes(&dir);
            if dir.exists() {
                std::fs::remove_dir_all(&dir).map_err(|e| format!("删除 {} 失败：{e}", dir.display()))?;
            }
        }
    }
    Ok(freed)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvStatus {
    pub gh: bool,
    pub curl: bool,
    pub pnpm: bool,
}

#[tauri::command]
pub fn env_status() -> EnvStatus {
    use std::process::Command;
    let curl_ok = || {
        crate::preflight::no_window(Command::new("curl"))
            .args(["--version"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    };
    let pnpm_ok = || {
        crate::preflight::no_window(Command::new("pnpm"))
            .args(["--version"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    };
    EnvStatus {
        gh: crate::sources::git::gh_available(),
        curl: curl_ok(),
        pnpm: pnpm_ok(),
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
        PluginSource::Git { url, .. } => (
            match e.kind {
                PluginKind::Ue => "UE",
                PluginKind::Houdini => "Houdini",
                PluginKind::Obsidian => "Obsidian",
            },
            "github",
            url.clone(),
        ),
        PluginSource::Local { path } => (
            match e.kind {
                PluginKind::Ue => "UE",
                PluginKind::Houdini => "Houdini",
                PluginKind::Obsidian => "Obsidian",
            },
            "local",
            path.to_string_lossy().into_owned(),
        ),
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
            .map(|t| {
                t.engine
                    .strip_prefix("Obsidian@")
                    .map(str::to_string)
                    .or_else(|| {
                        t.engine.split_once('-').map(|(_, v)| v.to_string())
                    })
                    .unwrap_or_else(|| t.engine.clone())
            })
            .collect(),
        status: if installed { "installed" } else { "idle" }.into(),
        origin,
        source: source.into(),
        desc: e.desc.clone().unwrap_or_else(|| {
            if source == "local" { "本地目录源".into() } else { "GitHub 源".into() }
        }),
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
        desc: inspected.desc.clone(),
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
    // 卡片说明：仓库描述优先，uplugin Description 兜底（提前取，url 随后 move 进 source）
    let desc = git::repo_description(&url).or_else(|| inspected.desc.clone());
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
        desc,
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

    let ue_list = crate::detect::ue::detect_ue_engines(&[]);
    let hou_list = crate::detect::houdini::detect_houdini();
    let obs_info = crate::detect::obsidian::detect_obsidian();
    let data = data_dir();
    let mut results = Vec::new();

    for label in &engines {
        // 按插件类型解析目标（UE 引擎 / Houdini 安装 / Obsidian vault，label=vault id）
        let pick = match entry.kind {
            PluginKind::Houdini => hou_list
                .iter()
                .find(|h| &h.version == label)
                .map(EnginePick::Hou)
                .ok_or_else(|| format!("未检测到 Houdini {label}")),
            PluginKind::Obsidian => obs_info
                .vaults
                .iter()
                .find(|v| &v.id == label)
                .map(EnginePick::Obs)
                .ok_or_else(|| format!("未检测到 vault {label}")),
            PluginKind::Ue => ue_list
                .iter()
                .find(|e| &e.version == label)
                .map(EnginePick::Ue)
                .ok_or_else(|| format!("未检测到 UE {label}")),
        };
        let pick = match pick {
            Ok(v) => v,
            Err(err) => {
                results.push(InstallResultDto {
                    engine: label.clone(),
                    ok: false,
                    error: Some(err),
                });
                continue;
            }
        };

        // 预检门禁（仅 UE）：阻塞项（不兼容 / 引擎残缺 / 文件锁）直接拒绝该引擎
        if let EnginePick::Ue(engine) = &pick {
            let compat_status = compat.as_ref().and_then(|m| m.get(label));
            let blockers: Vec<String> =
                crate::preflight::check_all(&entry, engine, compat_status, &data)
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
        }

        let result = install_one(&app, &entry, &pick, refs.as_ref(), &token, &data);
        results.push(apply_result(&mut reg, &entry.id, label, result));
    }

    let _ = save_registry(&reg);
    results
}

/// 安装目标：UE 引擎 / Houdini 安装 / Obsidian vault。
enum EnginePick<'a> {
    Ue(&'a UeEngine),
    Hou(&'a crate::detect::houdini::HoudiniInstall),
    Obs(&'a crate::detect::obsidian::ObsidianVault),
}

/// 单目标安装：Houdini → packages 落位；UE：Local → 拷贝，Git → Release 优先、构建兜底。
fn install_one(
    app: &AppHandle,
    entry: &PluginEntry,
    pick: &EnginePick<'_>,
    refs: Option<&std::collections::HashMap<String, String>>,
    token: &str,
    data_dir: &std::path::Path,
) -> Result<(InstalledTarget, Option<String>, InstallMethod), String> {
    let emit = |l: &str| {
        let _ = app.emit("install-log", l);
    };

    // Houdini：本地目录 / git 缓存仓库 → plugins 落位 + packages json
    if let EnginePick::Hou(hou) = pick {
        let src = match &entry.source {
            PluginSource::Local { path } => path.clone(),
            PluginSource::Git { url, .. } => {
                let state =
                    git::ensure_repo(url, data_dir, &mut |l| emit(l)).map_err(|e| e.to_string())?;
                state.path
            }
        };
        emit(&format!(
            "Houdini {} 安装：{} → plugins/{}",
            hou.version,
            src.display(),
            entry.id
        ));
        return crate::install::houdini::install_houdini(&src, &entry.id, hou)
            .map(|t| (t, None, InstallMethod::BinaryCopy))
            .map_err(|e| e.to_string());
    }

    // Obsidian：Release 散件 → 源码 pnpm build → 本地已构建，产物三件套落位 vault
    if let EnginePick::Obs(vault) = pick {
        use crate::install::obsidian;
        let (files_dir, method, tag) = match &entry.source {
            PluginSource::Local { path } => {
                let mut dir = path.clone();
                let mut m = InstallMethod::BinaryCopy;
                if !obsidian::has_built_files(&dir) {
                    emit("本地目录无产物，尝试 pnpm 构建");
                    obsidian::build_obsidian(&dir, &mut |l| emit(l))?;
                    m = InstallMethod::Build;
                }
                if !obsidian::has_built_files(&dir) {
                    // 构建产物不在根（自定义输出目录）→ 探测一层子目录
                    if let Some(sub) = std::fs::read_dir(&dir)
                        .ok()
                        .and_then(|it| {
                            it.flatten().find(|e| obsidian::has_built_files(&e.path()))
                        })
                    {
                        dir = sub.path();
                    }
                }
                (dir, m, None)
            }
            PluginSource::Git { url, .. } => {
                let state =
                    git::ensure_repo(url, data_dir, &mut |l| emit(l)).map_err(|e| e.to_string())?;
                let repo_name = git::repo_name(url).unwrap_or_else(|| "repo".into());
                let cache = data_dir.join("cache").join("releases").join(repo_name);
                // 1. Release 散件（Obsidian 惯例）
                match crate::install::release::install_release_files(
                    &git::repo_full_name(url).unwrap_or_default(),
                    &cache,
                    &mut |l| emit(l),
                ) {
                    Ok((tag, dir)) => (dir, InstallMethod::Release, Some(tag)),
                    Err(e) => {
                        let fallback = matches!(
                            e,
                            crate::install::release::ReleaseError::NoRelease(_)
                                | crate::install::release::ReleaseError::NoZipAsset { .. }
                        );
                        if !fallback {
                            return Err(e.to_string());
                        }
                        // 2. 源码 pnpm 构建
                        emit("无 Release 散件 → pnpm 源码构建");
                        obsidian::build_obsidian(&state.path, &mut |l| emit(l))?;
                        (state.path.clone(), InstallMethod::Build, None)
                    }
                }
            }
        };

        // minAppVersion 兼容提示（不阻塞）
        if let Some(min) = obsidian::min_app_version(&files_dir) {
            if let Some(app) = crate::detect::obsidian::app_version() {
                if !obsidian::version_lte(&min, &app) {
                    emit(&format!(
                        "⚠ manifest 要求 Obsidian ≥ {min}，本机为 {app}——装完可能无法启用"
                    ));
                }
            }
        }

        let t = obsidian::install_obsidian(&files_dir, vault, method, &mut |l| emit(l))
            .map_err(|e| e.to_string())?;
        return Ok((t, tag, method));
    }

    let EnginePick::Ue(engine) = pick else {
        unreachable!("Houdini/Obsidian 分支已提前返回");
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
