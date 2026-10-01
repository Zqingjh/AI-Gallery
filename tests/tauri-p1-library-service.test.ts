import { describe, expect, it, vi } from "vitest";
import type { CommandClient } from "../src/services/command-client";
import { createTauriP1LibraryService } from "../src/services/tauri-p1-library-service";

function setup() {
  const invoke = vi.fn();
  const client: CommandClient = { invoke };
  return {
    invoke,
    service: createTauriP1LibraryService(client, () => "D:\\Library"),
  };
}

describe("Tauri P1 作品库服务", () => {
  it("保存筛选只发送工作区与结构化筛选", async () => {
    const { invoke, service } = setup();
    invoke.mockResolvedValue({
      id: 1,
      name: "公开收藏",
      filter: { version: 1, isPublic: true },
    });
    await expect(
      service.saveSavedFilter({
        name: "公开收藏",
        filter: { version: 1, isPublic: true },
      }),
    ).resolves.toMatchObject({ id: 1 });
    expect(invoke).toHaveBeenCalledWith("p1_library_save_saved_filter", {
      request: {
        rootPath: "D:\\Library",
        input: {
          name: "公开收藏",
          filter: { version: 1, isPublic: true },
        },
      },
    });
  });

  it("分页响应形状异常时安全拒绝", async () => {
    const { invoke, service } = setup();
    invoke.mockResolvedValue({ items: [{ id: "bad" }], nextCursor: null });
    await expect(service.listSavedFilters()).rejects.toMatchObject({
      code: "INVALID_RESPONSE",
    });
  });

  it("所有列表方法透传强类型游标并限制单页数量", async () => {
    const { invoke, service } = setup();
    invoke
      .mockResolvedValueOnce({ items: [], nextCursor: null })
      .mockResolvedValueOnce({ items: [], nextCursor: null })
      .mockResolvedValueOnce({ items: [], nextCursor: null });
    const cursor = { updatedAt: 200, id: 12 };
    const versionCursor = { version: 3, id: 30 };

    await service.listSavedFilters(cursor);
    await service.listPromptVersions(7, versionCursor);
    await service.listModelComparison({ kind: "project", id: 9, cursor });

    expect(invoke.mock.calls[0]?.[1]).toMatchObject({
      request: { cursor, limit: 50 },
    });
    expect(invoke.mock.calls[1]?.[1]).toMatchObject({
      request: { promptId: 7, cursor: versionCursor, limit: 50 },
    });
    expect(invoke.mock.calls[2]?.[1]).toMatchObject({
      request: { scope: { kind: "project", projectId: 9 }, cursor, limit: 50 },
    });
  });

  it("拒绝结构异常的下一页游标", async () => {
    const { invoke, service } = setup();
    invoke.mockResolvedValue({
      items: [],
      nextCursor: { updatedAt: "bad", id: 1 },
    });
    await expect(service.listSavedFilters()).rejects.toMatchObject({
      code: "INVALID_RESPONSE",
    });
  });

  it("拒绝智能集合响应中的未知或越界字段", async () => {
    const { invoke, service } = setup();
    invoke.mockResolvedValue({
      items: [
        {
          id: 1,
          name: "危险响应",
          filter: { version: 1, storedPath: "private/file.png" },
        },
      ],
      nextCursor: null,
    });
    await expect(service.listSavedFilters()).rejects.toMatchObject({
      code: "INVALID_RESPONSE",
    });
  });

  it("未连接工作区时不调用桌面命令", async () => {
    const invoke = vi.fn();
    const service = createTauriP1LibraryService({ invoke }, () => null);
    await expect(service.listSavedFilters()).rejects.toMatchObject({
      code: "COMMAND_FAILED",
    });
    expect(invoke).not.toHaveBeenCalled();
  });

  it("批量写入携带确认标记，AI 预览只传用户选择的字段范围", async () => {
    const { invoke, service } = setup();
    invoke
      .mockResolvedValueOnce({
        targetCount: 2,
        categoryRelationsToAdd: 0,
        categoryRelationsToRemove: 0,
        tagRelationsToAdd: 0,
        tagRelationsToRemove: 0,
      })
      .mockResolvedValueOnce({
        targetCount: 2,
        fieldNames: ["title"],
        taxonomyDimensionCount: 3,
      })
      .mockResolvedValueOnce({
        created: [],
        failed: [
          {
            target: { type: "asset", id: "1" },
            kind: "request_failed",
          },
        ],
      });
    const input = {
      assetIds: [1, 2],
      model: { action: "keep" } as const,
      platform: { action: "keep" } as const,
      addCategoryIds: [],
      removeCategoryIds: [],
      addTagIds: [],
      removeTagIds: [],
    };
    await service.bulkEditAssets(input);
    const aiInput = {
      providerId: 9,
      assetIds: [1, 2],
      inputScope: {
        title: false,
        promptZh: true,
        promptEn: false,
        negativePrompt: false,
      },
    };
    await service.previewBatchAi(aiInput);
    await expect(service.createBatchAi(aiInput)).resolves.toEqual({
      created: 0,
      failed: 1,
      failureKinds: ["request_failed"],
    });
    expect(invoke.mock.calls[0]?.[1]).toMatchObject({
      request: { confirmed: true, input: { assetIds: [1, 2] } },
    });
    expect(invoke.mock.calls[1]?.[1]).toMatchObject({
      request: {
        rootPath: "D:\\Library",
        assetDisplayNumbers: [1, 2],
        inputScope: {
          title: false,
          promptZh: true,
          promptEn: false,
          negativePrompt: false,
        },
      },
    });
    expect(invoke.mock.calls[2]?.[1]).toMatchObject({
      request: {
        rootPath: "D:\\Library",
        providerId: 9,
        assetDisplayNumbers: [1, 2],
        inputScope: aiInput.inputScope,
        confirmed: true,
      },
    });
    expect(invoke.mock.calls[1]?.[1]).not.toHaveProperty("request.targets");
    expect(invoke.mock.calls[2]?.[1]).not.toHaveProperty("request.targets");
  });
});
