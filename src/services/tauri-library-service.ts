import type { CommandClient } from "./command-client";
import type { WorkspaceAccessMode } from "./workspace-management-service";
import {
  ServiceError,
  serviceErrorCode,
  toServiceError,
} from "./service-error";
import type {
  AssetDetail,
  AssetDraft,
  AssetListRequest,
  AssetSummary,
  CanvasProjectMember,
  CategoryDimension,
  CategoryItem,
  ImportRequest,
  ImportTask,
  LibraryService,
  MetadataPreset,
  MetadataPresetKind,
  MetadataPresets,
  DuplicateAssetGroup,
  DuplicateGroupPageRequest,
  ExportMode,
  PageRequest,
  PageResult,
  NumberedAssetListRequest,
  NumberedPageResult,
  ProjectDetail,
  ProjectDraft,
  ProjectSummary,
  TagItem,
  TrashItem,
  WorkspaceState,
} from "./library-service";

type UnknownRecord = Record<string, unknown>;

interface RawCursor {
  readonly updatedAt: number;
  readonly id: number;
}

interface RawDuplicateCursor {
  readonly updatedAt: number;
  readonly contentHash: string;
}

const terminalImportStates = new Set(["completed", "cancelled", "failed"]);

function record(value: unknown): UnknownRecord {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw invalidResponse();
  }
  return value as UnknownRecord;
}

function array(value: unknown): readonly unknown[] {
  if (!Array.isArray(value)) throw invalidResponse();
  return value;
}

function string(value: unknown): string {
  if (typeof value !== "string") throw invalidResponse();
  return value;
}

function number(value: unknown): number {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    throw invalidResponse();
  }
  return value;
}

function boolean(value: unknown): boolean {
  if (typeof value !== "boolean") throw invalidResponse();
  return value;
}

function nullableNumber(value: unknown): number | null {
  return value === null ? null : number(value);
}

function nullableString(value: unknown): string | null {
  return value === null ? null : string(value);
}

function invalidResponse(): ServiceError {
  return new ServiceError(
    serviceErrorCode.invalidResponse,
    "桌面服务返回了无法识别的作品库数据。",
  );
}

function requireRoot(rootPath: string | null): string {
  if (!rootPath) {
    throw new ServiceError(serviceErrorCode.commandFailed, "请先连接工作区。");
  }
  return rootPath;
}

function request(rootPath: string, fields: UnknownRecord = {}): UnknownRecord {
  return { request: { rootPath, ...fields } };
}

function encodeCursor(value: unknown): string | null {
  if (value === null) return null;
  const cursor = record(value);
  const encoded: RawCursor = {
    updatedAt: number(cursor.updatedAt),
    id: number(cursor.id),
  };
  return `${encoded.updatedAt}:${encoded.id}`;
}

function decodeCursor(value: string | undefined): RawCursor | undefined {
  if (value === undefined) return undefined;
  const separator = value.indexOf(":");
  const updatedAt = Number(value.slice(0, separator));
  const id = Number(value.slice(separator + 1));
  if (
    separator <= 0 ||
    !Number.isSafeInteger(updatedAt) ||
    !Number.isSafeInteger(id)
  ) {
    throw invalidResponse();
  }
  return { updatedAt, id };
}

function encodeDuplicateCursor(value: unknown): string | null {
  if (value === null) return null;
  const cursor = record(value);
  return `${number(cursor.updatedAt)}:${encodeURIComponent(string(cursor.contentHash))}`;
}

function decodeDuplicateCursor(
  value: string | undefined,
): RawDuplicateCursor | undefined {
  if (value === undefined) return undefined;
  const separator = value.indexOf(":");
  const updatedAt = Number(value.slice(0, separator));
  let contentHash = "";
  try {
    contentHash = decodeURIComponent(value.slice(separator + 1));
  } catch {
    throw invalidResponse();
  }
  if (separator <= 0 || !Number.isSafeInteger(updatedAt) || !contentHash) {
    throw invalidResponse();
  }
  return { updatedAt, contentHash };
}

function isoTimestamp(value: unknown): string {
  const timestamp = number(value);
  const date = new Date(timestamp);
  if (Number.isNaN(date.valueOf())) throw invalidResponse();
  return date.toISOString();
}

function parseProject(value: unknown): ProjectSummary {
  const item = record(value);
  const kind = item.kind ?? "simple";
  if (kind !== "simple" && kind !== "canvas") throw invalidResponse();
  return {
    id: String(number(item.id)),
    title: string(item.title),
    description: string(item.description),
    rating: number(item.rating),
    isFavorite: boolean(item.isFavorite),
    isPublic: boolean(item.isPublic),
    updatedAt: isoTimestamp(item.updatedAt),
    assetCount: number(item.assetCount),
    kind,
  };
}

function parseProjectDetail(value: unknown): ProjectDetail {
  const item = record(value);
  const prompt = record(item.prompt);
  return {
    ...parseProject(item),
    promptZh: string(prompt.promptZh),
    promptEn: string(prompt.promptEn),
    negativePrompt: string(prompt.negativePrompt),
    notes: string(item.notes),
    categoryIds: array(item.categoryIds).map((id) => String(number(id))),
    tagIds: array(item.tagIds).map((id) => String(number(id))),
  };
}

function parseAsset(value: unknown): AssetSummary {
  const item = record(value);
  const mediaType = item.mediaType;
  if (mediaType !== "image" && mediaType !== "video") {
    throw invalidResponse();
  }
  return {
    id: String(number(item.id)),
    ...(item.displayOrder === undefined
      ? {}
      : { displayOrder: number(item.displayOrder) }),
    projectId: item.projectId === null ? null : String(number(item.projectId)),
    fileName: string(item.fileName),
    mediaType,
    thumbnailUrl: null,
    coverUrl: null,
    modelName: nullableString(item.model) ?? "",
    platformName: nullableString(item.platform) ?? "",
    rating: number(item.rating),
    isFavorite: boolean(item.isFavorite),
    isPublic: boolean(item.isPublic),
    width: nullableNumber(item.width),
    height: nullableNumber(item.height),
    durationMs: nullableNumber(item.durationMs),
    updatedAt: isoTimestamp(item.updatedAt),
  };
}

function parseCanvasProjectMember(value: unknown): CanvasProjectMember {
  const item = record(value);
  const mediaType = item.mediaType;
  const role = item.role;
  if (mediaType !== "image" && mediaType !== "video") throw invalidResponse();
  if (role !== "output" && role !== "reference") throw invalidResponse();
  return {
    assetId: String(number(item.assetId)),
    displayOrder: number(item.displayOrder),
    fileName: string(item.fileName),
    mediaType,
    modelName: nullableString(item.modelName) ?? "",
    platformName: nullableString(item.platformName) ?? "",
    width: nullableNumber(item.width),
    height: nullableNumber(item.height),
    durationMs: nullableNumber(item.durationMs),
    updatedAt: isoTimestamp(item.updatedAt),
    role,
    referenceName: nullableString(item.referenceName),
    promptZh: string(item.promptZh),
    promptEn: string(item.promptEn),
    negativePrompt: string(item.negativePrompt),
  };
}

function parseAssetDetail(
  value: unknown,
  previewUrl: string | null,
  previewError: string | null = null,
): AssetDetail {
  const item = record(value);
  const prompt = record(item.prompt);
  return {
    ...parseAsset(item),
    title: string(item.fileName),
    promptZh: string(prompt.promptZh),
    promptEn: string(prompt.promptEn),
    negativePrompt: string(prompt.negativePrompt),
    generationParamsJson: JSON.stringify(record(item.generationParams)),
    notes: string(item.notes),
    previewUrl,
    previewError,
    categoryIds: array(item.categoryIds).map((id) => String(number(id))),
    tagIds: array(item.tagIds).map((id) => String(number(id))),
    storedPath: string(item.storedPath),
  };
}

function parseDuplicateGroup(value: unknown): DuplicateAssetGroup {
  const item = record(value);
  return {
    contentHash: string(item.contentHash),
    assetCount: number(item.assetCount),
    representativeAssetId: String(number(item.representativeAssetId)),
    representativeFileName: string(item.representativeFileName),
    updatedAt: isoTimestamp(item.updatedAt),
  };
}

function parseMetadataPreset(value: unknown): MetadataPreset {
  const item = record(value);
  return {
    id: String(number(item.id)),
    name: string(item.name),
    assetCount: number(item.assetCount),
  };
}

function parseMetadataPresets(value: unknown): MetadataPresets {
  const presets = record(value);
  return {
    models: array(presets.models).map(parseMetadataPreset),
    platforms: array(presets.platforms).map(parseMetadataPreset),
  };
}

function parseDuplicatePage(value: unknown): PageResult<DuplicateAssetGroup> {
  const page = record(value);
  return {
    items: array(page.items).map(parseDuplicateGroup),
    nextCursor: encodeDuplicateCursor(page.nextCursor),
  };
}

function parsePage<T>(
  value: unknown,
  parseItem: (item: unknown) => T,
): PageResult<T> {
  const page = record(value);
  return {
    items: array(page.items).map(parseItem),
    nextCursor: encodeCursor(page.nextCursor),
  };
}

function parseNumberedAssetPage(
  value: unknown,
): NumberedPageResult<AssetSummary> {
  const result = record(value);
  const page = number(result.page);
  const pageSize = number(result.pageSize);
  const totalCount = number(result.totalCount);
  const totalPages = number(result.totalPages);
  if (
    !Number.isSafeInteger(page) ||
    page < 1 ||
    (pageSize !== 10 && pageSize !== 25 && pageSize !== 50) ||
    !Number.isSafeInteger(totalCount) ||
    totalCount < 0 ||
    !Number.isSafeInteger(totalPages) ||
    totalPages < 0 ||
    totalPages !== (totalCount === 0 ? 0 : Math.ceil(totalCount / pageSize))
  ) {
    throw invalidResponse();
  }
  const items = array(result.items).map(parseAsset);
  if (items.length > pageSize) throw invalidResponse();
  return { items, page, pageSize, totalCount, totalPages };
}

function parseImportTask(value: unknown): ImportTask {
  const task = record(value);
  const state = task.state;
  if (
    state !== "queued" &&
    state !== "running" &&
    state !== "completed" &&
    state !== "cancelled" &&
    state !== "failed"
  ) {
    throw invalidResponse();
  }
  return {
    id: string(task.id),
    state,
    completed: number(task.completed),
    total: number(task.total),
    currentFileName: nullableString(task.currentFileName),
    message: nullableString(task.message),
  };
}

function projectInput(draft: ProjectDraft): UnknownRecord {
  return {
    kind: draft.kind,
    title: draft.title,
    description: draft.description,
    prompt: {
      promptZh: draft.promptZh,
      promptEn: draft.promptEn,
      negativePrompt: draft.negativePrompt,
    },
    rating: draft.rating,
    isFavorite: draft.isFavorite,
    isPublic: draft.isPublic,
    notes: draft.notes,
    categoryIds: draft.categoryIds.map(Number),
    tagIds: draft.tagIds.map(Number),
  };
}

function assetInput(raw: UnknownRecord, draft: AssetDraft): UnknownRecord {
  let generationParams: UnknownRecord;
  try {
    generationParams = record(
      draft.generationParamsJson.trim()
        ? JSON.parse(draft.generationParamsJson)
        : {},
    );
  } catch {
    throw new ServiceError(
      serviceErrorCode.commandFailed,
      "生成参数必须是 JSON 对象。",
    );
  }
  return {
    projectId: draft.projectId === null ? null : Number(draft.projectId),
    mediaType: raw.mediaType,
    pathKind: raw.pathKind,
    storedPath: raw.storedPath,
    fileName: draft.title.trim(),
    mimeType: raw.mimeType,
    fileSize: raw.fileSize,
    contentHash: raw.contentHash,
    width: raw.width,
    height: raw.height,
    durationMs: raw.durationMs,
    frameRate: raw.frameRate,
    hasAudio: raw.hasAudio,
    prompt: {
      promptZh: draft.promptZh,
      promptEn: draft.promptEn,
      negativePrompt: draft.negativePrompt,
    },
    model: draft.modelName.trim() || null,
    platform: draft.platformName.trim() || null,
    generationParams,
    rating: draft.rating,
    isFavorite: draft.isFavorite,
    isPublic: draft.isPublic,
    notes: draft.notes,
    categoryIds: draft.categoryIds.map(Number),
    tagIds: draft.tagIds.map(Number),
  };
}

function previewBlob(
  value: unknown,
  mediaType: "image" | "video",
  mimeType: string,
): string {
  if (
    !/^(image|video)\/[a-z0-9.+-]+$/i.test(mimeType) ||
    !mimeType.toLowerCase().startsWith(`${mediaType}/`)
  ) {
    throw invalidResponse();
  }
  let bytes: Uint8Array;
  if (value instanceof ArrayBuffer) {
    bytes = new Uint8Array(value);
  } else if (value instanceof Uint8Array) {
    bytes = value;
  } else if (Array.isArray(value)) {
    bytes = Uint8Array.from(value.map(number));
  } else {
    throw invalidResponse();
  }
  const ownedBytes = new Uint8Array(bytes.byteLength);
  ownedBytes.set(bytes);
  return URL.createObjectURL(
    new Blob([ownedBytes.buffer], {
      type: mimeType,
    }),
  );
}

function assetPreviewMimeType(
  rawMimeType: string | null,
  mediaType: "image" | "video",
  fileName: string,
): string {
  if (rawMimeType) return rawMimeType;
  if (mediaType === "video") return "video/mp4";
  const extension = fileName.split(".").at(-1)?.toLowerCase();
  const imageMimeTypes: Readonly<Record<string, string>> = {
    png: "image/png",
    jpg: "image/jpeg",
    jpeg: "image/jpeg",
    gif: "image/gif",
    webp: "image/webp",
  };
  if (extension && imageMimeTypes[extension]) return imageMimeTypes[extension];
  throw invalidResponse();
}

function parseThumbnail(value: unknown): string | null {
  const response = record(value);
  const state = string(response.state);
  if (state === "pending" || state === "unavailable") return null;
  if (state !== "ready") throw invalidResponse();
  const mimeType = string(response.mimeType);
  if (!/^image\/[a-z0-9.+-]+$/i.test(mimeType)) throw invalidResponse();
  const bytes = Uint8Array.from(array(response.bytes).map(number));
  const ownedBytes = new Uint8Array(bytes.byteLength);
  ownedBytes.set(bytes);
  return URL.createObjectURL(new Blob([ownedBytes.buffer], { type: mimeType }));
}

/**
 * 真实 command adapter 保存当前工作区路径，但路径只作为 IPC 输入，不进入列表、错误或全局 UI 状态。
 */
export type DisposableLibraryService = LibraryService & {
  dispose(): Promise<void>;
};

export function createTauriLibraryService(
  commandClient: CommandClient,
  getAccessMode: () => WorkspaceAccessMode | null = () => null,
  options: {
    readonly isolatedFromRoot?: () => string | null;
  } = {},
): DisposableLibraryService {
  let rootPath: string | null = null;
  let workspaceState: WorkspaceState = { isOpen: false, displayName: null };
  let disposed = false;
  const activeImportTaskIds = new Set<string>();
  const importSubscriptionCleanups = new Set<() => void>();

  async function invoke<TResult>(
    command: string,
    arguments_: UnknownRecord,
  ): Promise<TResult> {
    if (disposed) {
      throw new ServiceError(
        serviceErrorCode.commandFailed,
        "当前作品库会话已经关闭。",
      );
    }
    try {
      return await commandClient.invoke<TResult>(command, arguments_);
    } catch (error: unknown) {
      throw toServiceError(error);
    }
  }

  async function validateIsolation(nextRootPath: string): Promise<void> {
    const primaryRootPath = options.isolatedFromRoot?.();
    if (!primaryRootPath) return;
    await invoke<unknown>("validate_isolated_workspace", {
      request: {
        primaryRootPath,
        isolatedRootPath: nextRootPath,
      },
    });
  }

  function ensureWritable(): void {
    if (getAccessMode() === "readOnly") {
      throw new ServiceError(
        "WORKSPACE_READ_ONLY",
        "工作区正处于只读展示模式，无法修改内容。",
      );
    }
  }

  return {
    async getWorkspaceState(): Promise<WorkspaceState> {
      return workspaceState;
    },
    async selectWorkspaceDirectory(): Promise<string | null> {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({ directory: true, multiple: false });
      return typeof selected === "string" ? selected : null;
    },
    async createWorkspace(nextRootPath: string): Promise<WorkspaceState> {
      await validateIsolation(nextRootPath);
      const workspace = record(
        await invoke<unknown>("create_workspace", request(nextRootPath)),
      );
      await invoke<unknown>(
        "prepare_workspace_database",
        request(nextRootPath),
      );
      rootPath = nextRootPath;
      workspaceState = {
        isOpen: true,
        displayName: string(workspace.displayName),
      };
      return workspaceState;
    },
    async connectWorkspace(nextRootPath: string): Promise<WorkspaceState> {
      await validateIsolation(nextRootPath);
      const workspace = record(
        await invoke<unknown>("open_workspace", request(nextRootPath)),
      );
      await invoke<unknown>(
        "prepare_workspace_database",
        request(nextRootPath),
      );
      rootPath = nextRootPath;
      workspaceState = {
        isOpen: true,
        displayName: string(workspace.displayName),
      };
      return workspaceState;
    },
    async listProjects(pageRequest: PageRequest) {
      const root = requireRoot(rootPath);
      const response = await invoke<unknown>(
        "library_list_projects",
        request(root, {
          cursor: decodeCursor(pageRequest.cursor),
          limit: pageRequest.limit,
        }),
      );
      return parsePage(response, parseProject);
    },
    async createProject(draft: ProjectDraft) {
      ensureWritable();
      const root = requireRoot(rootPath);
      return parseProject(
        await invoke<unknown>(
          "library_create_project",
          request(root, { id: null, input: projectInput(draft) }),
        ),
      );
    },
    async getProjectDetail(id: string) {
      const root = requireRoot(rootPath);
      return parseProjectDetail(
        await invoke<unknown>(
          "library_get_project",
          request(root, { id: Number(id) }),
        ),
      );
    },
    async updateProject(id: string, draft: ProjectDraft) {
      ensureWritable();
      const root = requireRoot(rootPath);
      return parseProject(
        await invoke<unknown>(
          "library_update_project",
          request(root, { id: Number(id), input: projectInput(draft) }),
        ),
      );
    },
    async assignAssetsToProject(projectId, displayNumbers) {
      ensureWritable();
      const root = requireRoot(rootPath);
      const value = await invoke<unknown>(
        "library_assign_assets_to_project",
        request(root, {
          projectId: Number(projectId),
          displayNumbers: [...displayNumbers],
          confirmed: true,
        }),
      );
      if (
        typeof value !== "number" ||
        !Number.isSafeInteger(value) ||
        value < 0
      ) {
        throw new ServiceError("INVALID_RESPONSE", "批量归入结果无法识别。");
      }
      return value;
    },
    async removeAssetsFromProject(projectId, assetIds) {
      ensureWritable();
      const root = requireRoot(rootPath);
      const value = await invoke<unknown>(
        "library_remove_assets_from_project",
        request(root, {
          projectId: Number(projectId),
          assetIds: assetIds.map(Number),
          confirmed: true,
        }),
      );
      if (
        typeof value !== "number" ||
        !Number.isSafeInteger(value) ||
        value < 0
      ) {
        throw new ServiceError(
          "INVALID_RESPONSE",
          "批量移出项目结果无法识别。",
        );
      }
      return value;
    },
    async listCanvasMembers(projectId, pageRequest) {
      const root = requireRoot(rootPath);
      return parsePage(
        await invoke<unknown>(
          "library_list_canvas_members",
          request(root, {
            projectId: Number(projectId),
            cursor: decodeCursor(pageRequest.cursor),
            limit: pageRequest.limit,
            role: pageRequest.role,
          }),
        ),
        parseCanvasProjectMember,
      );
    },
    async setCanvasMember(projectId, assetId, role, referenceName) {
      ensureWritable();
      const root = requireRoot(rootPath);
      await invoke<unknown>(
        "library_set_canvas_member",
        request(root, {
          projectId: Number(projectId),
          assetId: Number(assetId),
          role,
          referenceName,
          confirmed: true,
        }),
      );
    },
    async updateCanvasOutputPrompt(projectId, assetId, prompt) {
      ensureWritable();
      const root = requireRoot(rootPath);
      await invoke<unknown>(
        "library_update_canvas_output_prompt",
        request(root, {
          projectId: Number(projectId),
          assetId: Number(assetId),
          prompt,
          confirmed: true,
        }),
      );
    },
    async listAssets(pageRequest: AssetListRequest) {
      const root = requireRoot(rootPath);
      const response = await invoke<unknown>(
        "library_list_assets",
        request(root, {
          projectId: pageRequest.projectId
            ? Number(pageRequest.projectId)
            : null,
          mediaType: pageRequest.mediaType ?? null,
          keyword: pageRequest.keyword?.trim() || null,
          searchField: pageRequest.searchField ?? "title",
          exactMatch: pageRequest.exactMatch ?? false,
          model: pageRequest.model?.trim() || null,
          platform: pageRequest.platform?.trim() || null,
          categoryIds: pageRequest.categoryIds?.map(Number) ?? [],
          rating: pageRequest.rating ?? null,
          isFavorite: pageRequest.isFavorite ?? null,
          isPublic: pageRequest.isPublic ?? null,
          createdAfter: pageRequest.createdAfter ?? null,
          createdBefore: pageRequest.createdBefore ?? null,
          minAspectRatio: pageRequest.minAspectRatio ?? null,
          maxAspectRatio: pageRequest.maxAspectRatio ?? null,
          cursor: decodeCursor(pageRequest.cursor),
          limit: pageRequest.limit,
        }),
      );
      return parsePage(response, parseAsset);
    },
    async listAssetPage(pageRequest: NumberedAssetListRequest) {
      const root = requireRoot(rootPath);
      const response = await invoke<unknown>(
        "library_list_assets_numbered",
        request(root, {
          projectId: pageRequest.projectId
            ? Number(pageRequest.projectId)
            : null,
          mediaType: pageRequest.mediaType ?? null,
          keyword: pageRequest.keyword?.trim() || null,
          searchField: pageRequest.searchField ?? "title",
          exactMatch: pageRequest.exactMatch ?? false,
          model: pageRequest.model?.trim() || null,
          platform: pageRequest.platform?.trim() || null,
          categoryIds: pageRequest.categoryIds?.map(Number) ?? [],
          rating: pageRequest.rating ?? null,
          isFavorite: pageRequest.isFavorite ?? null,
          isPublic: pageRequest.isPublic ?? null,
          createdAfter: pageRequest.createdAfter ?? null,
          createdBefore: pageRequest.createdBefore ?? null,
          minAspectRatio: pageRequest.minAspectRatio ?? null,
          maxAspectRatio: pageRequest.maxAspectRatio ?? null,
          page: pageRequest.page,
          pageSize: pageRequest.pageSize,
        }),
      );
      return parseNumberedAssetPage(response);
    },
    async listDuplicateGroups(pageRequest: DuplicateGroupPageRequest) {
      const root = requireRoot(rootPath);
      return parseDuplicatePage(
        await invoke<unknown>(
          "library_list_duplicate_groups",
          request(root, {
            cursor: decodeDuplicateCursor(pageRequest.cursor),
            limit: pageRequest.limit,
          }),
        ),
      );
    },
    async getAssetThumbnail(id: string): Promise<string | null> {
      const root = requireRoot(rootPath);
      const thumbnail = await invoke<unknown>(
        "library_read_asset_thumbnail",
        request(root, { id: Number(id) }),
      );
      return parseThumbnail(thumbnail);
    },
    async getAssetDetail(id: string) {
      const root = requireRoot(rootPath);
      const raw = await invoke<unknown>(
        "library_get_asset",
        request(root, { id: Number(id) }),
      );
      const rawAsset = record(raw);
      const summary = parseAsset(rawAsset);
      let previewUrl: string | null = null;
      let previewError: string | null = null;
      try {
        const preview = await commandClient.invoke<unknown>(
          "library_read_asset_preview",
          request(root, { id: Number(id) }),
        );
        previewUrl = previewBlob(
          preview,
          summary.mediaType,
          assetPreviewMimeType(
            nullableString(rawAsset.mimeType),
            summary.mediaType,
            summary.fileName,
          ),
        );
      } catch (error: unknown) {
        previewUrl = null;
        previewError = toServiceError(error).message;
      }
      return parseAssetDetail(raw, previewUrl, previewError);
    },
    async createAsset() {
      ensureWritable();
      throw new ServiceError(
        serviceErrorCode.commandFailed,
        "请通过导入媒体创建作品。",
      );
    },
    async updateAsset(id: string, draft: AssetDraft) {
      ensureWritable();
      const root = requireRoot(rootPath);
      const raw = record(
        await invoke<unknown>(
          "library_get_asset",
          request(root, { id: Number(id) }),
        ),
      );
      const updated = await invoke<unknown>(
        "library_update_asset",
        request(root, { id: Number(id), input: assetInput(raw, draft) }),
      );
      return parseAssetDetail(updated, null);
    },
    async updateAssetDisplayOrder(id, input) {
      ensureWritable();
      const root = requireRoot(rootPath);
      if (
        !Number.isSafeInteger(input.targetPosition) ||
        input.targetPosition < 1
      ) {
        throw new ServiceError(
          serviceErrorCode.commandFailed,
          "作品编号必须是大于零的整数。",
        );
      }
      return parseAssetDetail(
        await invoke<unknown>(
          "library_update_asset_display_order",
          request(root, {
            id: Number(id),
            targetPosition: input.targetPosition,
            mode: input.mode,
          }),
        ),
        null,
      );
    },
    async startImport(importRequest: ImportRequest) {
      ensureWritable();
      const root = requireRoot(rootPath);
      const task = parseImportTask(
        await invoke<unknown>(
          "start_media_import",
          request(root, {
            sourcePaths: importRequest.sourcePaths,
            mode: importRequest.mode,
            projectId:
              importRequest.projectId === null
                ? null
                : Number(importRequest.projectId),
          }),
        ),
      );
      if (!terminalImportStates.has(task.state)) {
        if (disposed) {
          await commandClient
            .invoke<unknown>("cancel_media_import", {
              request: { taskId: task.id },
            })
            .catch(() => undefined);
          throw new ServiceError(
            serviceErrorCode.commandFailed,
            "当前作品库会话已经关闭。",
          );
        }
        activeImportTaskIds.add(task.id);
      }
      return task;
    },
    async selectImportFiles(): Promise<readonly string[]> {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({
        directory: false,
        multiple: true,
        filters: [
          {
            name: "图片与 MP4",
            extensions: ["jpg", "jpeg", "png", "webp", "gif", "mp4"],
          },
        ],
      });
      if (selected === null) return [];
      return typeof selected === "string" ? [selected] : selected;
    },
    async selectImportDirectory(): Promise<string | null> {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({ directory: true, multiple: false });
      return typeof selected === "string" ? selected : null;
    },
    async selectExportDirectory(): Promise<string | null> {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({ directory: true, multiple: false });
      return typeof selected === "string" ? selected : null;
    },
    async exportAssets(assetIds, mode, targetDirectory) {
      ensureWritable();
      const root = requireRoot(rootPath);
      return string(
        await invoke<unknown>(
          "library_export_selection",
          request(root, {
            targetDirectory,
            mode,
            assetIds: assetIds.map(Number),
            projectIds: null,
          }),
        ),
      );
    },
    async exportProjects(projectIds, mode, targetDirectory) {
      ensureWritable();
      const root = requireRoot(rootPath);
      return string(
        await invoke<unknown>(
          "library_export_selection",
          request(root, {
            targetDirectory,
            mode,
            assetIds: null,
            projectIds: projectIds.map(Number),
          }),
        ),
      );
    },
    subscribeImport(taskId, listener) {
      let active = true;
      const cleanup = () => {
        if (!active) return;
        active = false;
        window.clearInterval(interval);
        importSubscriptionCleanups.delete(cleanup);
      };
      const poll = async () => {
        if (disposed) {
          cleanup();
          return;
        }
        try {
          const task = parseImportTask(
            await commandClient.invoke<unknown>("get_media_import_task", {
              request: { taskId },
            }),
          );
          if (!active) return;
          listener(task);
          if (terminalImportStates.has(task.state)) {
            activeImportTaskIds.delete(taskId);
            cleanup();
          }
        } catch {
          if (active) {
            listener({
              id: taskId,
              state: "failed",
              completed: 0,
              total: 0,
              currentFileName: null,
              message: "无法获取导入进度。",
            });
            activeImportTaskIds.delete(taskId);
            cleanup();
          }
        }
      };
      const interval = window.setInterval(() => void poll(), 250);
      importSubscriptionCleanups.add(cleanup);
      void poll();
      return cleanup;
    },
    async cancelImport(taskId: string) {
      ensureWritable();
      try {
        await invoke<unknown>("cancel_media_import", {
          request: { taskId },
        });
      } finally {
        activeImportTaskIds.delete(taskId);
      }
    },
    async copyText(value: string) {
      await navigator.clipboard.writeText(value);
    },
    async listTaxonomy() {
      const root = requireRoot(rootPath);
      const [rawDimensions, rawCategories, rawTags] = await Promise.all([
        invoke<unknown>("library_list_dimensions", request(root)),
        invoke<unknown>(
          "library_list_categories",
          request(root, { dimensionId: null }),
        ),
        invoke<unknown>("library_list_tags", request(root)),
      ]);
      const categories = array(rawCategories).map((value): CategoryItem => {
        const item = record(value);
        return {
          id: String(number(item.id)),
          dimensionId: String(number(item.dimensionId)),
          name: string(item.name),
          color: nullableString(item.color),
          assetCount: number(item.assetCount),
          enabled: boolean(item.isEnabled),
          aliases: array(item.aliases).map(string),
          description: string(item.description),
          icon: nullableString(item.icon),
        };
      });
      const dimensions = array(rawDimensions).map(
        (value): CategoryDimension => {
          const item = record(value);
          const id = String(number(item.id));
          return {
            id,
            name: string(item.name),
            allowsMultiple: boolean(item.allowsMultiple),
            aiCanSuggestNew: boolean(item.aiCanSuggestNew),
            enabled: boolean(item.isEnabled),
            categories: categories.filter(
              (category) => category.dimensionId === id,
            ),
          };
        },
      );
      const tags = array(rawTags).map((value): TagItem => {
        const item = record(value);
        return {
          id: String(number(item.id)),
          name: string(item.name),
          assetCount: number(item.assetCount),
        };
      });
      return { dimensions, tags };
    },
    async listMetadataPresets() {
      const root = requireRoot(rootPath);
      return parseMetadataPresets(
        await invoke<unknown>("library_list_metadata_presets", request(root)),
      );
    },
    async createMetadataPreset(kind: MetadataPresetKind, name: string) {
      ensureWritable();
      const root = requireRoot(rootPath);
      const id = number(
        await invoke<unknown>(
          "library_write_metadata_preset",
          request(root, { kind, id: null, name }),
        ),
      );
      return { id: String(id), name, assetCount: 0 };
    },
    async updateMetadataPreset(kind, id, name) {
      ensureWritable();
      const root = requireRoot(rootPath);
      await invoke<unknown>(
        "library_write_metadata_preset",
        request(root, { kind, id: Number(id), name }),
      );
    },
    async deleteMetadataPreset(kind, id) {
      ensureWritable();
      const root = requireRoot(rootPath);
      await invoke<unknown>(
        "library_delete_metadata_preset",
        request(root, { kind, id: Number(id) }),
      );
    },
    async createCategory(input) {
      ensureWritable();
      const root = requireRoot(rootPath);
      const id = number(
        await invoke<unknown>(
          "library_write_category",
          request(root, {
            id: null,
            input: {
              dimensionId: Number(input.dimensionId),
              name: input.name,
              aliases: [],
              description: "",
              color: null,
              icon: null,
              isEnabled: true,
            },
          }),
        ),
      );
      return {
        id: String(id),
        dimensionId: input.dimensionId,
        name: input.name,
        color: null,
        assetCount: 0,
        enabled: true,
        aliases: [],
        description: "",
        icon: null,
      };
    },
    async createDimension(name: string) {
      ensureWritable();
      const root = requireRoot(rootPath);
      const id = number(
        await invoke<unknown>(
          "library_write_dimension",
          request(root, {
            id: null,
            input: {
              name,
              allowsMultiple: true,
              aiCanSuggestNew: false,
              isEnabled: true,
            },
          }),
        ),
      );
      return {
        id: String(id),
        name,
        categories: [],
        allowsMultiple: true,
        aiCanSuggestNew: false,
        enabled: true,
      };
    },
    async updateDimension(dimension, name) {
      ensureWritable();
      const root = requireRoot(rootPath);
      await invoke<unknown>(
        "library_write_dimension",
        request(root, {
          id: Number(dimension.id),
          input: {
            name,
            allowsMultiple: dimension.allowsMultiple,
            aiCanSuggestNew: dimension.aiCanSuggestNew,
            isEnabled: dimension.enabled,
          },
        }),
      );
    },
    async deleteDimension(id) {
      ensureWritable();
      const root = requireRoot(rootPath);
      await invoke<unknown>(
        "library_delete_dimension",
        request(root, { id: Number(id) }),
      );
    },
    async createTag(name: string) {
      ensureWritable();
      const root = requireRoot(rootPath);
      const id = number(
        await invoke<unknown>(
          "library_write_tag",
          request(root, { id: null, name }),
        ),
      );
      return { id: String(id), name, assetCount: 0 };
    },
    async updateCategory(category, name) {
      ensureWritable();
      const root = requireRoot(rootPath);
      await invoke<unknown>(
        "library_write_category",
        request(root, {
          id: Number(category.id),
          input: {
            dimensionId: Number(category.dimensionId),
            name,
            aliases: category.aliases,
            description: category.description,
            color: category.color,
            icon: category.icon,
            isEnabled: category.enabled,
          },
        }),
      );
    },
    async updateTag(id, name) {
      ensureWritable();
      const root = requireRoot(rootPath);
      await invoke<unknown>(
        "library_write_tag",
        request(root, { id: Number(id), name }),
      );
    },
    async deleteTag(id) {
      ensureWritable();
      const root = requireRoot(rootPath);
      await invoke<unknown>(
        "library_delete_tag",
        request(root, { id: Number(id) }),
      );
    },
    async getCategoryImpact(categoryId: string) {
      const root = requireRoot(rootPath);
      const impact = record(
        await invoke<unknown>(
          "library_category_impact",
          request(root, { id: Number(categoryId) }),
        ),
      );
      return number(impact.projectCount) + number(impact.assetCount);
    },
    async deleteCategory(input) {
      ensureWritable();
      const root = requireRoot(rootPath);
      await invoke<unknown>(
        "library_delete_category",
        request(root, {
          id: Number(input.categoryId),
          replacementId:
            input.replacementCategoryId === null
              ? null
              : Number(input.replacementCategoryId),
        }),
      );
    },
    async moveAssetToTrash(assetId: string) {
      ensureWritable();
      const root = requireRoot(rootPath);
      const detail = parseAsset(
        await invoke<unknown>(
          "library_get_asset",
          request(root, { id: Number(assetId) }),
        ),
      );
      const trashId = number(
        await invoke<unknown>(
          "library_move_asset_to_trash",
          request(root, { id: Number(assetId) }),
        ),
      );
      return {
        id: String(trashId),
        entityType: "asset",
        entityId: assetId,
        title: detail.fileName,
        mediaType: detail.mediaType,
        thumbnailUrl: detail.thumbnailUrl,
        deletedAt: new Date().toISOString(),
        originalMediaPreserved: true,
      };
    },
    async moveAssetsToTrash(assetIds) {
      ensureWritable();
      const root = requireRoot(rootPath);
      await invoke<unknown>(
        "library_move_assets_to_trash",
        request(root, { ids: assetIds.map(Number) }),
      );
    },
    async moveProjectToTrash(projectId: string) {
      ensureWritable();
      const root = requireRoot(rootPath);
      const detail = parseProjectDetail(
        await invoke<unknown>(
          "library_get_project",
          request(root, { id: Number(projectId) }),
        ),
      );
      const trashId = number(
        await invoke<unknown>(
          "library_move_project_to_trash",
          request(root, { id: Number(projectId) }),
        ),
      );
      return {
        id: String(trashId),
        entityType: "project",
        entityId: projectId,
        title: detail.title,
        mediaType: null,
        thumbnailUrl: null,
        deletedAt: new Date().toISOString(),
        originalMediaPreserved: true,
      };
    },
    async undoLastTrash() {
      ensureWritable();
      const root = requireRoot(rootPath);
      await invoke<unknown>("library_undo_recent_delete", request(root));
    },
    async listTrash(pageRequest: PageRequest) {
      const root = requireRoot(rootPath);
      const page = record(
        await invoke<unknown>(
          "library_list_trash",
          request(root, {
            cursor: decodeCursor(pageRequest.cursor),
            limit: pageRequest.limit,
          }),
        ),
      );
      const items = array(page.items).map((value): TrashItem => {
        const item = record(value);
        const entityType = item.entityType;
        if (entityType !== "asset" && entityType !== "project") {
          throw invalidResponse();
        }
        const mediaType = item.mediaType;
        if (
          mediaType !== null &&
          mediaType !== "image" &&
          mediaType !== "video"
        ) {
          throw invalidResponse();
        }
        return {
          id: String(number(item.id)),
          entityType,
          entityId: String(number(item.entityId)),
          title: string(item.displayName),
          mediaType,
          thumbnailUrl: null,
          deletedAt: isoTimestamp(item.deletedAt),
          originalMediaPreserved: true,
        };
      });
      return { items, nextCursor: encodeCursor(page.nextCursor) };
    },
    async restoreTrashItem(trashId: string) {
      ensureWritable();
      const root = requireRoot(rootPath);
      await invoke<unknown>(
        "library_restore_trash",
        request(root, { id: Number(trashId) }),
      );
    },
    async restoreTrashItems(trashIds) {
      ensureWritable();
      const root = requireRoot(rootPath);
      await invoke<unknown>(
        "library_restore_trash_batch",
        request(root, { ids: trashIds.map(Number) }),
      );
    },
    async permanentlyDeleteTrashItem(trashId: string) {
      ensureWritable();
      const root = requireRoot(rootPath);
      await invoke<unknown>(
        "library_purge_trash_record",
        request(root, { id: Number(trashId) }),
      );
    },
    async permanentlyDeleteTrashItems(trashIds) {
      ensureWritable();
      const root = requireRoot(rootPath);
      await invoke<unknown>(
        "library_purge_trash_records",
        request(root, { ids: trashIds.map(Number) }),
      );
    },
    async dispose() {
      if (disposed) return;
      disposed = true;
      importSubscriptionCleanups.forEach((cleanup) => cleanup());
      importSubscriptionCleanups.clear();
      const taskIds = [...activeImportTaskIds];
      activeImportTaskIds.clear();
      await Promise.allSettled(
        taskIds.map((taskId) =>
          commandClient.invoke<unknown>("cancel_media_import", {
            request: { taskId },
          }),
        ),
      );
      rootPath = null;
      workspaceState = { isOpen: false, displayName: null };
    },
  };
}
