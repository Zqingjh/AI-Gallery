import type { CommandClient } from "./command-client";
import {
  ServiceError,
  serviceErrorCode,
  toServiceError,
} from "./service-error";
import type {
  P1CustomField,
  P1CustomFieldTargetType,
  P1CustomFieldValue,
  P1EditHistoryEntry,
  P1EditHistoryTargetType,
  P1LibraryService,
  P1BulkAssetEditInput,
  P1BulkAssetEditPreview,
  P1BatchAiFailureKind,
  P1BatchAiPreview,
  P1ComparisonAsset,
  P1Page,
  P1PageCursor,
  P1PromptVersion,
  P1PromptVersionCursor,
  P1SavedFilter,
  P1SavedAssetFilter,
} from "./p1-library-service";

type UnknownRecord = Record<string, unknown>;
const record = (value: unknown): UnknownRecord => {
  if (!value || typeof value !== "object" || Array.isArray(value))
    throw invalid();
  return value as UnknownRecord;
};
const string = (value: unknown): string => {
  if (typeof value !== "string") throw invalid();
  return value;
};
const number = (value: unknown): number => {
  if (typeof value !== "number" || !Number.isSafeInteger(value))
    throw invalid();
  return value;
};
const finiteNumber = (value: unknown): number => {
  if (typeof value !== "number" || !Number.isFinite(value)) throw invalid();
  return value;
};
const boolean = (value: unknown): boolean => {
  if (typeof value !== "boolean") throw invalid();
  return value;
};
const array = (value: unknown): readonly unknown[] => {
  if (!Array.isArray(value)) throw invalid();
  return value;
};
const invalid = () =>
  new ServiceError(
    serviceErrorCode.invalidResponse,
    "桌面服务返回了无法识别的 P1 数据。",
  );
const objectValue = (value: unknown): UnknownRecord => record(value);

function page<T, Cursor>(
  value: unknown,
  parse: (item: unknown) => T,
  parseCursor: (cursor: unknown) => Cursor,
): P1Page<T, Cursor> {
  const raw = record(value);
  return {
    items: array(raw.items).map(parse),
    nextCursor:
      raw.nextCursor === null || raw.nextCursor === undefined
        ? null
        : parseCursor(raw.nextCursor),
  };
}
function pageCursor(value: unknown): P1PageCursor {
  const raw = record(value);
  return { updatedAt: number(raw.updatedAt), id: number(raw.id) };
}
function promptVersionCursor(value: unknown): P1PromptVersionCursor {
  const raw = record(value);
  return { version: number(raw.version), id: number(raw.id) };
}
const targetType = (value: unknown): P1CustomFieldTargetType => {
  const raw = string(value);
  if (raw !== "asset" && raw !== "project") throw invalid();
  return raw;
};
const historyTarget = (value: unknown): P1EditHistoryTargetType => {
  const raw = string(value);
  if (
    raw !== "asset" &&
    raw !== "project" &&
    raw !== "prompt" &&
    raw !== "custom_field_value"
  )
    throw invalid();
  return raw;
};
function savedFilter(value: unknown): P1SavedFilter {
  const raw = record(value);
  const source = record(raw.filter);
  const allowed = new Set([
    "version",
    "keyword",
    "mediaType",
    "model",
    "platform",
    "categoryIds",
    "rating",
    "isFavorite",
    "isPublic",
    "createdAfter",
    "createdBefore",
    "minAspectRatio",
    "maxAspectRatio",
  ]);
  if (Object.keys(source).some((key) => !allowed.has(key))) throw invalid();
  if (number(source.version) !== 1) throw invalid();
  const optionalString = (key: string) =>
    source[key] === undefined ? undefined : string(source[key]);
  const optionalBoolean = (key: string) =>
    source[key] === undefined ? undefined : boolean(source[key]);
  const optionalInteger = (key: string) =>
    source[key] === undefined ? undefined : number(source[key]);
  const optionalFinite = (key: string) =>
    source[key] === undefined ? undefined : finiteNumber(source[key]);
  const mediaType = optionalString("mediaType");
  if (mediaType !== undefined && mediaType !== "image" && mediaType !== "video")
    throw invalid();
  const categoryIds =
    source.categoryIds === undefined
      ? undefined
      : array(source.categoryIds).map((id) => {
          const parsed = number(id);
          if (parsed <= 0) throw invalid();
          return parsed;
        });
  const rating = optionalInteger("rating");
  if (rating !== undefined && (rating < 1 || rating > 5)) throw invalid();
  const isFavorite = optionalBoolean("isFavorite");
  const isPublic = optionalBoolean("isPublic");
  const createdAfter = optionalInteger("createdAfter");
  const createdBefore = optionalInteger("createdBefore");
  const minAspectRatio = optionalFinite("minAspectRatio");
  const maxAspectRatio = optionalFinite("maxAspectRatio");
  const filter: P1SavedAssetFilter = {
    version: 1,
    ...(optionalString("keyword") !== undefined
      ? { keyword: optionalString("keyword") }
      : {}),
    ...(mediaType !== undefined ? { mediaType } : {}),
    ...(optionalString("model") !== undefined
      ? { model: optionalString("model") }
      : {}),
    ...(optionalString("platform") !== undefined
      ? { platform: optionalString("platform") }
      : {}),
    ...(categoryIds !== undefined ? { categoryIds } : {}),
    ...(rating !== undefined ? { rating } : {}),
    ...(isFavorite !== undefined ? { isFavorite } : {}),
    ...(isPublic !== undefined ? { isPublic } : {}),
    ...(createdAfter !== undefined ? { createdAfter } : {}),
    ...(createdBefore !== undefined ? { createdBefore } : {}),
    ...(minAspectRatio !== undefined ? { minAspectRatio } : {}),
    ...(maxAspectRatio !== undefined ? { maxAspectRatio } : {}),
  };
  return {
    id: number(raw.id),
    name: string(raw.name),
    filter,
  };
}
function customField(value: unknown): P1CustomField {
  const raw = record(value);
  const valueType = string(raw.valueType);
  if (!["text", "number", "boolean", "date", "json"].includes(valueType))
    throw invalid();
  return {
    id: number(raw.id),
    name: string(raw.name),
    targetType: targetType(raw.targetType),
    valueType: valueType as P1CustomField["valueType"],
    options: objectValue(raw.options),
    isEnabled: boolean(raw.isEnabled),
  };
}
function customValue(value: unknown): P1CustomFieldValue {
  const raw = record(value);
  const status = string(raw.status);
  if (status !== "confirmed" && status !== "pending") throw invalid();
  return {
    id: number(raw.id),
    fieldId: number(raw.fieldId),
    targetType: targetType(raw.targetType),
    targetId: number(raw.targetId),
    value: raw.value,
    status,
  };
}
function promptVersion(value: unknown): P1PromptVersion {
  const raw = record(value);
  return {
    id: number(raw.id),
    promptId: number(raw.promptId),
    version: number(raw.version),
    promptZh: string(raw.promptZh),
    promptEn: string(raw.promptEn),
    negativePrompt: string(raw.negativePrompt),
  };
}
function history(value: unknown): P1EditHistoryEntry {
  const raw = record(value);
  const action = string(raw.action);
  if (
    !["create", "update", "delete", "restore", "bulk_update"].includes(action)
  )
    throw invalid();
  const status = string(raw.status);
  if (status !== "confirmed" && status !== "pending") throw invalid();
  return {
    id: number(raw.id),
    targetType: historyTarget(raw.targetType),
    targetId: number(raw.targetId),
    action: action as P1EditHistoryEntry["action"],
    changedFields: array(raw.changedFields).map(string),
    status,
  };
}
function bulkPreview(value: unknown): P1BulkAssetEditPreview {
  const raw = record(value);
  return {
    targetCount: number(raw.targetCount),
    categoryRelationsToAdd: number(raw.categoryRelationsToAdd),
    categoryRelationsToRemove: number(raw.categoryRelationsToRemove),
    tagRelationsToAdd: number(raw.tagRelationsToAdd),
    tagRelationsToRemove: number(raw.tagRelationsToRemove),
  };
}
function comparison(value: unknown): P1ComparisonAsset {
  const raw = record(value);
  const model = raw.model;
  const platform = raw.platform;
  return {
    id: number(raw.id),
    fileName: string(raw.fileName),
    modelName: model === null ? "" : string(model),
    platformName: platform === null ? "" : string(platform),
    updatedAt: new Date(number(raw.updatedAt)).toISOString(),
  };
}
function batchAiPreview(value: unknown): P1BatchAiPreview {
  const raw = record(value);
  return {
    targetCount: number(raw.targetCount),
    fieldNames: array(raw.fieldNames).map(string),
    taxonomyDimensionCount: number(raw.taxonomyDimensionCount),
  };
}
function batchAiFailureKind(value: unknown): P1BatchAiFailureKind {
  const kind = string(value);
  if (
    kind !== "input_unavailable" &&
    kind !== "request_failed" &&
    kind !== "response_invalid" &&
    kind !== "persist_failed"
  )
    throw invalid();
  return kind;
}
const bulkInput = (input: P1BulkAssetEditInput): UnknownRecord => ({
  ...input,
  assetIds: [...input.assetIds],
  addCategoryIds: [...input.addCategoryIds],
  removeCategoryIds: [...input.removeCategoryIds],
  addTagIds: [...input.addTagIds],
  removeTagIds: [...input.removeTagIds],
});

export function createTauriP1LibraryService(
  client: CommandClient,
  getRoot: () => string | null,
): P1LibraryService {
  const invoke = async (
    command: string,
    fields: UnknownRecord = {},
  ): Promise<unknown> => {
    const rootPath = getRoot();
    if (!rootPath)
      throw new ServiceError(
        serviceErrorCode.commandFailed,
        "请先连接工作区。",
      );
    try {
      return await client.invoke(command, { request: { rootPath, ...fields } });
    } catch (error) {
      throw toServiceError(error);
    }
  };
  return {
    async previewBulkAssetEdit(input) {
      return bulkPreview(
        await invoke("library_preview_bulk_asset_edit", {
          input: bulkInput(input),
        }),
      );
    },
    async bulkEditAssets(input) {
      return bulkPreview(
        await invoke("library_bulk_edit_assets", {
          input: bulkInput(input),
          confirmed: true,
        }),
      );
    },
    async listModelComparison(input) {
      const scope =
        input.kind === "project"
          ? { kind: "project", projectId: input.id }
          : { kind: "matchingPrompt", baselineAssetId: input.id };
      return page(
        await invoke("library_list_model_comparison", {
          scope,
          cursor: input.cursor ?? null,
          limit: 50,
        }),
        comparison,
        pageCursor,
      );
    },
    async previewBatchAi(input) {
      return batchAiPreview(
        await invoke("ai_get_batch_send_preview", {
          assetDisplayNumbers: [...input.assetIds],
          inputScope: input.inputScope,
        }),
      );
    },
    async createBatchAi(input) {
      const raw = record(
        await invoke("ai_create_classification_suggestions_batch", {
          providerId: input.providerId,
          assetDisplayNumbers: [...input.assetIds],
          inputScope: input.inputScope,
          confirmed: true,
        }),
      );
      const failures = array(raw.failed).map((item) =>
        batchAiFailureKind(record(item).kind),
      );
      return {
        created: array(raw.created).length,
        failed: failures.length,
        failureKinds: failures,
      };
    },
    async listSavedFilters(cursor) {
      return page(
        await invoke("p1_library_list_saved_filters", {
          cursor: cursor ?? null,
          limit: 50,
        }),
        savedFilter,
        pageCursor,
      );
    },
    async saveSavedFilter(input) {
      return savedFilter(
        await invoke("p1_library_save_saved_filter", { input }),
      );
    },
    async deleteSavedFilter(id) {
      await invoke("p1_library_delete_saved_filter", { id });
    },
    async listCustomFields(type, cursor) {
      return page(
        await invoke("p1_library_list_custom_fields", {
          targetType: type,
          cursor: cursor ?? null,
          limit: 50,
        }),
        customField,
        pageCursor,
      );
    },
    async saveCustomField(input) {
      return customField(
        await invoke("p1_library_save_custom_field", {
          input: { ...input, id: null },
        }),
      );
    },
    async listCustomFieldValues(input) {
      return page(
        await invoke("p1_library_list_custom_field_values", {
          ...input,
          cursor: input.cursor ?? null,
          limit: 50,
        }),
        customValue,
        pageCursor,
      );
    },
    async saveCustomFieldValue(input) {
      return customValue(
        await invoke("p1_library_save_manual_custom_field_value", { input }),
      );
    },
    async confirmPendingCustomFieldValue(id) {
      return customValue(
        await invoke("p1_library_confirm_pending_custom_field_value", { id }),
      );
    },
    async listPromptVersions(promptId, cursor) {
      return page(
        await invoke("p1_library_list_prompt_versions", {
          promptId,
          cursor: cursor ?? null,
          limit: 50,
        }),
        promptVersion,
        promptVersionCursor,
      );
    },
    async createPromptVersion(promptId, prompt) {
      return promptVersion(
        await invoke("p1_library_create_prompt_version", { promptId, prompt }),
      );
    },
    async listEditHistory(input) {
      return page(
        await invoke("p1_library_list_edit_history", {
          ...input,
          cursor: input.cursor ?? null,
          limit: 50,
        }),
        history,
        pageCursor,
      );
    },
  };
}
