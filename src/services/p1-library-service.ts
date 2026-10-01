export type P1CustomFieldTargetType = "project" | "asset";
export type P1CustomFieldValueType =
  "text" | "number" | "boolean" | "date" | "json";
export type P1EditHistoryTargetType =
  P1CustomFieldTargetType | "prompt" | "custom_field_value";

export interface P1PageCursor {
  readonly updatedAt: number;
  readonly id: number;
}
export interface P1PromptVersionCursor {
  readonly version: number;
  readonly id: number;
}
export interface P1Page<T, Cursor = P1PageCursor> {
  readonly items: readonly T[];
  readonly nextCursor: Cursor | null;
}
export interface P1SavedAssetFilter {
  readonly version: 1;
  readonly keyword?: string;
  readonly mediaType?: "image" | "video";
  readonly model?: string;
  readonly platform?: string;
  readonly categoryIds?: readonly number[];
  readonly rating?: number;
  readonly isFavorite?: boolean;
  readonly isPublic?: boolean;
  readonly createdAfter?: number;
  readonly createdBefore?: number;
  readonly minAspectRatio?: number;
  readonly maxAspectRatio?: number;
}
export interface P1SavedFilter {
  readonly id: number;
  readonly name: string;
  readonly filter: P1SavedAssetFilter;
}
export interface P1CustomField {
  readonly id: number;
  readonly name: string;
  readonly targetType: P1CustomFieldTargetType;
  readonly valueType: P1CustomFieldValueType;
  readonly options: Record<string, unknown>;
  readonly isEnabled: boolean;
}
export interface P1CustomFieldValue {
  readonly id: number;
  readonly fieldId: number;
  readonly targetType: P1CustomFieldTargetType;
  readonly targetId: number;
  readonly value: unknown;
  readonly status: "confirmed" | "pending";
}
export interface P1PromptText {
  readonly promptZh: string;
  readonly promptEn: string;
  readonly negativePrompt: string;
}
export interface P1PromptVersion extends P1PromptText {
  readonly id: number;
  readonly promptId: number;
  readonly version: number;
}
export interface P1EditHistoryEntry {
  readonly id: number;
  readonly targetType: P1EditHistoryTargetType;
  readonly targetId: number;
  readonly action: "create" | "update" | "delete" | "restore" | "bulk_update";
  readonly changedFields: readonly string[];
  readonly status: "confirmed" | "pending";
}
export type P1NullableTextEdit =
  | { readonly action: "keep" | "clear" }
  | { readonly action: "set"; readonly value: string };
export interface P1BulkAssetEditInput {
  readonly assetIds: readonly number[];
  readonly rating?: number;
  readonly isFavorite?: boolean;
  readonly isPublic?: boolean;
  readonly model: P1NullableTextEdit;
  readonly platform: P1NullableTextEdit;
  readonly addCategoryIds: readonly number[];
  readonly removeCategoryIds: readonly number[];
  readonly addTagIds: readonly number[];
  readonly removeTagIds: readonly number[];
}
export interface P1BulkAssetEditPreview {
  readonly targetCount: number;
  readonly categoryRelationsToAdd: number;
  readonly categoryRelationsToRemove: number;
  readonly tagRelationsToAdd: number;
  readonly tagRelationsToRemove: number;
}
export interface P1ComparisonAsset {
  readonly id: number;
  readonly fileName: string;
  readonly modelName: string;
  readonly platformName: string;
  readonly updatedAt: string;
}
export interface P1BatchAiPreview {
  readonly targetCount: number;
  readonly fieldNames: readonly string[];
  readonly taxonomyDimensionCount: number;
}
export interface P1BatchAiResult {
  readonly created: number;
  readonly failed: number;
  readonly failureKinds: readonly P1BatchAiFailureKind[];
}
export type P1BatchAiFailureKind =
  | "input_unavailable"
  | "request_failed"
  | "response_invalid"
  | "persist_failed";
/** 批量 AI 发送范围；只有用户勾选的字段才会发送给 Provider。 */
export interface P1BatchAiInputScope {
  readonly title: boolean;
  readonly promptZh: boolean;
  readonly promptEn: boolean;
  readonly negativePrompt: boolean;
}

export interface P1BatchAiInput {
  readonly providerId: number;
  readonly assetIds: readonly number[];
  readonly inputScope: P1BatchAiInputScope;
}

export interface P1LibraryService {
  previewBulkAssetEdit(
    input: P1BulkAssetEditInput,
  ): Promise<P1BulkAssetEditPreview>;
  bulkEditAssets(input: P1BulkAssetEditInput): Promise<P1BulkAssetEditPreview>;
  listModelComparison(input: {
    readonly kind: "project" | "matchingPrompt";
    readonly id: number;
    readonly cursor?: P1PageCursor;
  }): Promise<P1Page<P1ComparisonAsset>>;
  previewBatchAi(input: P1BatchAiInput): Promise<P1BatchAiPreview>;
  createBatchAi(input: P1BatchAiInput): Promise<P1BatchAiResult>;
  listSavedFilters(cursor?: P1PageCursor): Promise<P1Page<P1SavedFilter>>;
  saveSavedFilter(input: {
    readonly id?: number;
    readonly name: string;
    readonly filter: P1SavedAssetFilter;
  }): Promise<P1SavedFilter>;
  deleteSavedFilter(id: number): Promise<void>;
  listCustomFields(
    targetType: P1CustomFieldTargetType,
    cursor?: P1PageCursor,
  ): Promise<P1Page<P1CustomField>>;
  saveCustomField(input: {
    readonly name: string;
    readonly targetType: P1CustomFieldTargetType;
    readonly valueType: P1CustomFieldValueType;
    readonly options: Record<string, unknown>;
    readonly isEnabled: boolean;
  }): Promise<P1CustomField>;
  listCustomFieldValues(input: {
    readonly targetType: P1CustomFieldTargetType;
    readonly targetId: number;
    readonly cursor?: P1PageCursor;
  }): Promise<P1Page<P1CustomFieldValue>>;
  saveCustomFieldValue(input: {
    readonly fieldId: number;
    readonly targetType: P1CustomFieldTargetType;
    readonly targetId: number;
    readonly value: unknown;
  }): Promise<P1CustomFieldValue>;
  confirmPendingCustomFieldValue(id: number): Promise<P1CustomFieldValue>;
  listPromptVersions(
    promptId: number,
    cursor?: P1PromptVersionCursor,
  ): Promise<P1Page<P1PromptVersion, P1PromptVersionCursor>>;
  createPromptVersion(
    promptId: number,
    prompt: P1PromptText,
  ): Promise<P1PromptVersion>;
  listEditHistory(input: {
    readonly targetType: P1EditHistoryTargetType;
    readonly targetId: number;
    readonly cursor?: P1PageCursor;
  }): Promise<P1Page<P1EditHistoryEntry>>;
}
