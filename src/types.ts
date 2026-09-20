/** 插件行视图模型（前端展示层；与 registry.rs 的 PluginEntry 对应） */
export interface PluginRow {
  id: string;
  /** 显示名 */
  name: string;
  /** 来源描述：GitHub 仓库全名或本地路径 */
  origin: string;
  host: "UE" | "Houdini";
  /** 已装版本；未安装为空 */
  version: string;
  /** 来源最新版本；与 version 不同即"可更新" */
  latest: string;
  /** 已安装引擎（UE 版本串 / Houdini 版本串） */
  engines: string[];
  status: "installed" | "updatable" | "idle" | "error";
  source: "github" | "local";
  desc: string;
}

/** 列表筛选状态（§4.1；引擎维度 M2 接 compat 后加） */
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
