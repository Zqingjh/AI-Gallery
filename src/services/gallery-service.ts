import type { AssetSummary, PageRequest, PageResult } from "./library-service";

export type { AssetSummary, PageRequest, PageResult } from "./library-service";

export interface StartupSnapshot {
  readonly workspaceName: string;
  readonly readOnly: boolean;
}

/**
 * UI 只能依赖此契约；数据库、文件系统和 AI 调用由后续 adapter 实现。
 * 列表从契约层即采用分页摘要，避免形成全量媒体加载路径。
 */
export interface GalleryService {
  getStartupSnapshot(): Promise<StartupSnapshot>;
  listAssetSummaries(request: PageRequest): Promise<PageResult<AssetSummary>>;
}
