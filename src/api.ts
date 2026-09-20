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
}

export interface PluginRow {
  id: string;
  name: string;
  host: "UE" | "Houdini";
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

export interface FilterState {
  host: "all" | "UE" | "Houdini";
  status: "all" | "installed" | "updatable" | "idle" | "error";
  source: "all" | "github" | "local";
  search: string;
}

export const DEFAULT_FILTER: FilterState = {
  host: "all",
  status: "all",
  source: "all",
  search: "",
};
