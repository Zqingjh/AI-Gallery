import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import P1LibraryPage, {
  type P1LibraryService,
} from "../src/features/p1/P1LibraryPage";
import P1LibraryRoute from "../src/features/p1/P1LibraryRoute";
import type { CommandClient } from "../src/services/command-client";

function createService(): P1LibraryService {
  const emptyPage = { items: [], nextCursor: null };
  return {
    previewBulkAssetEdit: vi.fn().mockResolvedValue({
      targetCount: 3,
      categoryRelationsToAdd: 0,
      categoryRelationsToRemove: 0,
      tagRelationsToAdd: 0,
      tagRelationsToRemove: 0,
    }),
    bulkEditAssets: vi.fn().mockResolvedValue({
      targetCount: 3,
      categoryRelationsToAdd: 0,
      categoryRelationsToRemove: 0,
      tagRelationsToAdd: 0,
      tagRelationsToRemove: 0,
    }),
    listModelComparison: vi.fn().mockResolvedValue(emptyPage),
    previewBatchAi: vi.fn(),
    createBatchAi: vi.fn(),
    listSavedFilters: vi.fn().mockResolvedValue(emptyPage),
    saveSavedFilter: vi.fn(),
    deleteSavedFilter: vi.fn(),
    listCustomFields: vi.fn().mockResolvedValue(emptyPage),
    saveCustomField: vi.fn(),
    listCustomFieldValues: vi.fn().mockResolvedValue(emptyPage),
    saveCustomFieldValue: vi.fn(),
    confirmPendingCustomFieldValue: vi.fn(),
    listPromptVersions: vi.fn().mockResolvedValue(emptyPage),
    createPromptVersion: vi.fn(),
    listEditHistory: vi.fn().mockResolvedValue(emptyPage),
  };
}

function createTaxonomyService() {
  return {
    listTaxonomy: vi.fn().mockResolvedValue({
      dimensions: [
        {
          id: "1",
          name: "视觉风格",
          allowsMultiple: true,
          aiCanSuggestNew: false,
          enabled: true,
          categories: [
            {
              id: "3",
              dimensionId: "1",
              name: "电影感",
              color: null,
              assetCount: 0,
              enabled: true,
              aliases: [],
              description: "",
              icon: null,
            },
          ],
        },
      ],
      tags: [{ id: "4", name: "精选", assetCount: 0 }],
    }),
    listMetadataPresets: vi.fn().mockResolvedValue({
      models: [{ id: "model-1", name: "Flux", assetCount: 1 }],
      platforms: [{ id: "platform-1", name: "LiblibAI", assetCount: 1 }],
    }),
  };
}

describe("P1 创作效率页", () => {
  it("进入创作效率页时先连接当前工作区，再读取分类与标签预设", async () => {
    const invoke = vi.fn(async (command: string) => {
      if (command === "open_workspace") return { displayName: "aigc" };
      if (command === "prepare_workspace_database") return {};
      if (command === "library_list_dimensions") {
        return [
          {
            id: 1,
            name: "视觉风格",
            allowsMultiple: true,
            aiCanSuggestNew: false,
            isEnabled: true,
          },
        ];
      }
      if (command === "library_list_categories") {
        return [
          {
            id: 3,
            dimensionId: 1,
            name: "电影感",
            color: null,
            assetCount: 0,
            isEnabled: true,
            aliases: [],
            description: "",
            icon: null,
          },
        ];
      }
      if (command === "library_list_tags") return [];
      throw new Error(`未处理命令：${command}`);
    });
    const commandClient: CommandClient = {
      invoke: async <TResult,>(command: string) =>
        (await invoke(command)) as TResult,
    };

    render(
      <P1LibraryRoute
        commandClient={commandClient}
        workspaceRoot="D:/gallery"
        accessMode="readWrite"
        onBack={vi.fn()}
        onApplyFilter={vi.fn()}
      />,
    );

    const addCategory = await screen.findByRole("button", {
      name: "添加分类",
    });
    fireEvent.click(addCategory);
    expect(await screen.findByRole("option", { name: "电影感" })).toBeVisible();
    expect(invoke.mock.calls.map(([command]) => command)).toEqual(
      expect.arrayContaining([
        "open_workspace",
        "prepare_workspace_database",
        "library_list_dimensions",
        "library_list_categories",
        "library_list_tags",
      ]),
    );
  });

  it("仅保留批量操作，不再渲染模型对比及后续工具", () => {
    render(
      <P1LibraryPage
        service={createService()}
        taxonomyService={createTaxonomyService()}
      />,
    );
    expect(screen.getByText("批量编辑与分类")).toBeInTheDocument();
    expect(screen.queryByText("模型对比")).not.toBeInTheDocument();
    expect(screen.queryByText("批量 AI 建议")).not.toBeInTheDocument();
    expect(screen.queryByText("媒体完整性")).not.toBeInTheDocument();
    expect(screen.queryByText("模型操作")).not.toBeInTheDocument();
    expect(screen.queryByText("平台操作")).not.toBeInTheDocument();
  });

  it("AI 批量分类提供独立的作品编号输入框", () => {
    render(
      <P1LibraryPage
        service={createService()}
        taxonomyService={createTaxonomyService()}
      />,
    );

    expect(
      screen.getByLabelText("AI 分类作品编号（逗号、分号或 1-20）"),
    ).toBeInTheDocument();
  });

  it("按可见作品编号解析范围并批量设置评分", async () => {
    const service = createService();
    vi.spyOn(window, "confirm").mockReturnValue(true);
    render(
      <P1LibraryPage
        service={service}
        taxonomyService={createTaxonomyService()}
      />,
    );

    fireEvent.change(screen.getByLabelText("作品编号（逗号、分号或 1-20）"), {
      target: { value: "1-2；4" },
    });
    fireEvent.change(screen.getByLabelText("评分（留空保持）"), {
      target: { value: "5" },
    });
    fireEvent.click(screen.getByRole("button", { name: "确认批量修改" }));

    await waitFor(() => expect(service.bulkEditAssets).toHaveBeenCalled());
    expect(service.bulkEditAssets).toHaveBeenCalledWith(
      expect.objectContaining({ assetIds: [1, 2, 4], rating: 5 }),
    );
    const submitted = vi.mocked(service.bulkEditAssets).mock.calls[0]![0];
    expect(submitted.model).toEqual({ action: "keep" });
    expect(submitted.platform).toEqual({ action: "keep" });
    expect(submitted).not.toHaveProperty("isPublic");
    expect(submitted).not.toHaveProperty("isFavorite");
    expect(screen.getByRole("status")).toHaveTextContent("批量修改已完成");
  });

  it("模型和平台可使用预设或输入新值，并在确认前明确预览字段", async () => {
    const service = createService();
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
    render(
      <P1LibraryPage
        service={service}
        taxonomyService={createTaxonomyService()}
      />,
    );

    fireEvent.change(screen.getByLabelText("作品编号（逗号、分号或 1-20）"), {
      target: { value: "8" },
    });
    const modelInput = screen.getByLabelText("添加模型（留空保持）");
    expect(modelInput).toHaveAttribute("list", "p1-batch-model-presets");
    await waitFor(() =>
      expect(
        document.querySelector('#p1-batch-model-presets option[value="Flux"]'),
      ).not.toBeNull(),
    );
    fireEvent.change(modelInput, { target: { value: "  Flux  " } });
    fireEvent.change(screen.getByLabelText("添加平台（留空保持）"), {
      target: { value: "  新平台  " },
    });
    fireEvent.click(screen.getByRole("button", { name: "预览影响" }));

    expect(await screen.findByRole("status")).toHaveTextContent(
      "模型“Flux”、平台“新平台”",
    );
    fireEvent.click(screen.getByRole("button", { name: "确认批量修改" }));
    await waitFor(() => expect(service.bulkEditAssets).toHaveBeenCalled());
    expect(service.bulkEditAssets).toHaveBeenCalledWith(
      expect.objectContaining({
        model: { action: "set", value: "Flux" },
        platform: { action: "set", value: "新平台" },
      }),
    );
    expect(confirm).toHaveBeenCalledWith(
      expect.stringContaining("模型“Flux”、平台“新平台”"),
    );
  });

  it("从预设中选择分类和标签后提交关系 ID", async () => {
    const service = createService();
    vi.spyOn(window, "confirm").mockReturnValue(true);
    render(
      <P1LibraryPage
        service={service}
        taxonomyService={createTaxonomyService()}
      />,
    );

    fireEvent.change(screen.getByLabelText("作品编号（逗号、分号或 1-20）"), {
      target: { value: "1" },
    });
    const addCategory = await screen.findByRole("button", {
      name: "添加分类",
    });
    expect(
      screen.queryByRole("option", { name: "电影感" }),
    ).not.toBeInTheDocument();
    fireEvent.click(addCategory);
    fireEvent.click(screen.getByRole("option", { name: "电影感" }));
    fireEvent.click(screen.getByRole("button", { name: "完成" }));
    fireEvent.click(screen.getByRole("button", { name: "添加标签" }));
    fireEvent.click(screen.getByRole("option", { name: "精选" }));
    fireEvent.click(screen.getByRole("button", { name: "完成" }));
    fireEvent.click(screen.getByRole("button", { name: "确认批量修改" }));

    await waitFor(() => expect(service.bulkEditAssets).toHaveBeenCalled());
    expect(service.bulkEditAssets).toHaveBeenCalledWith(
      expect.objectContaining({ addCategoryIds: [3], addTagIds: [4] }),
    );
  });

  it("展示情况可同时选择公开和收藏，留空时保持原样", async () => {
    const service = createService();
    vi.spyOn(window, "confirm").mockReturnValue(true);
    render(
      <P1LibraryPage
        service={service}
        taxonomyService={createTaxonomyService()}
      />,
    );

    fireEvent.change(screen.getByLabelText("作品编号（逗号、分号或 1-20）"), {
      target: { value: "1" },
    });
    fireEvent.click(
      screen.getByRole("button", { name: "展示情况（留空保持）" }),
    );
    fireEvent.click(screen.getByRole("option", { name: "公开" }));
    fireEvent.click(screen.getByRole("option", { name: "收藏" }));
    fireEvent.click(screen.getByRole("button", { name: "完成" }));
    fireEvent.click(screen.getByRole("button", { name: "确认批量修改" }));

    await waitFor(() => expect(service.bulkEditAssets).toHaveBeenCalled());
    expect(service.bulkEditAssets).toHaveBeenCalledWith(
      expect.objectContaining({ isPublic: true, isFavorite: true }),
    );
  });

  it("只读展示模式隐藏批量写入表单", () => {
    render(
      <P1LibraryPage
        service={createService()}
        taxonomyService={createTaxonomyService()}
        accessMode="readOnly"
      />,
    );
    expect(screen.getByText("只读展示")).toBeInTheDocument();
    expect(screen.queryByText("批量编辑与分类")).not.toBeInTheDocument();
  });
});
