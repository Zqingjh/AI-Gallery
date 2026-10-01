import type { CommandClient } from "./command-client";
import type { WorkspaceAccessMode } from "./workspace-management-service";
import {
  ServiceError,
  serviceErrorCode,
  toServiceError,
} from "./service-error";
import type {
  AiProvider,
  AiProviderCapabilities,
  AiProviderKind,
  AiSendPreview,
  AiService,
  AiSuggestion,
  AiSuggestionPage,
  CreateAiSuggestionsInput,
  ResolveAiSuggestionInput,
  SaveAiProviderInput,
} from "./ai-service";

type UnknownRecord = Record<string, unknown>;

const inputScope = {
  title: true,
  promptZh: true,
  promptEn: true,
  negativePrompt: true,
} as const;

function invalidResponse(): ServiceError {
  return new ServiceError(
    serviceErrorCode.invalidResponse,
    "桌面服务返回了无法识别的 AI 数据。",
  );
}

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

function boolean(value: unknown): boolean {
  if (typeof value !== "boolean") throw invalidResponse();
  return value;
}

function number(value: unknown): number {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    throw invalidResponse();
  }
  return value;
}

function nullableString(value: unknown): string | null {
  return value === null ? null : string(value);
}

function requireWorkspaceRoot(getWorkspaceRoot: () => string | null): string {
  const rootPath = getWorkspaceRoot();
  if (!rootPath) {
    throw new ServiceError(
      serviceErrorCode.commandFailed,
      "请先连接工作区后再使用 AI 功能。",
    );
  }
  return rootPath;
}

function providerKind(value: unknown): AiProviderKind {
  const kind = string(value);
  if (kind === "openaiCompatible" || kind === "gemini" || kind === "ollama") {
    return kind;
  }
  throw invalidResponse();
}

function providerKindInput(value: AiProviderKind): string {
  return value === "openaiCompatible" ? "openai_compatible" : value;
}

function capabilities(value: unknown): AiProviderCapabilities {
  const raw = record(value);
  return {
    textClassification: boolean(raw.textClassification),
    vision: boolean(raw.vision),
  };
}

function provider(value: unknown): AiProvider {
  const raw = record(value);
  return {
    id: string(raw.id),
    kind: providerKind(raw.kind),
    displayName: string(raw.displayName),
    endpoint: string(raw.endpoint),
    model: string(raw.model),
    capabilities: capabilities(raw.capabilities),
    timeoutMs: number(raw.timeoutMs),
    isEnabled: boolean(raw.isEnabled),
    needsCredential: boolean(raw.needsCredential),
  };
}

function targetType(value: unknown): "asset" | "project" {
  const type = string(value);
  if (type === "asset" || type === "project") return type;
  throw invalidResponse();
}

function suggestion(value: unknown): AiSuggestion {
  const raw = record(value);
  const rawTarget = record(raw.target);
  const rawDimension = record(raw.dimension);
  const rawCategory = raw.category === null ? null : record(raw.category);
  const confidence = number(raw.confidence);
  if (confidence < 0 || confidence > 1) throw invalidResponse();
  return {
    id: string(raw.id),
    target: {
      type: targetType(rawTarget.type),
      id: string(rawTarget.id),
      title: string(rawTarget.title),
    },
    dimension: { id: string(rawDimension.id), name: string(rawDimension.name) },
    category: rawCategory
      ? { id: string(rawCategory.id), name: string(rawCategory.name) }
      : null,
    suggestedCategoryName: nullableString(raw.suggestedCategoryName),
    confidence,
    reason: string(raw.reason),
    updatedAt: string(raw.updatedAt),
  };
}

function preview(value: unknown): AiSendPreview {
  const raw = record(value);
  const fields = array(raw.fields).map(string);
  const fieldCount = number(raw.fieldCount);
  if (
    fieldCount !== fields.length ||
    fields.some((field) => !(field in inputScope))
  ) {
    throw invalidResponse();
  }
  return {
    targetType: targetType(raw.targetType),
    targetId: string(raw.targetId),
    fields,
    fieldCount,
  };
}

function resolution(input: ResolveAiSuggestionInput): UnknownRecord {
  switch (input.kind) {
    case "accept":
      return { kind: input.kind };
    case "acceptExisting":
      return { kind: input.kind, categoryId: input.categoryId };
    case "createCategory":
      return { kind: input.kind, name: input.name };
    case "mergeIntoExisting":
      return { kind: input.kind, categoryId: input.categoryId };
    case "convertToTag":
      return { kind: input.kind, name: input.name };
    case "reject":
      return { kind: input.kind };
  }
}

export function createTauriAiService(
  client: CommandClient,
  getWorkspaceRoot: () => string | null,
  getAccessMode: () => WorkspaceAccessMode | null = () => null,
): AiService {
  const request = (fields: UnknownRecord = {}): UnknownRecord => ({
    request: { rootPath: requireWorkspaceRoot(getWorkspaceRoot), ...fields },
  });
  const invoke = async <TResult>(
    command: string,
    fields?: UnknownRecord,
  ): Promise<TResult> => {
    try {
      return await client.invoke<TResult>(command, request(fields));
    } catch (error) {
      throw toServiceError(error);
    }
  };
  const ensureWritable = (): void => {
    if (getAccessMode() === "readOnly") {
      throw new ServiceError(
        "WORKSPACE_READ_ONLY",
        "工作区正处于只读展示模式，无法修改内容。",
      );
    }
  };

  return {
    async listProviders() {
      return array(await invoke<unknown>("ai_list_providers")).map(provider);
    },
    async saveProvider(input: SaveAiProviderInput) {
      ensureWritable();
      const { apiKey, ...config } = input;
      const result = await invoke<unknown>("ai_save_provider", {
        id: config.id === null ? null : Number(config.id),
        input: {
          kind: providerKindInput(config.kind),
          displayName: config.displayName,
          endpoint: config.endpoint,
          model: config.model,
          capabilities: {
            classification: config.capabilities.textClassification,
          },
          timeoutMs: config.timeoutMs,
          isEnabled: config.isEnabled,
        },
        ...(apiKey ? { apiKey } : {}),
      });
      return provider(result);
    },
    async deleteProvider(id: string) {
      ensureWritable();
      await invoke("ai_delete_provider", { id: Number(id) });
    },
    async testProvider(id: string) {
      ensureWritable();
      await invoke("ai_test_provider_connection", { id: Number(id) });
    },
    async getSendPreview(input) {
      return preview(
        await invoke<unknown>("ai_get_send_preview", {
          targetType: input.targetType,
          targetId: input.targetId,
          inputScope,
        }),
      );
    },
    async createSuggestions(input: CreateAiSuggestionsInput) {
      ensureWritable();
      await invoke("ai_create_classification_suggestions", {
        providerId: input.providerId,
        targetType: input.targetType,
        targetId: input.targetId,
        inputScope,
      });
    },
    async listPending({ cursor, limit }) {
      const raw = record(
        await invoke<unknown>("ai_list_pending_suggestions", {
          cursor,
          limit,
        }),
      );
      return {
        items: array(raw.items).map(suggestion),
        nextCursor: raw.nextCursor === null ? null : string(raw.nextCursor),
      } satisfies AiSuggestionPage;
    },
    async resolveSuggestion(id, input) {
      ensureWritable();
      await invoke("ai_resolve_suggestion", {
        id: Number(id),
        resolution: resolution(input),
      });
    },
  };
}
