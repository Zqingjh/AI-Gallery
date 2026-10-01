import type { CommandClient } from "./command-client";
import type {
  DuplicateAssetGroup,
  DuplicateGroupPageRequest,
  PageResult,
} from "./library-service";
import type {
  AssetCover,
  AssetPathRepairPreview,
  MediaIntegrityService,
  SelectedLocalFile,
  VideoKeyFramePreview,
} from "./media-integrity-service";
import {
  ServiceError,
  serviceErrorCode,
  toServiceError,
} from "./service-error";

type UnknownRecord = Record<string, unknown>;

interface RawDuplicateCursor {
  readonly updatedAt: number;
  readonly contentHash: string;
}

const MAX_PREVIEW_BYTES = 10 * 1024 * 1024;

function invalidResponse(): ServiceError {
  return new ServiceError(
    serviceErrorCode.invalidResponse,
    "桌面服务返回了无法识别的媒体完整性数据。",
  );
}

function record(value: unknown): UnknownRecord {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
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

function integer(value: unknown): number {
  if (typeof value !== "number" || !Number.isSafeInteger(value)) {
    throw invalidResponse();
  }
  return value;
}

function nonNegativeInteger(value: unknown): number {
  const parsed = integer(value);
  if (parsed < 0) throw invalidResponse();
  return parsed;
}

function nullableNonNegativeInteger(value: unknown): number | null {
  return value === null ? null : nonNegativeInteger(value);
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

function encodeDuplicateCursor(value: unknown): string | null {
  if (value === null) return null;
  const cursor = record(value);
  return `${integer(cursor.updatedAt)}:${encodeURIComponent(string(cursor.contentHash))}`;
}

function isoTimestamp(value: unknown): string {
  const timestamp = integer(value);
  const date = new Date(timestamp);
  if (Number.isNaN(date.valueOf())) throw invalidResponse();
  return date.toISOString();
}

function duplicateGroup(value: unknown): DuplicateAssetGroup {
  const raw = record(value);
  return {
    contentHash: string(raw.contentHash),
    assetCount: nonNegativeInteger(raw.assetCount),
    representativeAssetId: String(integer(raw.representativeAssetId)),
    representativeFileName: string(raw.representativeFileName),
    updatedAt: isoTimestamp(raw.updatedAt),
  };
}

function duplicatePage(value: unknown): PageResult<DuplicateAssetGroup> {
  const raw = record(value);
  return {
    items: array(raw.items).map(duplicateGroup),
    nextCursor: encodeDuplicateCursor(raw.nextCursor),
  };
}

function imagePreview(value: UnknownRecord): {
  readonly previewUrl: string | null;
  dispose(): void;
} {
  if (value.bytes === undefined || value.bytes === null) {
    return { previewUrl: null, dispose() {} };
  }
  const mimeType = string(value.mimeType);
  if (!/^image\/(png|jpe?g|webp)$/i.test(mimeType)) throw invalidResponse();
  const source = array(value.bytes);
  if (source.length > MAX_PREVIEW_BYTES) throw invalidResponse();
  const bytes = Uint8Array.from(
    source.map((item) => {
      const byte = integer(item);
      if (byte < 0 || byte > 255) throw invalidResponse();
      return byte;
    }),
  );
  const previewUrl = URL.createObjectURL(
    new Blob([bytes.buffer], { type: mimeType }),
  );
  let active = true;
  return {
    previewUrl,
    dispose() {
      if (!active) return;
      active = false;
      URL.revokeObjectURL(previewUrl);
    },
  };
}

function assetCover(value: unknown): AssetCover {
  const raw = record(value);
  const sourceType = string(raw.sourceType);
  if (sourceType !== "keyFrame" && sourceType !== "custom") {
    throw invalidResponse();
  }
  return {
    assetId: integer(raw.assetId),
    sourceType,
    frameTimestampMs: nullableNonNegativeInteger(raw.frameTimestampMs),
    ...imagePreview(raw),
  };
}

function keyFramePreview(value: unknown): VideoKeyFramePreview {
  const raw = record(value);
  return {
    assetId: integer(raw.assetId),
    frameTimestampMs: nonNegativeInteger(raw.frameTimestampMs),
    ...imagePreview(raw),
  };
}

function pathRepairPreview(value: unknown): AssetPathRepairPreview {
  const raw = record(value);
  const pathKind = string(raw.pathKind);
  const verification = string(raw.verification);
  if (pathKind !== "managed" && pathKind !== "external") {
    throw invalidResponse();
  }
  if (verification !== "matched" && verification !== "unverified") {
    throw invalidResponse();
  }
  return {
    assetId: integer(raw.assetId),
    fileName: string(raw.fileName),
    pathKind,
    fileSize: nonNegativeInteger(raw.fileSize),
    verification,
  };
}

function selectedFile(path: string): SelectedLocalFile {
  const segments = path.split(/[\\/]/u);
  return { path, fileName: segments.at(-1) || "已选择文件" };
}

async function selectOne(options: {
  readonly name: string;
  readonly extensions?: readonly string[];
}): Promise<SelectedLocalFile | null> {
  const { open } = await import("@tauri-apps/plugin-dialog");
  const selected = await open({
    directory: false,
    multiple: false,
    ...(options.extensions
      ? {
          filters: [
            { name: options.name, extensions: [...options.extensions] },
          ],
        }
      : {}),
  });
  return typeof selected === "string" ? selectedFile(selected) : null;
}

export function createTauriMediaIntegrityService(
  client: CommandClient,
  getRoot: () => string | null,
): MediaIntegrityService {
  const invoke = async (
    command: string,
    fields: UnknownRecord = {},
  ): Promise<unknown> => {
    const rootPath = getRoot();
    if (!rootPath) {
      throw new ServiceError(
        serviceErrorCode.commandFailed,
        "请先连接工作区。",
      );
    }
    try {
      return await client.invoke(command, { request: { rootPath, ...fields } });
    } catch (error) {
      throw toServiceError(error);
    }
  };

  return {
    async listDuplicateGroups(request: DuplicateGroupPageRequest) {
      return duplicatePage(
        await invoke("library_list_duplicate_groups", {
          cursor: decodeDuplicateCursor(request.cursor),
          limit: request.limit,
        }),
      );
    },
    async getAssetCover(assetId) {
      const response = await invoke("get_asset_cover", { assetId });
      return response === null ? null : assetCover(response);
    },
    selectFfmpegExecutable() {
      return selectOne({ name: "FFmpeg 可执行文件", extensions: ["exe"] });
    },
    selectVideoFile() {
      return selectOne({ name: "视频", extensions: ["mp4"] });
    },
    selectCustomCover() {
      return selectOne({
        name: "封面图片",
        extensions: ["png", "jpg", "jpeg", "webp"],
      });
    },
    selectRepairCandidate() {
      return selectOne({ name: "媒体文件" });
    },
    async previewVideoKeyFrame(input) {
      return keyFramePreview(await invoke("preview_video_key_frame", input));
    },
    async setVideoKeyFrameCover(input) {
      return assetCover(
        await invoke("set_video_key_frame_cover", {
          ...input,
          confirmed: true,
        }),
      );
    },
    async generateDefaultVideoCover(assetId) {
      return assetCover(
        await invoke("set_default_video_cover", { assetId, confirmed: true }),
      );
    },
    async setCustomVideoCover(input) {
      return assetCover(
        await invoke("set_custom_video_cover", {
          ...input,
          confirmed: true,
        }),
      );
    },
    async removeVideoCover(assetId) {
      await invoke("remove_video_cover", { assetId, confirmed: true });
    },
    async previewAssetPathRepair(input) {
      return pathRepairPreview(
        await invoke("preview_asset_path_repair", input),
      );
    },
    async executeAssetPathRepair(input) {
      return pathRepairPreview(
        await invoke("execute_asset_path_repair", {
          ...input,
          confirmed: true,
        }),
      );
    },
  };
}
