export type MediaType = "image" | "video";
export type ImportMode = "copy" | "reference";
export type ProjectKind = "simple" | "canvas";
export type AssetSearchField = "title" | "prompt" | "notes";
export type ExportMode = "complete" | "prompts";

export interface PageRequest {
  readonly cursor?: string;
  readonly limit: number;
}

export interface PageResult<T> {
  readonly items: readonly T[];
  readonly nextCursor: string | null;
}

/** 资产检索条件。未填写的字段不会限制结果。 */
export interface AssetListRequest extends PageRequest {
  readonly projectId?: string;
  readonly mediaType?: MediaType;
  readonly keyword?: string;
  readonly searchField?: AssetSearchField;
  readonly exactMatch?: boolean;
  readonly model?: string;
  readonly platform?: string;
  readonly categoryIds?: readonly string[];
  readonly rating?: number;
  readonly isFavorite?: boolean;
  readonly isPublic?: boolean;
  readonly createdAfter?: number;
  readonly createdBefore?: number;
  readonly minAspectRatio?: number;
  readonly maxAspectRatio?: number;
}

export interface NumberedAssetListRequest extends Omit<
  AssetListRequest,
  "cursor" | "limit"
> {
  readonly page: number;
  readonly pageSize: 10 | 25 | 50;
}

export interface NumberedPageResult<T> {
  readonly items: readonly T[];
  readonly page: number;
  readonly pageSize: 10 | 25 | 50;
  readonly totalCount: number;
  readonly totalPages: number;
}

export interface DuplicateAssetGroup {
  readonly contentHash: string;
  readonly assetCount: number;
  readonly representativeAssetId: string;
  readonly representativeFileName: string;
  readonly updatedAt: string;
}

export interface DuplicateGroupPageRequest {
  readonly cursor?: string;
  readonly limit: number;
}

export interface WorkspaceState {
  readonly isOpen: boolean;
  readonly displayName: string | null;
}

export interface ProjectSummary {
  readonly id: string;
  readonly title: string;
  readonly description: string;
  readonly rating: number;
  readonly isFavorite: boolean;
  readonly isPublic: boolean;
  readonly updatedAt: string;
  readonly assetCount: number;
  readonly kind: ProjectKind;
}

export interface ProjectDetail extends ProjectSummary {
  readonly promptZh: string;
  readonly promptEn: string;
  readonly negativePrompt: string;
  readonly notes: string;
  readonly categoryIds: readonly string[];
  readonly tagIds: readonly string[];
}

/** 列表摘要刻意不包含原图、完整视频或详情预览地址。 */
export interface AssetSummary {
  readonly id: string;
  /** 稳定展示编号；编辑元数据不会改变此位置。 */
  readonly displayOrder?: number;
  readonly projectId: string | null;
  readonly fileName: string;
  readonly mediaType: MediaType;
  readonly thumbnailUrl: string | null;
  readonly coverUrl?: string | null;
  readonly modelName: string;
  readonly platformName: string;
  readonly rating: number;
  readonly isFavorite: boolean;
  readonly isPublic: boolean;
  readonly width: number | null;
  readonly height: number | null;
  readonly durationMs: number | null;
  readonly updatedAt: string;
}

export interface AssetDetail extends AssetSummary {
  readonly title: string;
  readonly promptZh: string;
  readonly promptEn: string;
  readonly negativePrompt: string;
  readonly generationParamsJson: string;
  readonly notes: string;
  readonly previewUrl: string | null;
  readonly previewError: string | null;
  readonly categoryIds: readonly string[];
  readonly tagIds: readonly string[];
  readonly storedPath: string;
}

export interface ProjectDraft {
  readonly kind: ProjectKind;
  readonly title: string;
  readonly description: string;
  readonly rating: number;
  readonly isFavorite: boolean;
  readonly isPublic: boolean;
  readonly promptZh: string;
  readonly promptEn: string;
  readonly negativePrompt: string;
  readonly notes: string;
  readonly categoryIds: readonly string[];
  readonly tagIds: readonly string[];
}

/** 画布项目只返回当前页的成员摘要与提示词，不包含原始媒体。 */
export type CanvasMemberRole = "output" | "reference";

export interface CanvasProjectMember {
  readonly assetId: string;
  readonly displayOrder: number;
  readonly fileName: string;
  readonly mediaType: MediaType;
  readonly modelName: string;
  readonly platformName: string;
  readonly width: number | null;
  readonly height: number | null;
  readonly durationMs: number | null;
  readonly updatedAt: string;
  readonly role: CanvasMemberRole;
  readonly referenceName: string | null;
  readonly promptZh: string;
  readonly promptEn: string;
  readonly negativePrompt: string;
}

export interface CanvasOutputPrompt {
  readonly promptZh: string;
  readonly promptEn: string;
  readonly negativePrompt: string;
}

export interface AssetDraft {
  readonly title: string;
  readonly projectId: string | null;
  readonly modelName: string;
  readonly platformName: string;
  readonly promptZh: string;
  readonly promptEn: string;
  readonly negativePrompt: string;
  readonly generationParamsJson: string;
  readonly notes: string;
  readonly rating: number;
  readonly isFavorite: boolean;
  readonly isPublic: boolean;
  readonly categoryIds: readonly string[];
  readonly tagIds: readonly string[];
}

export interface ImportRequest {
  readonly sourcePaths: readonly string[];
  readonly mode: ImportMode;
  readonly projectId: string | null;
}

export type ImportState =
  "queued" | "running" | "completed" | "cancelled" | "failed";

export interface ImportTask {
  readonly id: string;
  readonly state: ImportState;
  readonly completed: number;
  readonly total: number;
  readonly currentFileName: string | null;
  readonly message: string | null;
}

export interface CategoryItem {
  readonly id: string;
  readonly dimensionId: string;
  readonly name: string;
  readonly color: string | null;
  readonly assetCount: number;
  readonly enabled: boolean;
  readonly aliases: readonly string[];
  readonly description: string;
  readonly icon: string | null;
}

export interface CategoryDimension {
  readonly id: string;
  readonly name: string;
  readonly categories: readonly CategoryItem[];
  readonly allowsMultiple: boolean;
  readonly aiCanSuggestNew: boolean;
  readonly enabled: boolean;
}

export interface TagItem {
  readonly id: string;
  readonly name: string;
  readonly assetCount: number;
}

export type MetadataPresetKind = "model" | "platform";

export interface MetadataPreset {
  readonly id: string;
  readonly name: string;
  readonly assetCount: number;
}

export interface MetadataPresets {
  readonly models: readonly MetadataPreset[];
  readonly platforms: readonly MetadataPreset[];
}

export interface TrashItem {
  readonly id: string;
  readonly entityType: "project" | "asset";
  readonly entityId: string;
  readonly title: string;
  readonly mediaType: MediaType | null;
  readonly thumbnailUrl: string | null;
  readonly deletedAt: string;
  readonly originalMediaPreserved: boolean;
}

export interface LibraryService {
  getWorkspaceState(): Promise<WorkspaceState>;
  selectWorkspaceDirectory(): Promise<string | null>;
  createWorkspace(rootPath: string): Promise<WorkspaceState>;
  connectWorkspace(rootPath: string): Promise<WorkspaceState>;

  listProjects(request: PageRequest): Promise<PageResult<ProjectSummary>>;
  getProjectDetail(id: string): Promise<ProjectDetail>;
  createProject(draft: ProjectDraft): Promise<ProjectSummary>;
  updateProject(id: string, draft: ProjectDraft): Promise<ProjectSummary>;
  assignAssetsToProject(
    projectId: string,
    displayNumbers: readonly number[],
  ): Promise<number>;
  removeAssetsFromProject(
    projectId: string,
    assetIds: readonly string[],
  ): Promise<number>;
  listCanvasMembers(
    projectId: string,
    request: PageRequest & { readonly role?: CanvasMemberRole },
  ): Promise<PageResult<CanvasProjectMember>>;
  setCanvasMember(
    projectId: string,
    assetId: string,
    role: CanvasProjectMember["role"],
    referenceName: string | null,
  ): Promise<void>;
  updateCanvasOutputPrompt(
    projectId: string,
    assetId: string,
    prompt: CanvasOutputPrompt,
  ): Promise<void>;

  listAssets(request: AssetListRequest): Promise<PageResult<AssetSummary>>;
  listAssetPage(
    request: NumberedAssetListRequest,
  ): Promise<NumberedPageResult<AssetSummary>>;
  listDuplicateGroups(
    request: DuplicateGroupPageRequest,
  ): Promise<PageResult<DuplicateAssetGroup>>;
  /**
   * 仅在可见卡片上按需读取缩略图。返回的 blob URL 由调用方在不再使用时释放。
   */
  getAssetThumbnail(id: string): Promise<string | null>;
  getAssetDetail(id: string): Promise<AssetDetail>;
  createAsset(draft: AssetDraft): Promise<AssetDetail>;
  updateAsset(id: string, draft: AssetDraft): Promise<AssetDetail>;
  updateAssetDisplayOrder(
    id: string,
    input: {
      readonly targetPosition: number;
      readonly mode: "swap" | "shiftFollowing";
    },
  ): Promise<AssetDetail>;

  startImport(request: ImportRequest): Promise<ImportTask>;
  selectImportFiles(): Promise<readonly string[]>;
  selectImportDirectory(): Promise<string | null>;
  selectExportDirectory(): Promise<string | null>;
  exportAssets(
    assetIds: readonly string[],
    mode: ExportMode,
    targetDirectory: string,
  ): Promise<string>;
  exportProjects(
    projectIds: readonly string[],
    mode: ExportMode,
    targetDirectory: string,
  ): Promise<string>;
  subscribeImport(
    taskId: string,
    listener: (task: ImportTask) => void,
  ): () => void;
  cancelImport(taskId: string): Promise<void>;

  copyText(value: string): Promise<void>;

  listTaxonomy(): Promise<{
    readonly dimensions: readonly CategoryDimension[];
    readonly tags: readonly TagItem[];
  }>;
  listMetadataPresets(): Promise<MetadataPresets>;
  createMetadataPreset(
    kind: MetadataPresetKind,
    name: string,
  ): Promise<MetadataPreset>;
  updateMetadataPreset(
    kind: MetadataPresetKind,
    id: string,
    name: string,
  ): Promise<void>;
  deleteMetadataPreset(kind: MetadataPresetKind, id: string): Promise<void>;
  createDimension(name: string): Promise<CategoryDimension>;
  updateDimension(dimension: CategoryDimension, name: string): Promise<void>;
  deleteDimension(id: string): Promise<void>;
  createCategory(input: {
    readonly dimensionId: string;
    readonly name: string;
  }): Promise<CategoryItem>;
  createTag(name: string): Promise<TagItem>;
  updateCategory(category: CategoryItem, name: string): Promise<void>;
  updateTag(id: string, name: string): Promise<void>;
  deleteTag(id: string): Promise<void>;
  getCategoryImpact(categoryId: string): Promise<number>;
  deleteCategory(input: {
    readonly categoryId: string;
    readonly replacementCategoryId: string | null;
  }): Promise<void>;

  moveAssetToTrash(assetId: string): Promise<TrashItem>;
  moveAssetsToTrash(assetIds: readonly string[]): Promise<void>;
  moveProjectToTrash(projectId: string): Promise<TrashItem>;
  undoLastTrash(): Promise<void>;
  listTrash(request: PageRequest): Promise<PageResult<TrashItem>>;
  restoreTrashItem(trashId: string): Promise<void>;
  restoreTrashItems(trashIds: readonly string[]): Promise<void>;
  permanentlyDeleteTrashItem(trashId: string): Promise<void>;
  permanentlyDeleteTrashItems(trashIds: readonly string[]): Promise<void>;
}
