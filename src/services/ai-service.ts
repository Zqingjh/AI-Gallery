export type AiProviderKind = "openaiCompatible" | "gemini" | "ollama";

export interface AiProviderCapabilities {
  readonly textClassification: boolean;
  readonly vision: boolean;
}

/** 不包含密钥的 Provider 配置；密钥仅能随单次保存请求提交。 */
export interface AiProvider {
  readonly id: string;
  readonly kind: AiProviderKind;
  readonly displayName: string;
  readonly endpoint: string;
  readonly model: string;
  readonly capabilities: AiProviderCapabilities;
  readonly timeoutMs: number;
  readonly isEnabled: boolean;
  readonly needsCredential: boolean;
}

export interface SaveAiProviderInput {
  readonly id: string | null;
  readonly kind: AiProviderKind;
  readonly displayName: string;
  readonly endpoint: string;
  readonly model: string;
  readonly capabilities: AiProviderCapabilities;
  readonly timeoutMs: number;
  readonly isEnabled: boolean;
  /** 仅限本次提交；实现不得缓存、回传或记录它。 */
  readonly apiKey?: string;
}

export interface AiSendPreview {
  readonly targetType: "asset" | "project";
  readonly targetId: string;
  readonly fields: readonly string[];
  readonly fieldCount: number;
}

export interface CreateAiSuggestionsInput {
  readonly providerId: string;
  readonly targetType: "asset" | "project";
  readonly targetId: string;
}

export interface AiSuggestionTarget {
  readonly type: "asset" | "project";
  readonly id: string;
  readonly title: string;
}

export interface AiSuggestionDimension {
  readonly id: string;
  readonly name: string;
}

export interface AiSuggestionCategory {
  readonly id: string;
  readonly name: string;
}

export interface AiSuggestion {
  readonly id: string;
  readonly target: AiSuggestionTarget;
  readonly dimension: AiSuggestionDimension;
  readonly category: AiSuggestionCategory | null;
  readonly suggestedCategoryName: string | null;
  readonly confidence: number;
  readonly reason: string;
  readonly updatedAt: string;
}

export interface AiSuggestionPage {
  readonly items: readonly AiSuggestion[];
  readonly nextCursor: string | null;
}

export type ResolveAiSuggestionInput =
  | { readonly kind: "accept" }
  | { readonly kind: "acceptExisting"; readonly categoryId: string }
  | { readonly kind: "createCategory"; readonly name: string }
  | { readonly kind: "mergeIntoExisting"; readonly categoryId: string }
  | { readonly kind: "convertToTag"; readonly name: string }
  | { readonly kind: "reject" };

/** UI 与 AI 领域交互的唯一入口，不暴露 Tauri、HTTP 或安全存储细节。 */
export interface AiService {
  listProviders(): Promise<readonly AiProvider[]>;
  saveProvider(input: SaveAiProviderInput): Promise<AiProvider>;
  deleteProvider(id: string): Promise<void>;
  testProvider(id: string): Promise<void>;
  getSendPreview(input: {
    readonly targetType: "asset" | "project";
    readonly targetId: string;
  }): Promise<AiSendPreview>;
  createSuggestions(input: CreateAiSuggestionsInput): Promise<void>;
  listPending(request: {
    readonly cursor?: string;
    readonly limit: number;
  }): Promise<AiSuggestionPage>;
  resolveSuggestion(
    id: string,
    resolution: ResolveAiSuggestionInput,
  ): Promise<void>;
}
