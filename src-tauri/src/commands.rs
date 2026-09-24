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
    crate::preflight::dir_size(dir)
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
pub async fn add_git_source(app: AppHandle, url: String) -> Result<PluginDto, String> {
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

/// git 源的引用枚举已内联进 compat_map（前端不直接消费，命令移除）。

// ————————————————— 更新检查（M2）—————————————————

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateDto {
    pub id: String,
    pub state: crate::update::UpdateState,
}

/// 全量检查更新（git 源联网、本地源读盘）。前端列表出黄点用。
#[tauri::command]
pub async fn check_updates() -> Vec<UpdateDto> {
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

/// git 源 → engine → CompatStatus（compat_for / preflight_for / compat_all 共用；
/// 本地源空表）。阻塞 IO（pull + ls-remote），异步命令里跑。
fn compat_map(
    entry: &PluginEntry,
) -> Result<std::collections::HashMap<String, crate::compat::CompatStatus>, String> {
    let PluginSource::Git { url, .. } = &entry.source else {
        return Ok(std::collections::HashMap::new());
    };
    let dir = data_dir();
    // 确保缓存仓库在（add 时已 clone；被清缓存也能自愈）
    let state = git::ensure_repo(url, &dir, &mut |_| {}).map_err(|e| e.to_string())?;

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
pub async fn compat_for(id: String) -> Result<Vec<crate::compat::EngineCompat>, String> {
    let reg = load_registry();
    let Some(entry) = reg.plugins.iter().find(|p| p.id == id) else {
        return Err(format!("条目不存在：{id}"));
    };
    let map = compat_map(entry)?;
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
pub async fn preflight_for(id: String) -> Result<Vec<EnginePreflight>, String> {
    let reg = load_registry();
    let Some(entry) = reg.plugins.iter().find(|p| p.id == id) else {
        return Err(format!("条目不存在：{id}"));
    };
    let compat = compat_map(entry)?;
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompatAllDto {
    pub id: String,
    pub compat: Vec<crate::compat::EngineCompat>,
}

/// 全量兼容矩阵（列表卡片徽标条用）：git 源 UE 插件逐个计算，
/// spawn_blocking 后台线程跑——ls-remote × N 秒级耗时也不卡 UI。
#[tauri::command]
pub async fn compat_all() -> Vec<CompatAllDto> {
    tauri::async_runtime::spawn_blocking(|| {
        let reg = load_registry();
        reg.plugins
            .iter()
            .filter(|p| p.kind == PluginKind::Ue && matches!(p.source, PluginSource::Git { .. }))
            .map(|entry| {
                let compat = compat_map(entry)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|(engine, status)| crate::compat::EngineCompat { engine, status })
                    .collect();
                CompatAllDto { id: entry.id.clone(), compat }
            })
            .collect()
    })
    .await
    .unwrap_or_default()
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
pub async fn install_local(
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
    let mut release_applied = false;

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
        let (dto, tag_applied) = apply_result(&mut reg, &entry.id, label, result);
        release_applied |= tag_applied;
        results.push(dto);
    }

    sync_entry_identity(&mut reg, &entry.id, release_applied, &data);
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
            // 1. 确保缓存并选定分支：compat 给的 ref → 后端自动挑版本分支 → 默认分支
            let state =
                git::ensure_repo(url, data_dir, &mut |l| emit(l)).map_err(|e| e.to_string())?;
            let mut picked = refs.and_then(|m| m.get(&engine.version)).cloned();
            let mut auto = false;
            if picked.is_none() {
                if let Some(pick) = pick_ref_for_engine(&state.path, &engine.version) {
                    auto = true;
                    picked = Some(pick);
                }
            }
            let git_ref = picked
                .or_else(|| default_ref.clone())
                .unwrap_or_else(|| state.default_ref.clone());
            if auto {
                emit(&format!("自动选择版本分支：{git_ref}（兼容矩阵未提供，按分支名匹配）"));
            }
            git::checkout(&state.path, &git_ref)
                .map_err(|e| format!("checkout {git_ref} 失败：{e}"))?;

            // 2. Release 附件优先（与所选分支可能不一致——附版本来源提示）
            match crate::install::release::install_release(url, engine, data_dir, &mut |l| emit(l)) {
                Ok((target, tag)) => {
                    if git_ref != state.default_ref {
                        emit(&format!(
                            "⚠ 安装的是 Release {tag}，未按所选分支 {git_ref} 构建（Release 优先）"
                        ));
                    }
                    Ok((target, Some(tag), InstallMethod::Release))
                }
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
                    emit(&format!("构建分支：{git_ref}"));
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

/// 兜底选分支：refs 没给该引擎时，扫本地缓存仓库的远端分支名，
/// 精确匹配引擎 major.minor（ue5.7 / ue4.26），分支 uplugin 的 EngineVersion
/// 一致者优先。避免盲编默认分支（BetterHLSL main 是 UE5 代码的实测教训）。
pub fn pick_ref_for_engine(repo: &std::path::Path, engine_version: &str) -> Option<String> {
    use crate::compat::{ev_major_minor, parse_ref_ue_version, uplugin_engine_version, RefVersion};

    let target = ev_major_minor(engine_version)?;
    let mut best: Option<(bool, String)> = None; // (EV 确认, 分支名)
    for name in git::remote_branch_names(repo) {
        let Some(RefVersion::Exact { major, minor }) = parse_ref_ue_version(&name) else {
            continue;
        };
        if (major, minor) != target {
            continue;
        }
        let confirmed = git::read_uplugin_at(repo, &name)
            .ok()
            .and_then(|(_, text)| uplugin_engine_version(&text))
            .and_then(|ev| ev_major_minor(&ev))
            .is_some_and(|em| em == target);
        let better = match &best {
            None => true,
            Some((had_confirm, _)) => confirmed && !*had_confirm,
        };
        if better {
            best = Some((confirmed, name));
        }
    }
    best.map(|(_, name)| name)
}

/// 单引擎安装结果落 registry（Release 安装附带 tag 版本）；返回 (DTO, 是否走了 Release 附件)。
fn apply_result(
    reg: &mut Registry,
    id: &str,
    label: &str,
    result: Result<(InstalledTarget, Option<String>, InstallMethod), String>,
) -> (InstallResultDto, bool) {
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
            (
                InstallResultDto { engine: label.to_string(), ok: true, error: None },
                tag.is_some(),
            )
        }
        Err(err) => (
            InstallResultDto { engine: label.to_string(), ok: false, error: Some(err) },
            false,
        ),
    }
}

/// 安装后同步登记身份（commit / 版本 / 本地摘要基线）到实际装到的内容。
/// 不做这步，更新检查会拿旧基线比较——commit 通道与本地源刚装完就报"可更新"。
fn sync_entry_identity(
    reg: &mut Registry,
    id: &str,
    release_applied: bool,
    data_dir: &std::path::Path,
) {
    let Some(e) = reg.plugins.iter_mut().find(|p| p.id == id) else { return };
    let source = e.source.clone();
    match source {
        PluginSource::Git { url, .. } => {
            // 缓存仓库停在最后安装的分支/标签上；更新检查的 commit 通道读的正是
            // 这个位置（ensure_repo pull 当前分支 → head），登记同一基线才能正确比对
            if let Ok(path) = git::repo_cache_path(data_dir, &url) {
                if let Ok(st) = git::repo_state(&path) {
                    e.commit = Some(st.head);
                }
                // Release 版本号已由 apply_result 落（tag 权威）；非 Release 跟随源码 uplugin
                if !release_applied {
                    if let Ok(inspected) = inspect_local(&path) {
                        e.version = inspected.version;
                    }
                }
            }
        }
        PluginSource::Local { path } => {
            // 摘要基线对齐本次安装内容，否则本地源黄点永不消
            e.local_digest = Some(crate::update::local_digest(&path).to_string());
            if let Ok(inspected) = inspect_local(&path) {
                e.version = inspected.version;
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "dpm-sync-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// 本地源安装后：摘要基线 + 版本号跟随源（更新黄点闭环的前提）。
    #[test]
    fn sync_local_refreshes_digest_and_version() {
        let root = temp_root("local");
        std::fs::write(
            root.join("P.uplugin"),
            "{\"Name\":\"P\",\"VersionName\":\"1.2.3\"}",
        )
        .unwrap();

        let mut reg = Registry::default();
        reg.upsert(PluginEntry {
            id: "P".into(),
            kind: PluginKind::Ue,
            source: PluginSource::Local { path: root.clone() },
            version: "0.0.1".into(),
            desc: None,
            commit: None,
            local_digest: Some("0".into()), // 过期基线 = 黄点状态
            installed: vec![],
        });

        sync_entry_identity(&mut reg, "P", false, &root);
        let e = reg.get("P").unwrap();
        assert_eq!(e.version, "1.2.3", "版本跟随源码 uplugin");
        assert_eq!(
            e.local_digest.as_deref(),
            Some(crate::update::local_digest(&root).to_string()).as_deref(),
            "摘要基线对齐安装内容"
        );
        std::fs::remove_dir_all(&root).ok();
    }
}
