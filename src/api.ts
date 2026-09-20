/** Tauri 后端调用封装 + DTO 类型（与 commands.rs 对应） */
import { invoke } from "@tauri-apps/api/core";

export const inTauri = "__TAURI_INTERNALS__" in window;

export async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!inTauri) throw new Error("浏览器预览模式：后端不可用（请在应用内操作）");
  return invoke<T>(cmd, args);
}

export interface UeEngine {
  version: string;
  root: string;
  runuat: string;
}

export interface HoudiniInstall {
  version: string;
  root: string;
  packagesDir: string;
}

export interface EnginesDto {
  ue: UeEngine[];
  houdini: HoudiniInstall[];
  obsidian: ObsidianInfo;
}

export interface ObsidianVault {
  id: string;
  name: string;
  path: string;
  open: boolean;
}

export interface ObsidianInfo {
  vaults: ObsidianVault[];
  appVersion: string | null;
}

/** 安装对话框通用引擎行（UE 引擎 / Houdini 安装 / Obsidian vault 统一形状；
 *  id 存在时（vault）以 id 作为安装目标标识，否则用 version */
export interface EngineRow {
  version: string;
  root: string;
  id?: string;
}

export interface PluginRow {
  id: string;
  name: string;
  host: "UE" | "Houdini" | "Obsidian";
  version: string;
  latest: string;
  engines: string[];
  status: "installed" | "updatable" | "idle" | "error";
  origin: string;
  source: "github" | "local";
  desc: string;
}

export interface InstallResult {
  engine: string;
  ok: boolean;
  error: string | null;
}

export interface TargetPath {
  engine: string;
  path: string;
}

export interface UninstallReport {
  removed: TargetPath[];
  missing: TargetPath[];
  locked: TargetPath[];
  failed: { engine: string; path: string; reason: string }[];
  entryExhausted: boolean;
}

/** compat（§3.7 四态） */
export type CompatStatusDto =
  | { kind: "installed" }
  | { kind: "installable"; gitRef: string; confirmed: boolean }
  | { kind: "unverified" }
  | { kind: "incompatible"; reason: string };

export interface EngineCompatDto {
  engine: string;
  status: CompatStatusDto;
}

/** 更新检查（§3.5） */
export type UpdateStateDto =
  | { kind: "latest" }
  | { kind: "available"; newVersion: string }
  | { kind: "failed"; reason: string };

export interface UpdateDto {
  id: string;
  state: UpdateStateDto;
}

/** 预检（§3.8） */
export interface PreflightItem {
  key: "compat-source" | "vs-toolchain" | "engine-complete" | "file-lock" | "disk-space";
  ok: boolean;
  blocking: boolean;
  message: string;
}

export interface EnginePreflight {
  engine: string;
  items: PreflightItem[];
}

/** 设置与缓存（M6） */
export interface Settings {
  extraUeRoots: string[];
}

export interface CacheStats {
  repos: number;
  releases: number;
  build: number;
}

export interface EnvStatus {
  gh: boolean;
  curl: boolean;
  pnpm: boolean;
}

export interface FilterState {
  host: "all" | "UE" | "Houdini" | "Obsidian";
  status: "all" | "installed" | "updatable" | "idle" | "error";
  source: "all" | "github" | "local";
  engine: string;
  search: string;
}

export const DEFAULT_FILTER: FilterState = {
  host: "all",
  status: "all",
  source: "all",
  engine: "all",
  search: "",
};
