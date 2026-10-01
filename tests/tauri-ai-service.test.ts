import { describe, expect, it, vi } from "vitest";
import type { CommandClient } from "../src/services/command-client";
import { serviceErrorCode } from "../src/services/service-error";
import { createTauriAiService } from "../src/services/tauri-ai-service";

const workspaceRoot = "D:\\Private\\AI-Gallery";
const apiKey = "secret-key-must-not-return";

function client(): {
  readonly client: CommandClient;
  readonly invoke: ReturnType<typeof vi.fn>;
} {
  const invoke = vi.fn();
  return { client: { invoke }, invoke };
}

function providerResponse(overrides: Record<string, unknown> = {}) {
  return {
    id: "provider-1",
    kind: "openaiCompatible",
    displayName: "本地代理",
    endpoint: "https://example.test/v1",
    model: "gpt-test",
    capabilities: { textClassification: true, vision: false },
    timeoutMs: 30_000,
    isEnabled: true,
    needsCredential: false,
    ...overrides,
  };
}

describe("Tauri AI 服务", () => {
  it("前端服务在只读模式下不发起 AI 管理写入", async () => {
    const { client: commandClient, invoke } = client();
    const service = createTauriAiService(
      commandClient,
      () => workspaceRoot,
      () => "readOnly",
    );

    await expect(service.deleteProvider("provider-1")).rejects.toMatchObject({
      code: "WORKSPACE_READ_ONLY",
    });
    expect(invoke).not.toHaveBeenCalled();
  });

  it("删除和连通性测试将服务 ID 作为数值传给命令", async () => {
    const { client: commandClient, invoke } = client();
    invoke.mockResolvedValue(undefined);
    const service = createTauriAiService(commandClient, () => workspaceRoot);

    await service.deleteProvider("12");
    await service.testProvider("12");

    expect(invoke).toHaveBeenNthCalledWith(1, "ai_delete_provider", {
      request: { rootPath: workspaceRoot, id: 12 },
    });
    expect(invoke).toHaveBeenNthCalledWith(2, "ai_test_provider_connection", {
      request: { rootPath: workspaceRoot, id: 12 },
    });
  });

  it("仅在单次保存请求中传递密钥，返回的 Provider 不会包含密钥", async () => {
    const { client: commandClient, invoke } = client();
    invoke.mockResolvedValue(providerResponse());
    const service = createTauriAiService(commandClient, () => workspaceRoot);

    const saved = await service.saveProvider({
      id: null,
      kind: "openaiCompatible",
      displayName: "本地代理",
      endpoint: "https://example.test/v1",
      model: "gpt-test",
      capabilities: { textClassification: true, vision: false },
      timeoutMs: 30_000,
      isEnabled: true,
      apiKey,
    });

    expect(invoke).toHaveBeenCalledWith("ai_save_provider", {
      request: expect.objectContaining({
        rootPath: workspaceRoot,
        apiKey,
        input: expect.not.objectContaining({ apiKey: expect.anything() }),
      }),
    });
    expect(invoke.mock.calls[0]?.[1]).toMatchObject({
      request: {
        input: {
          kind: "openai_compatible",
          capabilities: { classification: true },
        },
      },
    });
    expect(JSON.stringify(saved)).not.toContain(apiKey);
    expect(saved).not.toHaveProperty("apiKey");
  });

  it("预览和创建建议只发送受限字段范围，不发送提示词内容或媒体", async () => {
    const { client: commandClient, invoke } = client();
    invoke
      .mockResolvedValueOnce({
        targetType: "asset",
        targetId: "7",
        fields: ["title", "promptZh"],
        fieldCount: 2,
      })
      .mockResolvedValueOnce(undefined);
    const service = createTauriAiService(commandClient, () => workspaceRoot);

    await expect(
      service.getSendPreview({ targetType: "asset", targetId: "7" }),
    ).resolves.toMatchObject({ fields: ["title", "promptZh"] });
    await service.createSuggestions({
      providerId: "provider-1",
      targetType: "asset",
      targetId: "7",
    });

    expect(invoke).toHaveBeenNthCalledWith(1, "ai_get_send_preview", {
      request: {
        rootPath: workspaceRoot,
        targetType: "asset",
        targetId: "7",
        inputScope: {
          title: true,
          promptZh: true,
          promptEn: true,
          negativePrompt: true,
        },
      },
    });
    expect(invoke).toHaveBeenNthCalledWith(
      2,
      "ai_create_classification_suggestions",
      expect.objectContaining({
        request: expect.not.objectContaining({
          promptZh: expect.anything(),
          storedPath: expect.anything(),
          media: expect.anything(),
        }),
      }),
    );
  });

  it("待审核建议使用分页 DTO，并将处理动作转换为稳定请求", async () => {
    const { client: commandClient, invoke } = client();
    invoke
      .mockResolvedValueOnce({
        items: [
          {
            id: "11",
            target: { type: "asset", id: "7", title: "城市" },
            dimension: { id: "2", name: "视觉风格" },
            category: null,
            suggestedCategoryName: "赛博朋克",
            confidence: 0.82,
            reason: "文本中的风格词与分类一致",
            updatedAt: "2026-07-14T10:00:00.000Z",
          },
        ],
        nextCursor: "10:1",
      })
      .mockResolvedValueOnce(undefined);
    const service = createTauriAiService(commandClient, () => workspaceRoot);

    const page = await service.listPending({ limit: 24 });
    await service.resolveSuggestion("11", {
      kind: "convertToTag",
      name: "未来城市",
    });

    expect(page.nextCursor).toBe("10:1");
    expect(invoke).toHaveBeenLastCalledWith("ai_resolve_suggestion", {
      request: {
        rootPath: workspaceRoot,
        id: 11,
        resolution: { kind: "convertToTag", name: "未来城市" },
      },
    });
  });

  it("直接接受时由后端按已保存的建议决定复用或创建分类", async () => {
    const { client: commandClient, invoke } = client();
    invoke.mockResolvedValueOnce(undefined);
    const service = createTauriAiService(commandClient, () => workspaceRoot);

    await service.resolveSuggestion("11", { kind: "accept" });

    expect(invoke).toHaveBeenLastCalledWith("ai_resolve_suggestion", {
      request: {
        rootPath: workspaceRoot,
        id: 11,
        resolution: { kind: "accept" },
      },
    });
  });

  it("未连接工作区时在发送 IPC 前失败", async () => {
    const { client: commandClient, invoke } = client();
    const service = createTauriAiService(commandClient, () => null);

    await expect(service.listProviders()).rejects.toMatchObject({
      code: serviceErrorCode.commandFailed,
    });
    expect(invoke).not.toHaveBeenCalled();
  });
});
