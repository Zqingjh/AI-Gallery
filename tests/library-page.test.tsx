import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { StrictMode } from "react";
import { describe, expect, it, vi } from "vitest";
import { text } from "../src/app/texts";
import { LibraryPage } from "../src/features/library/LibraryPage";
import type {
  AssetDetail,
  AssetSummary,
  LibraryService,
  ProjectSummary,
} from "../src/services/library-service";
import type { MediaIntegrityService } from "../src/services/media-integrity-service";

const asset: AssetSummary = {
  id: "asset-1",
  projectId: null,
  fileName: "晨雾城市.png",
  mediaType: "image",
  thumbnailUrl: "thumb://asset-1",
  modelName: "Flux",
  platformName: "Local",
  rating: 4,
  isFavorite: true,
  isPublic: false,
  width: 1280,
  height: 960,
  durationMs: null,
  updatedAt: "2026-07-13T10:00:00Z",
};

const detail: AssetDetail = {
  ...asset,
  title: "晨雾城市",
  promptZh: "清晨薄雾中的未来城市",
  promptEn: "future city in morning fog",
  negativePrompt: "low quality",
  generationParamsJson: '{"seed":42}',
  notes: "",
  previewUrl: "blob:preview-1",
  categoryIds: [],
  tagIds: [],
  storedPath: "media/images/晨雾城市.png",
  previewError: null,
};

const project: ProjectSummary = {
  id: "project-1",
  title: "城市实验",
  description: "光线与建筑实验",
  rating: 4,
  isFavorite: false,
  isPublic: false,
  updatedAt: "2026-07-13T10:00:00Z",
  assetCount: 1,
  kind: "simple",
};

function createService(
  overrides: Partial<LibraryService> = {},
): LibraryService {
  const service: LibraryService = {
    getWorkspaceState: vi
      .fn()
      .mockResolvedValue({ isOpen: true, displayName: "创作档案" }),
    selectWorkspaceDirectory: vi.fn().mockResolvedValue(null),
    createWorkspace: vi
      .fn()
      .mockResolvedValue({ isOpen: true, displayName: "创作档案" }),
    connectWorkspace: vi
      .fn()
      .mockResolvedValue({ isOpen: true, displayName: "创作档案" }),
    listProjects: vi
      .fn()
      .mockResolvedValue({ items: [project], nextCursor: null }),
    createProject: vi.fn(),
    getProjectDetail: vi.fn().mockResolvedValue({
      ...project,
      promptZh: "",
      promptEn: "",
      negativePrompt: "",
      notes: "",
      categoryIds: [],
      tagIds: [],
    }),
    updateProject: vi.fn(),
    assignAssetsToProject: vi.fn().mockResolvedValue(0),
    removeAssetsFromProject: vi.fn().mockResolvedValue(0),
    listCanvasMembers: vi
      .fn()
      .mockResolvedValue({ items: [], nextCursor: null }),
    setCanvasMember: vi.fn().mockResolvedValue(undefined),
    updateCanvasOutputPrompt: vi.fn().mockResolvedValue(undefined),
    listAssets: vi.fn().mockResolvedValue({ items: [asset], nextCursor: null }),
    listAssetPage: vi.fn().mockResolvedValue({
      items: [asset],
      page: 1,
      pageSize: 25,
      totalCount: 1,
      totalPages: 1,
    }),
    listDuplicateGroups: vi
      .fn()
      .mockResolvedValue({ items: [], nextCursor: null }),
    getAssetThumbnail: vi.fn().mockResolvedValue(null),
    getAssetDetail: vi.fn().mockResolvedValue(detail),
    createAsset: vi.fn(),
    updateAsset: vi.fn(),
    updateAssetDisplayOrder: vi.fn().mockResolvedValue(detail),
    startImport: vi.fn(),
    selectImportFiles: vi.fn().mockResolvedValue([]),
    selectImportDirectory: vi.fn().mockResolvedValue(null),
    selectExportDirectory: vi.fn().mockResolvedValue(null),
    exportAssets: vi.fn().mockResolvedValue("export"),
    exportProjects: vi.fn().mockResolvedValue("export"),
    subscribeImport: vi.fn().mockReturnValue(vi.fn()),
    cancelImport: vi.fn().mockResolvedValue(undefined),
    copyText: vi.fn().mockResolvedValue(undefined),
    listTaxonomy: vi.fn().mockResolvedValue({ dimensions: [], tags: [] }),
    listMetadataPresets: vi.fn().mockResolvedValue({
      models: [{ id: "model-1", name: "Flux", assetCount: 1 }],
      platforms: [{ id: "platform-1", name: "Local", assetCount: 1 }],
    }),
    createMetadataPreset: vi.fn().mockResolvedValue({
      id: "model-new",
      name: "Seedream 5.0 Lite",
      assetCount: 0,
    }),
    updateMetadataPreset: vi.fn().mockResolvedValue(undefined),
    deleteMetadataPreset: vi.fn().mockResolvedValue(undefined),
    createDimension: vi.fn(),
    updateDimension: vi.fn().mockResolvedValue(undefined),
    deleteDimension: vi.fn().mockResolvedValue(undefined),
    createCategory: vi.fn(),
    createTag: vi.fn(),
    updateCategory: vi.fn().mockResolvedValue(undefined),
    updateTag: vi.fn().mockResolvedValue(undefined),
    deleteTag: vi.fn().mockResolvedValue(undefined),
    getCategoryImpact: vi.fn().mockResolvedValue(0),
    deleteCategory: vi.fn().mockResolvedValue(undefined),
    moveAssetToTrash: vi.fn().mockResolvedValue({
      id: "trash-1",
      entityType: "asset",
      entityId: asset.id,
      title: asset.fileName,
      mediaType: asset.mediaType,
      thumbnailUrl: asset.thumbnailUrl,
      deletedAt: "2026-07-13T11:00:00Z",
      originalMediaPreserved: true,
    }),
    moveAssetsToTrash: vi.fn().mockResolvedValue(undefined),
    moveProjectToTrash: vi.fn().mockResolvedValue({
      id: "trash-project-1",
      entityType: "project",
      entityId: project.id,
      title: project.title,
      mediaType: null,
      thumbnailUrl: null,
      deletedAt: "2026-07-13T11:00:00Z",
      originalMediaPreserved: true,
    }),
    undoLastTrash: vi.fn().mockResolvedValue(undefined),
    listTrash: vi.fn().mockResolvedValue({ items: [], nextCursor: null }),
    restoreTrashItem: vi.fn().mockResolvedValue(undefined),
    restoreTrashItems: vi.fn().mockResolvedValue(undefined),
    permanentlyDeleteTrashItem: vi.fn().mockResolvedValue(undefined),
    permanentlyDeleteTrashItems: vi.fn().mockResolvedValue(undefined),
    ...overrides,
  };
  // 旧用例通过游标接口注入列表；适配成数字分页后继续验证原有卡片行为。
  if (overrides.listAssets && !overrides.listAssetPage) {
    const legacyListAssets = overrides.listAssets;
    service.listAssetPage = vi.fn(async (request) => {
      const { page, pageSize, ...filters } = request;
      const result = await legacyListAssets({ limit: pageSize, ...filters });
      const totalCount = result.nextCursor
        ? page * pageSize + 1
        : (page - 1) * pageSize + result.items.length;
      return {
        items: result.items,
        page,
        pageSize,
        totalCount,
        totalPages: totalCount === 0 ? 0 : Math.ceil(totalCount / pageSize),
      };
    });
  }
  return service;
}

describe("M2 作品库页面", () => {
  it("作品标题只有在每次间隔不超过一秒的连续五击后才激活隐藏界面", async () => {
    const onSecretActivate = vi.fn();
    const { container } = render(
      <LibraryPage
        service={createService()}
        onOpenSettings={vi.fn()}
        onSecretActivate={onSecretActivate}
      />,
    );
    await screen.findByText(asset.fileName);
    const trigger = container.querySelector<HTMLButtonElement>(
      ".library-title-frame",
    );
    expect(trigger).not.toBeNull();

    let now = new Date("2026-07-16T10:00:00Z").getTime();
    const nowSpy = vi.spyOn(Date, "now").mockImplementation(() => now);
    try {
      for (let index = 0; index < 4; index += 1) {
        fireEvent.click(trigger!);
        now += 200;
      }
      expect(onSecretActivate).not.toHaveBeenCalled();

      now += 1_001;
      for (let index = 0; index < 5; index += 1) {
        fireEvent.click(trigger!);
        now += 200;
      }
      expect(onSecretActivate).toHaveBeenCalledTimes(1);
    } finally {
      nowSpy.mockRestore();
    }
  });

  it("隐藏模式使用独立标题与退出入口，不再响应五击激活", async () => {
    const onExitPrivate = vi.fn();
    const { container } = render(
      <LibraryPage
        service={createService()}
        onOpenSettings={vi.fn()}
        mode="nsfw"
        onExitPrivate={onExitPrivate}
      />,
    );
    await screen.findByText(asset.fileName);

    expect(
      screen.getByRole("heading", { name: "NSFW作品" }),
    ).toBeInTheDocument();
    expect(container.querySelector(".library-title-frame")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "退出NSFW" }));
    expect(onExitPrivate).toHaveBeenCalledTimes(1);
  });

  it("模型和平台筛选保持空白，点击后以最多五项的浮层展示", async () => {
    const service = createService({
      listMetadataPresets: vi.fn().mockResolvedValue({
        models: Array.from({ length: 8 }, (_, index) => ({
          id: `model-${index}`,
          name: `模型 ${index + 1}`,
          assetCount: 0,
        })),
        platforms: Array.from({ length: 7 }, (_, index) => ({
          id: `platform-${index}`,
          name: `平台 ${index + 1}`,
          assetCount: 0,
        })),
      }),
    });
    render(<LibraryPage service={service} onOpenSettings={vi.fn()} />);

    const modelTrigger = await screen.findByRole("button", {
      name: text.library.filterModel,
    });
    expect(modelTrigger).toHaveTextContent("");
    expect(
      screen.queryByRole("option", { name: "模型 8" }),
    ).not.toBeInTheDocument();

    fireEvent.click(modelTrigger);
    expect(
      screen.getByRole("listbox", { name: text.library.filterModel }),
    ).toBeVisible();
    expect(await screen.findByRole("option", { name: "模型 8" })).toBeVisible();
  });

  it("点击项目进入详情并自动按项目筛选作品", async () => {
    const listAssetPage = vi.fn().mockResolvedValue({
      items: [{ ...asset, projectId: project.id }],
      page: 1,
      pageSize: 25,
      totalCount: 1,
      totalPages: 1,
    });
    const service = createService({ listAssetPage });
    render(<LibraryPage service={service} onOpenSettings={vi.fn()} />);

    await screen.findByText(asset.fileName);
    fireEvent.click(
      screen.getByRole("button", { name: text.library.tabs.projects }),
    );
    fireEvent.click(
      await screen.findByRole("button", {
        name: `${text.library.project.open}：${project.title}`,
      }),
    );

    expect(
      await screen.findByText(text.library.project.includedWorks),
    ).toBeVisible();
    await waitFor(() =>
      expect(listAssetPage).toHaveBeenLastCalledWith({
        projectId: project.id,
        page: 1,
        pageSize: 25,
      }),
    );
  });

  it("可选择项目中的作品并仅解除项目关系", async () => {
    const removeAssetsFromProject = vi.fn().mockResolvedValue(1);
    vi.spyOn(window, "confirm").mockReturnValue(true);
    const service = createService({ removeAssetsFromProject });
    render(<LibraryPage service={service} onOpenSettings={vi.fn()} />);

    await screen.findByText(asset.fileName);
    fireEvent.click(
      screen.getByRole("button", { name: text.library.tabs.projects }),
    );
    fireEvent.click(
      await screen.findByRole("button", {
        name: `${text.library.project.open}：${project.title}`,
      }),
    );
    fireEvent.click(
      await screen.findByRole("checkbox", {
        name: `选择作品：${asset.fileName}`,
      }),
    );
    fireEvent.click(
      screen.getByRole("button", {
        name: text.library.project.removeSelectedLabel,
      }),
    );

    await waitFor(() =>
      expect(removeAssetsFromProject).toHaveBeenCalledWith(project.id, [
        asset.id,
      ]),
    );
  });

  it("编辑项目可按逗号分隔的展示编号批量归入作品", async () => {
    const updateProject = vi.fn().mockResolvedValue(project);
    const assignAssetsToProject = vi.fn().mockResolvedValue(3);
    vi.spyOn(window, "confirm").mockReturnValue(true);
    const service = createService({ updateProject, assignAssetsToProject });
    render(<LibraryPage service={service} onOpenSettings={vi.fn()} />);

    await screen.findByText(asset.fileName);
    fireEvent.click(
      screen.getByRole("button", { name: text.library.tabs.projects }),
    );
    fireEvent.click(
      await screen.findByRole("button", { name: text.library.project.edit }),
    );
    fireEvent.change(
      await screen.findByPlaceholderText(
        text.library.form.projectAssetsPlaceholder,
      ),
      { target: { value: "1，2,15" } },
    );
    fireEvent.click(
      screen.getByRole("button", { name: text.library.form.save }),
    );

    await waitFor(() =>
      expect(assignAssetsToProject).toHaveBeenCalledWith(
        project.id,
        [1, 2, 15],
      ),
    );
  });

  it("项目字段已保存但归入失败时给出明确的部分成功提示", async () => {
    const updateProject = vi.fn().mockResolvedValue(project);
    const assignAssetsToProject = vi
      .fn()
      .mockRejectedValue(new Error("missing"));
    vi.spyOn(window, "confirm").mockReturnValue(true);
    const service = createService({ updateProject, assignAssetsToProject });
    render(<LibraryPage service={service} onOpenSettings={vi.fn()} />);

    await screen.findByText(asset.fileName);
    fireEvent.click(
      screen.getByRole("button", { name: text.library.tabs.projects }),
    );
    fireEvent.click(
      await screen.findByRole("button", { name: text.library.project.edit }),
    );
    fireEvent.change(
      await screen.findByPlaceholderText(
        text.library.form.projectAssetsPlaceholder,
      ),
      { target: { value: "999" } },
    );
    fireEvent.click(
      screen.getByRole("button", { name: text.library.form.save }),
    );

    expect(await screen.findByRole("alert")).toHaveTextContent(
      text.library.form.projectSavedAssetsFailed,
    );
    expect(updateProject).toHaveBeenCalledOnce();
    expect(assignAssetsToProject).toHaveBeenCalledWith(project.id, [999]);
  });

  it("旧项目默认显示为分类项目，且项目卡片和详情不再显示英文小标题", async () => {
    render(<LibraryPage service={createService()} onOpenSettings={vi.fn()} />);
    await screen.findByText(asset.fileName);
    fireEvent.click(
      screen.getByRole("button", { name: text.library.tabs.projects }),
    );
    expect(
      await screen.findByText(text.library.project.simpleKind),
    ).toBeVisible();
    expect(screen.queryByText(/PROJECT\s*·/)).not.toBeInTheDocument();
    fireEvent.click(
      screen.getByRole("button", {
        name: `${text.library.project.open}：${project.title}`,
      }),
    );
    await screen.findByRole("heading", { name: project.title });
    expect(screen.queryByText(/PROJECT\s*·/)).not.toBeInTheDocument();
    expect(
      screen.getByText(new RegExp(`项目编号 ${project.id}`)),
    ).toBeVisible();
  });

  it("画布项目可切换成员角色、命名参考图并在输出提示词中插入引用", async () => {
    const canvasProject = {
      ...project,
      kind: "canvas" as const,
      assetCount: 2,
    };
    const reference = {
      assetId: "reference-1",
      displayOrder: 1,
      fileName: "构图参考.png",
      mediaType: "image" as const,
      modelName: "",
      platformName: "",
      width: 800,
      height: 600,
      durationMs: null,
      updatedAt: asset.updatedAt,
      role: "reference" as const,
      referenceName: "构图",
      promptZh: "",
      promptEn: "",
      negativePrompt: "",
    };
    const output = {
      ...reference,
      assetId: asset.id,
      displayOrder: 2,
      fileName: asset.fileName,
      role: "output" as const,
      referenceName: null,
      promptZh: "未来城市",
    };
    const setCanvasMember = vi.fn().mockResolvedValue(undefined);
    const updateCanvasOutputPrompt = vi.fn().mockResolvedValue(undefined);
    const service = createService({
      listProjects: vi
        .fn()
        .mockResolvedValue({ items: [canvasProject], nextCursor: null }),
      getProjectDetail: vi.fn().mockResolvedValue({
        ...canvasProject,
        promptZh: "",
        promptEn: "",
        negativePrompt: "",
        notes: "",
        categoryIds: [],
        tagIds: [],
      }),
      listCanvasMembers: vi.fn().mockImplementation((_projectId, request) =>
        Promise.resolve({
          items: [reference, output].filter(
            (member) => !request.role || member.role === request.role,
          ),
          nextCursor: null,
        }),
      ),
      setCanvasMember,
      updateCanvasOutputPrompt,
    });
    render(<LibraryPage service={service} onOpenSettings={vi.fn()} />);
    await screen.findByText(asset.fileName);
    fireEvent.click(
      screen.getByRole("button", { name: text.library.tabs.projects }),
    );
    fireEvent.click(
      await screen.findByRole("button", {
        name: `${text.library.project.open}：${canvasProject.title}`,
      }),
    );

    expect(await screen.findByText("参考图片")).toBeVisible();
    expect(service.listCanvasMembers).toHaveBeenCalledWith(
      canvasProject.id,
      expect.objectContaining({ role: "reference", limit: 25 }),
    );
    expect(service.listCanvasMembers).toHaveBeenCalledWith(
      canvasProject.id,
      expect.objectContaining({ role: "output", limit: 25 }),
    );
    fireEvent.change(
      await screen.findByLabelText(`引用名称：${output.fileName}`),
      {
        target: { value: "色彩" },
      },
    );
    fireEvent.click(screen.getByRole("button", { name: "设为参考图" }));
    await waitFor(() =>
      expect(setCanvasMember).toHaveBeenCalledWith(
        canvasProject.id,
        output.assetId,
        "reference",
        "色彩",
      ),
    );

    const zhField = screen.getByText("中文提示词").closest("label");
    fireEvent.click(zhField!.querySelector<HTMLButtonElement>("button")!);
    fireEvent.click(screen.getByRole("button", { name: "保存输出提示词" }));
    await waitFor(() =>
      expect(updateCanvasOutputPrompt).toHaveBeenCalledWith(
        canvasProject.id,
        output.assetId,
        expect.objectContaining({ promptZh: "未来城市 @构图" }),
      ),
    );
  });

  it("画布项目只读时隐藏角色、提示词和移出写操作", async () => {
    const canvasProject = { ...project, kind: "canvas" as const };
    const service = createService({
      listProjects: vi
        .fn()
        .mockResolvedValue({ items: [canvasProject], nextCursor: null }),
      getProjectDetail: vi.fn().mockResolvedValue({
        ...canvasProject,
        promptZh: "",
        promptEn: "",
        negativePrompt: "",
        notes: "",
        categoryIds: [],
        tagIds: [],
      }),
      listCanvasMembers: vi.fn().mockImplementation((_projectId, request) => {
        const item = {
          assetId: asset.id,
          displayOrder: 1,
          fileName: asset.fileName,
          mediaType: "image",
          modelName: "Flux",
          platformName: "Local",
          width: 1280,
          height: 960,
          durationMs: null,
          updatedAt: asset.updatedAt,
          role: "output",
          referenceName: null,
          promptZh: "只读提示词",
          promptEn: "",
          negativePrompt: "",
        };
        return Promise.resolve({
          items: !request.role || request.role === "output" ? [item] : [],
          nextCursor: null,
        });
      }),
    });
    render(
      <LibraryPage
        service={service}
        onOpenSettings={vi.fn()}
        accessMode="readOnly"
      />,
    );
    await screen.findByText(asset.fileName);
    fireEvent.click(
      screen.getByRole("button", { name: text.library.tabs.projects }),
    );
    fireEvent.click(
      await screen.findByRole("button", {
        name: `${text.library.project.open}：${canvasProject.title}`,
      }),
    );
    expect(await screen.findByText("只读提示词")).toBeVisible();
    expect(screen.queryByRole("button", { name: "保存输出提示词" })).toBeNull();
    expect(screen.queryByText("设为参考图")).toBeNull();
    expect(screen.queryByText("移出项目")).toBeNull();
  });

  it("切换画布项目时旧请求晚到不会覆盖新项目成员", async () => {
    type CanvasPage = Awaited<ReturnType<LibraryService["listCanvasMembers"]>>;
    const firstProject = { ...project, kind: "canvas" as const };
    const secondProject = {
      ...project,
      id: "project-2",
      title: "第二个画布",
      kind: "canvas" as const,
    };
    const staleResolvers: Array<(value: CanvasPage) => void> = [];
    const secondOutput = {
      assetId: "second-output",
      displayOrder: 8,
      fileName: "第二项目输出.png",
      mediaType: "image" as const,
      modelName: "",
      platformName: "",
      width: 800,
      height: 600,
      durationMs: null,
      updatedAt: asset.updatedAt,
      role: "output" as const,
      referenceName: null,
      promptZh: "第二项目",
      promptEn: "",
      negativePrompt: "",
    };
    const service = createService({
      listProjects: vi.fn().mockResolvedValue({
        items: [firstProject, secondProject],
        nextCursor: null,
      }),
      getProjectDetail: vi.fn().mockImplementation((projectId: string) =>
        Promise.resolve({
          ...(projectId === firstProject.id ? firstProject : secondProject),
          promptZh: "",
          promptEn: "",
          negativePrompt: "",
          notes: "",
          categoryIds: [],
          tagIds: [],
        }),
      ),
      listCanvasMembers: vi.fn().mockImplementation((projectId, request) => {
        if (projectId === firstProject.id) {
          return new Promise<CanvasPage>((resolve) =>
            staleResolvers.push(resolve),
          );
        }
        return Promise.resolve({
          items: request.role === "output" ? [secondOutput] : [],
          nextCursor: null,
        });
      }),
    });
    render(<LibraryPage service={service} onOpenSettings={vi.fn()} />);
    await screen.findByText(asset.fileName);
    fireEvent.click(
      screen.getByRole("button", { name: text.library.tabs.projects }),
    );
    fireEvent.click(
      await screen.findByRole("button", {
        name: `${text.library.project.open}：${firstProject.title}`,
      }),
    );
    await screen.findByText("参考图片");
    fireEvent.click(
      screen.getByRole("button", {
        name: new RegExp(text.library.project.back),
      }),
    );
    fireEvent.click(
      await screen.findByRole("button", {
        name: `${text.library.project.open}：${secondProject.title}`,
      }),
    );
    expect(
      await screen.findByRole("button", {
        name: `打开作品：${secondOutput.fileName}`,
      }),
    ).toBeVisible();

    await act(async () => {
      staleResolvers.forEach((resolve) =>
        resolve({
          items: [{ ...secondOutput, fileName: "过期项目成员.png" }],
          nextCursor: null,
        }),
      );
      await Promise.resolve();
    });
    expect(
      screen.queryByRole("button", { name: "打开作品：过期项目成员.png" }),
    ).toBeNull();
    expect(
      screen.getByRole("button", {
        name: `打开作品：${secondOutput.fileName}`,
      }),
    ).toBeVisible();
  });

  it("作品使用数字分页并支持 10/25/50 每页数量", async () => {
    const listAssetPage = vi.fn().mockImplementation(async (request) => ({
      items: [asset],
      page: request.page,
      pageSize: request.pageSize,
      totalCount: 88,
      totalPages: Math.ceil(88 / request.pageSize),
    }));
    const service = createService({ listAssetPage });
    render(<LibraryPage service={service} onOpenSettings={vi.fn()} />);

    await screen.findByText(asset.fileName);
    const pageSize = screen.getByLabelText(text.library.pageSize);
    expect(pageSize).toHaveValue("25");
    for (const page of ["1", "2", "3", "4"]) {
      expect(
        screen.getByRole("button", { name: `第 ${page} 页` }),
      ).toBeVisible();
    }

    fireEvent.click(screen.getByRole("button", { name: "第 3 页" }));
    await waitFor(() =>
      expect(listAssetPage).toHaveBeenLastCalledWith(
        expect.objectContaining({ page: 3, pageSize: 25 }),
      ),
    );

    fireEvent.change(pageSize, { target: { value: "10" } });
    await waitFor(() =>
      expect(listAssetPage).toHaveBeenLastCalledWith(
        expect.objectContaining({ page: 1, pageSize: 10 }),
      ),
    );
  });

  it("数据变化导致当前页越界时自动回到最后一个有效页", async () => {
    const pageTwoAsset = { ...asset, id: "asset-2", fileName: "回退页.png" };
    const listAssetPage = vi.fn().mockImplementation(async (request) => {
      if (request.page === 3) {
        return {
          items: [],
          page: 3,
          pageSize: 25,
          totalCount: 50,
          totalPages: 2,
        };
      }
      return {
        items: request.page === 2 ? [pageTwoAsset] : [asset],
        page: request.page,
        pageSize: 25,
        totalCount: request.page === 1 ? 51 : 50,
        totalPages: request.page === 1 ? 3 : 2,
      };
    });
    render(
      <LibraryPage
        service={createService({ listAssetPage })}
        onOpenSettings={vi.fn()}
      />,
    );

    fireEvent.click(
      await screen.findByRole("button", { name: text.library.pageLabel(3) }),
    );

    expect(await screen.findByText(pageTwoAsset.fileName)).toBeInTheDocument();
    expect(listAssetPage).toHaveBeenLastCalledWith({ page: 2, pageSize: 25 });
  });

  it("项目页解释项目用途并显示稳定编号", async () => {
    render(<LibraryPage service={createService()} onOpenSettings={vi.fn()} />);
    await screen.findByText(asset.fileName);
    fireEvent.click(
      screen.getByRole("button", { name: text.library.tabs.projects }),
    );
    expect(screen.getByText(text.library.project.description)).toBeVisible();
    expect(
      screen.getByText(`${text.library.project.idLabel} ${project.id}`),
    ).toBeVisible();
  });

  it("视频详情可直达对应作品的封面管理工具", async () => {
    const videoAsset: AssetSummary = {
      ...asset,
      id: "44",
      fileName: "豆包.mp4",
      mediaType: "video",
      durationMs: 10_000,
    };
    const mediaIntegrityService = {
      getAssetCover: vi.fn().mockResolvedValue(null),
      generateDefaultVideoCover: vi.fn(),
    } as unknown as MediaIntegrityService;
    const service = createService({
      listAssetPage: vi.fn().mockResolvedValue({
        items: [videoAsset],
        page: 1,
        pageSize: 25,
        totalCount: 1,
        totalPages: 1,
      }),
      getAssetDetail: vi.fn().mockResolvedValue({
        ...detail,
        ...videoAsset,
        title: "豆包",
        previewUrl: "blob:video-preview",
      }),
    });
    render(
      <LibraryPage
        service={service}
        onOpenSettings={vi.fn()}
        mediaIntegrityService={mediaIntegrityService}
      />,
    );

    fireEvent.click(
      await screen.findByRole("button", {
        name: `${text.library.openDetail}：${videoAsset.fileName}`,
      }),
    );
    fireEvent.click(
      await screen.findByRole("button", {
        name: text.library.detail.manageVideoCover,
      }),
    );

    await waitFor(() => {
      expect(mediaIntegrityService.getAssetCover).toHaveBeenCalledWith(
        Number(videoAsset.id),
      );
    });
  });
  it("StrictMode 预演不会在详情仍打开时撤销预览 URL", async () => {
    const revoke = vi.fn();
    Object.defineProperty(URL, "revokeObjectURL", {
      configurable: true,
      value: revoke,
    });
    const service = createService();
    const view = render(
      <StrictMode>
        <LibraryPage service={service} onOpenSettings={vi.fn()} />
      </StrictMode>,
    );
    await screen.findByText(asset.fileName);
    fireEvent.click(
      screen.getByRole("button", {
        name: `${text.library.openDetail}：${asset.fileName}`,
      }),
    );

    expect(await screen.findByRole("dialog")).toBeInTheDocument();
    expect(revoke).not.toHaveBeenCalledWith(detail.previewUrl);

    fireEvent.click(
      screen.getByRole("button", { name: text.library.form.close }),
    );
    expect(revoke).toHaveBeenCalledWith(detail.previewUrl);
    view.unmount();
  });

  it("详情默认显示中文提示词，可切换英文并展示备注", async () => {
    const service = createService({
      getAssetDetail: vi.fn().mockResolvedValue({
        ...detail,
        notes: "这是编辑后保留的备注",
      }),
    });
    render(<LibraryPage service={service} onOpenSettings={vi.fn()} />);
    fireEvent.click(
      await screen.findByRole("button", {
        name: `${text.library.openDetail}：${asset.fileName}`,
      }),
    );
    expect(await screen.findByText(detail.promptZh)).toBeVisible();
    expect(screen.getByText("这是编辑后保留的备注")).toBeVisible();
    expect(
      screen.queryByText(text.library.detail.duration),
    ).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "English" }));
    expect(screen.getByText(detail.promptEn)).toBeVisible();
  });

  it("切换到另一作品详情时释放旧预览，并保留新预览", async () => {
    const secondAsset = { ...asset, id: "asset-2", fileName: "第二张.png" };
    const revoke = vi.fn();
    Object.defineProperty(URL, "revokeObjectURL", {
      configurable: true,
      value: revoke,
    });
    const service = createService({
      listAssetPage: vi.fn().mockResolvedValue({
        items: [asset, secondAsset],
        page: 1,
        pageSize: 25,
        totalCount: 2,
        totalPages: 1,
      }),
      getAssetDetail: vi.fn().mockImplementation(async (id) => ({
        ...detail,
        ...(id === secondAsset.id ? secondAsset : asset),
        title: id === secondAsset.id ? "第二张" : detail.title,
        previewUrl: id === secondAsset.id ? "blob:preview-2" : "blob:preview-1",
      })),
    });
    render(<LibraryPage service={service} onOpenSettings={vi.fn()} />);
    fireEvent.click(
      await screen.findByRole("button", {
        name: `${text.library.openDetail}：${asset.fileName}`,
      }),
    );
    await screen.findByRole("dialog", { name: detail.title });

    fireEvent.click(
      screen.getByRole("button", {
        name: `${text.library.openDetail}：${secondAsset.fileName}`,
      }),
    );

    await screen.findByRole("dialog", { name: "第二张" });
    expect(revoke).toHaveBeenCalledWith("blob:preview-1");
    expect(revoke).not.toHaveBeenCalledWith("blob:preview-2");
  });

  it("详情无法预览时显示 service 返回的安全原因", async () => {
    const service = createService({
      getAssetDetail: vi.fn().mockResolvedValue({
        ...detail,
        previewUrl: null,
        previewError: "媒体文件过大，无法在详情中直接预览。",
      }),
    });
    render(<LibraryPage service={service} onOpenSettings={vi.fn()} />);
    await screen.findByText(asset.fileName);
    fireEvent.click(
      screen.getByRole("button", {
        name: `${text.library.openDetail}：${asset.fileName}`,
      }),
    );
    expect(
      await screen.findByText("媒体文件过大，无法在详情中直接预览。"),
    ).toBeInTheDocument();
  });
  it("只读展示模式隐藏所有写入和管理入口，但保留筛选、预览与复制", async () => {
    const service = createService();
    Object.defineProperty(URL, "revokeObjectURL", {
      configurable: true,
      value: vi.fn(),
    });
    render(
      <LibraryPage
        service={service}
        onOpenSettings={vi.fn()}
        onOpenAiReview={vi.fn()}
        accessMode="readOnly"
      />,
    );

    await screen.findByText(asset.fileName);
    expect(
      screen.getByText(text.workspace.readOnlyDescription),
    ).toBeInTheDocument();
    expect(screen.getByLabelText(text.library.search)).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: text.library.importMedia }),
    ).toBeNull();
    expect(
      screen.queryByRole("button", { name: text.library.addProject }),
    ).toBeNull();
    expect(screen.queryByRole("button", { name: text.nav.review })).toBeNull();
    expect(
      screen.queryByRole("button", { name: text.library.tabs.taxonomy }),
    ).toBeNull();
    expect(
      screen.queryByRole("button", { name: text.library.tabs.trash }),
    ).toBeNull();

    fireEvent.click(
      screen.getByRole("button", {
        name: `${text.library.openDetail}：${asset.fileName}`,
      }),
    );
    await screen.findByRole("dialog", { name: detail.title });
    expect(
      screen.getByRole("button", { name: text.library.detail.copyZh }),
    ).toBeEnabled();
    expect(
      screen.queryByRole("button", { name: text.library.detail.copyPath }),
    ).toBeNull();
    expect(
      screen.queryByRole("button", { name: text.library.detail.edit }),
    ).toBeNull();
    expect(
      screen.queryByRole("button", { name: text.library.detail.delete }),
    ).toBeNull();
  });

  it("未打开工作区时通过路径连接，但不在完成页展示路径", async () => {
    const service = createService({
      getWorkspaceState: vi
        .fn()
        .mockResolvedValue({ isOpen: false, displayName: null }),
    });
    render(<LibraryPage service={service} onOpenSettings={vi.fn()} />);

    const input = await screen.findByLabelText(text.library.workspacePath);
    fireEvent.change(input, { target: { value: "D:\\Private\\AI-Gallery" } });
    fireEvent.click(
      screen.getByRole("button", { name: text.library.connectWorkspace }),
    );

    expect(await screen.findByText(asset.fileName)).toBeInTheDocument();
    expect(service.connectWorkspace).toHaveBeenCalledWith(
      "D:\\Private\\AI-Gallery",
    );
    expect(
      screen.queryByText("D:\\Private\\AI-Gallery"),
    ).not.toBeInTheDocument();
  });

  it("分页替换摘要以保持有界 DOM，卡片只渲染缩略图地址", async () => {
    const nextAsset = {
      ...asset,
      id: "asset-2",
      fileName: "夜航.mp4",
      mediaType: "video" as const,
      thumbnailUrl: null,
      coverUrl: "cover://asset-2",
    };
    const listAssets = vi
      .fn()
      .mockResolvedValueOnce({ items: [asset], nextCursor: "page-2" })
      .mockResolvedValueOnce({ items: [nextAsset], nextCursor: null });
    const service = createService({ listAssets });
    const { container } = render(
      <LibraryPage service={service} onOpenSettings={vi.fn()} />,
    );

    await screen.findByText(asset.fileName);
    fireEvent.click(
      screen.getByRole("button", { name: text.library.pageLabel(2) }),
    );

    expect(await screen.findByText(nextAsset.fileName)).toBeInTheDocument();
    expect(listAssets).toHaveBeenLastCalledWith({
      limit: 25,
    });
    expect(container.querySelector('img[src="thumb://asset-1"]')).toBeNull();
    expect(
      container.querySelector('img[src="cover://asset-2"]'),
    ).toBeInTheDocument();
  });

  it("应用智能集合后以完整结构化条件重新查询作品", async () => {
    const listAssets = vi
      .fn()
      .mockResolvedValue({ items: [asset], nextCursor: null });
    const service = createService({ listAssets });
    render(
      <LibraryPage
        service={service}
        onOpenSettings={vi.fn()}
        savedAssetFilter={{
          version: 1,
          keyword: "夜景",
          mediaType: "image",
          model: "flux",
          platform: "local",
          categoryIds: [3, 4],
          rating: 4,
          isFavorite: true,
          isPublic: false,
          createdAfter: Date.parse("2026-07-01T00:00:00Z"),
          createdBefore: Date.parse("2026-07-14T00:00:00Z"),
          minAspectRatio: 1,
          maxAspectRatio: 2,
        }}
      />,
    );

    await screen.findByText("已应用智能集合筛选。");
    await waitFor(() =>
      expect(listAssets).toHaveBeenCalledWith({
        limit: 25,
        keyword: "夜景",
        exactMatch: false,
        mediaType: "image",
        model: "flux",
        platform: "local",
        categoryIds: ["3", "4"],
        rating: 4,
        isFavorite: true,
        isPublic: false,
        createdAfter: Date.parse("2026-07-01T00:00:00"),
        createdBefore: Date.parse("2026-07-14T23:59:59.999"),
        minAspectRatio: 1,
        maxAspectRatio: 2,
      }),
    );
  });

  it("详情按需读取、复制有反馈，并在关闭时释放 blob 预览", async () => {
    const service = createService();
    const revoke = vi.fn();
    Object.defineProperty(URL, "revokeObjectURL", {
      configurable: true,
      value: revoke,
    });
    render(<LibraryPage service={service} onOpenSettings={vi.fn()} />);

    fireEvent.click(
      await screen.findByRole("button", {
        name: `${text.library.openDetail}：${asset.fileName}`,
      }),
    );
    expect(
      await screen.findByRole("dialog", { name: detail.title }),
    ).toBeInTheDocument();
    expect(service.getAssetDetail).toHaveBeenCalledWith(asset.id);

    fireEvent.click(
      screen.getByRole("button", { name: text.library.detail.copyZh }),
    );
    expect(await screen.findByRole("status")).toHaveTextContent(
      text.library.detail.copySuccess,
    );
    expect(service.copyText).toHaveBeenCalledWith(detail.promptZh);

    fireEvent.click(
      screen.getByRole("button", { name: text.library.detail.copyPath }),
    );
    expect(service.copyText).toHaveBeenLastCalledWith(detail.storedPath);

    fireEvent.click(
      screen.getByRole("button", { name: text.library.form.close }),
    );
    expect(revoke).toHaveBeenCalledWith(detail.previewUrl);
  });

  it("删除先显示确认，确认后才调用回收站服务", async () => {
    const service = createService();
    render(<LibraryPage service={service} onOpenSettings={vi.fn()} />);
    fireEvent.click(
      await screen.findByRole("button", {
        name: `${text.library.openDetail}：${asset.fileName}`,
      }),
    );
    fireEvent.click(
      await screen.findByRole("button", { name: text.library.detail.delete }),
    );

    expect(
      screen.getByRole("alertdialog", {
        name: text.library.trash.confirmTitle,
      }),
    ).toBeInTheDocument();
    expect(service.moveAssetToTrash).not.toHaveBeenCalled();
    fireEvent.click(
      screen.getByRole("button", { name: text.library.trash.confirmMove }),
    );
    await waitFor(() =>
      expect(service.moveAssetToTrash).toHaveBeenCalledWith(asset.id),
    );
  });

  it("批量移入回收站后重新读取当前页，实时更新总数和作品编号", async () => {
    const first = {
      ...asset,
      id: "asset-1",
      fileName: "一号.png",
      displayOrder: 1,
    };
    const second = {
      ...asset,
      id: "asset-2",
      fileName: "二号.png",
      displayOrder: 2,
    };
    const third = {
      ...asset,
      id: "asset-3",
      fileName: "三号.png",
      displayOrder: 3,
    };
    let trashed = false;
    const listAssetPage = vi.fn().mockImplementation(async () =>
      trashed
        ? {
            items: [first, { ...third, displayOrder: 2 }],
            page: 1,
            pageSize: 25,
            totalCount: 2,
            totalPages: 1,
          }
        : {
            items: [first, second, third],
            page: 1,
            pageSize: 25,
            totalCount: 3,
            totalPages: 1,
          },
    );
    const moveAssetsToTrash = vi.fn().mockImplementation(async () => {
      trashed = true;
    });
    const service = createService({ listAssetPage, moveAssetsToTrash });
    render(<LibraryPage service={service} onOpenSettings={vi.fn()} />);

    await screen.findByText("#3 · 三号.png");
    fireEvent.click(screen.getByLabelText("选择作品：二号.png"));
    fireEvent.click(
      screen.getByRole("button", {
        name: text.library.detail.moveSelectedToTrashAria,
      }),
    );
    fireEvent.click(
      screen.getByRole("button", { name: text.library.trash.confirmMove }),
    );

    await waitFor(() =>
      expect(moveAssetsToTrash).toHaveBeenCalledWith(["asset-2"]),
    );
    await screen.findByText("#2 · 三号.png");
    expect(screen.getByText(text.library.paginationSummary(2))).toBeVisible();
  });

  it("可全选当前筛选结果的所有分页作品", async () => {
    const second = { ...asset, id: "asset-2", fileName: "第二页.png" };
    const listAssetPage = vi.fn().mockImplementation(async (request) => ({
      items: request.page === 1 ? [asset] : [second],
      page: request.page,
      pageSize: request.pageSize,
      totalCount: 2,
      totalPages: 2,
    }));
    const service = createService({ listAssetPage });
    render(<LibraryPage service={service} onOpenSettings={vi.fn()} />);

    await screen.findByText(asset.fileName);
    fireEvent.click(
      screen.getByRole("button", {
        name: text.library.detail.selectAllResults,
      }),
    );
    await screen.findByText(text.library.detail.selectedAssets(2));
    expect(listAssetPage).toHaveBeenCalledWith(
      expect.objectContaining({ page: 2, pageSize: 25 }),
    );
  });

  it("项目删除从界面可达且必须确认", async () => {
    const service = createService();
    render(<LibraryPage service={service} onOpenSettings={vi.fn()} />);
    await screen.findByText(asset.fileName);
    fireEvent.click(
      screen.getByRole("button", { name: text.library.tabs.projects }),
    );
    fireEvent.click(
      screen.getByRole("button", { name: text.library.project.delete }),
    );

    expect(
      screen.getByRole("alertdialog", {
        name: text.library.trash.confirmProjectTitle,
      }),
    ).toBeInTheDocument();
    expect(service.moveProjectToTrash).not.toHaveBeenCalled();
    fireEvent.click(
      screen.getByRole("button", { name: text.library.trash.confirmMove }),
    );
    await waitFor(() =>
      expect(service.moveProjectToTrash).toHaveBeenCalledWith(project.id),
    );
  });

  it("导入完成后自动重读首页", async () => {
    const listAssets = vi
      .fn()
      .mockResolvedValue({ items: [asset], nextCursor: null });
    const service = createService({
      listAssets,
      startImport: vi.fn().mockResolvedValue({
        id: "import-1",
        state: "completed",
        completed: 1,
        total: 1,
        currentFileName: "new.png",
        message: "导入完成。",
      }),
    });
    render(<LibraryPage service={service} onOpenSettings={vi.fn()} />);
    await screen.findByText(asset.fileName);

    fireEvent.click(
      screen.getByRole("button", { name: text.library.importMedia }),
    );
    fireEvent.change(
      await screen.findByLabelText(text.library.import.sourcePaths),
      {
        target: { value: "D:\\media\\new.png" },
      },
    );
    fireEvent.click(
      screen.getByRole("button", { name: text.library.import.start }),
    );

    await waitFor(() => expect(listAssets).toHaveBeenCalledTimes(2));
  });

  it("点击分类进入作品详情，并可改名、合并或删除分类", async () => {
    const deleteCategory = vi.fn().mockResolvedValue(undefined);
    const updateCategory = vi.fn().mockResolvedValue(undefined);
    const listAssetPage = vi.fn().mockResolvedValue({
      items: [asset],
      page: 1,
      pageSize: 25,
      totalCount: 1,
      totalPages: 1,
    });
    const service = createService({
      listTaxonomy: vi.fn().mockResolvedValue({
        dimensions: [
          {
            id: "dimension-1",
            name: "视觉风格",
            allowsMultiple: true,
            aiCanSuggestNew: false,
            enabled: true,
            categories: [
              {
                id: "category-1",
                dimensionId: "dimension-1",
                name: "赛博朋克",
                color: null,
                assetCount: 0,
                enabled: true,
                aliases: [],
                description: "",
                icon: null,
              },
              {
                id: "category-2",
                dimensionId: "dimension-1",
                name: "极简",
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
        tags: [],
      }),
      getCategoryImpact: vi.fn().mockResolvedValue(7),
      deleteCategory,
      updateCategory,
      listAssetPage,
    });
    render(<LibraryPage service={service} onOpenSettings={vi.fn()} />);
    await screen.findByText(asset.fileName);
    fireEvent.click(
      screen.getByRole("button", { name: text.library.tabs.taxonomy }),
    );
    fireEvent.click(
      await screen.findByRole("button", {
        name: text.library.taxonomy.openCategory("赛博朋克"),
      }),
    );
    expect(
      await screen.findByText(text.library.taxonomy.categoryAssets(1)),
    ).toBeInTheDocument();
    expect(listAssetPage).toHaveBeenLastCalledWith({
      categoryIds: ["category-1"],
      page: 1,
      pageSize: 25,
    });

    fireEvent.click(
      screen.getByRole("button", { name: text.library.taxonomy.editCategory }),
    );
    expect(
      screen.getByRole("button", { name: text.library.taxonomy.delete }),
    ).toBeInTheDocument();
    fireEvent.change(
      screen.getByLabelText(text.library.taxonomy.categoryName),
      { target: { value: "未来都市" } },
    );
    fireEvent.click(
      screen.getByRole("button", {
        name: text.library.taxonomy.saveCategoryName,
      }),
    );
    await waitFor(() =>
      expect(updateCategory).toHaveBeenCalledWith(
        expect.objectContaining({ id: "category-1" }),
        "未来都市",
      ),
    );

    fireEvent.click(
      screen.getByRole("button", { name: text.library.taxonomy.editCategory }),
    );
    fireEvent.click(
      screen.getByRole("button", { name: text.library.taxonomy.delete }),
    );
    expect(
      await screen.findByText(text.library.taxonomy.deleteConfirmDescription, {
        exact: false,
      }),
    ).toBeInTheDocument();
    fireEvent.click(
      screen.getByRole("button", { name: text.library.form.cancel }),
    );

    fireEvent.click(
      screen.getByRole("button", { name: text.library.taxonomy.editCategory }),
    );
    fireEvent.change(screen.getByLabelText(text.library.taxonomy.mergeTarget), {
      target: { value: "category-2" },
    });
    fireEvent.click(
      screen.getByRole("button", {
        name: text.library.taxonomy.mergeCategory,
      }),
    );
    expect(await screen.findByText(/将影响 7 件作品/)).toBeInTheDocument();
    fireEvent.click(
      screen.getByRole("button", { name: text.library.taxonomy.confirmMerge }),
    );

    await waitFor(() =>
      expect(deleteCategory).toHaveBeenCalledWith({
        categoryId: "category-1",
        replacementCategoryId: "category-2",
      }),
    );
  });

  it("新建分类可明确选择所属维度", async () => {
    const created = {
      id: "category-new",
      dimensionId: "dimension-2",
      name: "角色设定",
      color: null,
      assetCount: 0,
      enabled: true,
      aliases: [],
      description: "",
      icon: null,
    };
    const createCategory = vi.fn().mockResolvedValue(created);
    const service = createService({
      listTaxonomy: vi.fn().mockResolvedValue({
        dimensions: [
          {
            id: "dimension-1",
            name: "视觉风格",
            categories: [],
            allowsMultiple: true,
            aiCanSuggestNew: false,
            enabled: true,
          },
          {
            id: "dimension-2",
            name: "应用场景",
            categories: [],
            allowsMultiple: true,
            aiCanSuggestNew: false,
            enabled: true,
          },
        ],
        tags: [],
      }),
      createCategory,
    });
    render(<LibraryPage service={service} onOpenSettings={vi.fn()} />);
    await screen.findByText(asset.fileName);
    fireEvent.click(
      screen.getByRole("button", { name: text.library.tabs.taxonomy }),
    );
    fireEvent.change(
      await screen.findByLabelText(text.library.taxonomy.targetDimension),
      { target: { value: "dimension-2" } },
    );
    fireEvent.change(
      screen.getByLabelText(text.library.taxonomy.categoryName),
      {
        target: { value: "角色设定" },
      },
    );
    fireEvent.click(
      screen.getByRole("button", { name: text.library.taxonomy.addCategory }),
    );
    await waitFor(() =>
      expect(createCategory).toHaveBeenCalledWith({
        dimensionId: "dimension-2",
        name: "角色设定",
      }),
    );
  });

  it("编辑作品后关闭会确认未保存更改", async () => {
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(false);
    const service = createService();
    render(<LibraryPage service={service} onOpenSettings={vi.fn()} />);
    fireEvent.click(
      await screen.findByRole("button", {
        name: `${text.library.openDetail}：${asset.fileName}`,
      }),
    );
    fireEvent.click(
      await screen.findByRole("button", { name: text.library.detail.edit }),
    );
    const title = await screen.findByLabelText(text.library.form.title);
    fireEvent.change(title, { target: { value: "未保存的新标题" } });
    const closeButtons = screen.getAllByRole("button", {
      name: text.library.form.close,
    });
    fireEvent.click(closeButtons[closeButtons.length - 1]);

    expect(confirm).toHaveBeenCalledWith(text.library.form.confirmDiscard);
    expect(screen.getByDisplayValue("未保存的新标题")).toBeInTheDocument();
    confirm.mockRestore();
  });

  it("图片识别结果经用户应用后才写入正式作品字段", async () => {
    const pendingDetail: AssetDetail = {
      ...detail,
      promptEn: "",
      negativePrompt: "",
      generationParamsJson: JSON.stringify({
        _pendingRecognition: {
          promptZh: "",
          promptEn: "future skyline",
          negativePrompt: "blur",
          generationParams: { Steps: 20, Seed: 42 },
        },
      }),
    };
    const updateAsset = vi.fn().mockResolvedValue({
      ...pendingDetail,
      promptEn: "future skyline",
      negativePrompt: "blur",
      generationParamsJson: '{"Steps":20,"Seed":42}',
    });
    const service = createService({
      getAssetDetail: vi.fn().mockResolvedValue(pendingDetail),
      updateAsset,
    });
    render(<LibraryPage service={service} onOpenSettings={vi.fn()} />);
    fireEvent.click(
      await screen.findByRole("button", {
        name: `${text.library.openDetail}：${asset.fileName}`,
      }),
    );
    fireEvent.click(
      await screen.findByRole("button", { name: text.library.detail.edit }),
    );

    expect(
      await screen.findByText(text.library.form.recognitionTitle),
    ).toBeInTheDocument();
    expect(screen.getByLabelText(text.library.form.promptEn)).toHaveValue("");
    fireEvent.click(
      screen.getByRole("button", {
        name: text.library.form.recognitionAccept,
      }),
    );
    expect(screen.getByLabelText(text.library.form.promptEn)).toHaveValue(
      "future skyline",
    );
    fireEvent.click(
      screen.getByRole("button", { name: text.library.form.save }),
    );

    await waitFor(() => expect(updateAsset).toHaveBeenCalledTimes(1));
    expect(updateAsset.mock.calls[0][1]).toMatchObject({
      promptEn: "future skyline",
      negativePrompt: "blur",
    });
    expect(updateAsset.mock.calls[0][1].generationParamsJson).not.toContain(
      "_pendingRecognition",
    );
  });

  it("搜索和组合筛选会以首游标重新请求资产页", async () => {
    const listAssets = vi
      .fn()
      .mockResolvedValue({ items: [asset], nextCursor: "page-2" });
    const service = createService({ listAssets });
    render(<LibraryPage service={service} onOpenSettings={vi.fn()} />);
    await screen.findByText(asset.fileName);
    fireEvent.change(screen.getByPlaceholderText(text.library.search), {
      target: { value: "晨雾" },
    });
    fireEvent.change(screen.getByLabelText(text.library.filterType), {
      target: { value: "image" },
    });
    fireEvent.click(
      screen.getByRole("button", { name: text.library.filterModel }),
    );
    fireEvent.click(screen.getByRole("option", { name: "Flux" }));
    fireEvent.click(
      screen.getByRole("button", { name: text.library.filterPlatform }),
    );
    fireEvent.click(screen.getByRole("option", { name: "Local" }));
    fireEvent.change(screen.getByLabelText(text.library.filterRating), {
      target: { value: "4" },
    });
    fireEvent.change(screen.getByLabelText(text.library.filterFavorite), {
      target: { value: "true" },
    });
    await waitFor(() =>
      expect(listAssets).toHaveBeenLastCalledWith({
        limit: 25,
        keyword: "晨雾",
        exactMatch: false,
        mediaType: "image",
        model: "Flux",
        platform: "Local",
        rating: 4,
        isFavorite: true,
      }),
    );
  });

  it("重复组仅显示摘要，并可打开代表作品详情", async () => {
    const listDuplicateGroups = vi.fn().mockResolvedValue({
      items: [
        {
          contentHash: "hash-a",
          assetCount: 2,
          representativeAssetId: asset.id,
          representativeFileName: asset.fileName,
          updatedAt: asset.updatedAt,
        },
      ],
      nextCursor: null,
    });
    const service = createService({ listDuplicateGroups });
    render(<LibraryPage service={service} onOpenSettings={vi.fn()} />);
    await screen.findByText(asset.fileName);
    fireEvent.click(
      screen.getByRole("button", { name: text.library.duplicateGroups }),
    );
    expect(
      await screen.findByRole("heading", {
        name: text.library.duplicateGroupsTitle,
      }),
    ).toBeInTheDocument();
    expect(listDuplicateGroups).toHaveBeenCalledWith({ limit: 24 });
    fireEvent.click(screen.getByRole("button", { name: /2 个相同文件/ }));
    await waitFor(() =>
      expect(service.getAssetDetail).toHaveBeenCalledWith(asset.id),
    );
    expect(service.moveAssetToTrash).not.toHaveBeenCalled();
  });

  it("可见卡片按需读取缩略图，并在切换页面后保留有界会话缓存", async () => {
    const withoutThumbnail = { ...asset, thumbnailUrl: null };
    const getAssetThumbnail = vi.fn().mockResolvedValue("blob:thumbnail-1");
    const service = createService({
      listAssets: vi
        .fn()
        .mockResolvedValue({ items: [withoutThumbnail], nextCursor: null }),
      getAssetThumbnail,
    });
    const revoke = vi.fn();
    Object.defineProperty(URL, "revokeObjectURL", {
      configurable: true,
      value: revoke,
    });
    const view = render(
      <LibraryPage service={service} onOpenSettings={vi.fn()} />,
    );
    await screen.findByText(asset.fileName);
    await waitFor(() =>
      expect(getAssetThumbnail).toHaveBeenCalledWith(withoutThumbnail.id),
    );
    view.unmount();
    expect(revoke).not.toHaveBeenCalledWith("blob:thumbnail-1");
  });

  it("图片缩略图首次 pending 后使用有界重试，并在就绪后显示", async () => {
    vi.useFakeTimers();
    try {
      const withoutThumbnail = { ...asset, thumbnailUrl: null };
      const getAssetThumbnail = vi
        .fn()
        .mockResolvedValueOnce(null)
        .mockResolvedValueOnce("blob:thumbnail-ready");
      const service = createService({
        listAssets: vi
          .fn()
          .mockResolvedValue({ items: [withoutThumbnail], nextCursor: null }),
        getAssetThumbnail,
      });
      const { container } = render(
        <LibraryPage service={service} onOpenSettings={vi.fn()} />,
      );

      await act(async () => {
        for (let index = 0; index < 10; index += 1) await Promise.resolve();
      });
      expect(getAssetThumbnail).toHaveBeenCalledTimes(1);
      expect(container.querySelector(".asset-thumbnail img")).toBeNull();

      await act(async () => {
        await vi.advanceTimersByTimeAsync(250);
      });

      expect(getAssetThumbnail).toHaveBeenCalledTimes(2);
      expect(
        container.querySelector('img[src="blob:thumbnail-ready"]'),
      ).toBeInTheDocument();
    } finally {
      vi.useRealTimers();
    }
  });

  it("缩略图等待队列满时不会继续向 service 发起请求", async () => {
    const widthDescriptor = Object.getOwnPropertyDescriptor(
      HTMLElement.prototype,
      "clientWidth",
    );
    Object.defineProperty(HTMLElement.prototype, "clientWidth", {
      configurable: true,
      value: 1_000,
    });

    const deferred: Array<(url: string) => void> = [];
    let requestCount = 0;
    const getAssetThumbnail = vi.fn(() => {
      requestCount += 1;
      if (requestCount <= 2) {
        return new Promise<string>((resolve) => deferred.push(resolve));
      }
      return Promise.resolve(`blob:thumbnail-${requestCount}`);
    });
    const createAssets = (prefix: string) =>
      Array.from({ length: 48 }, (_, index) => ({
        ...asset,
        id: `${prefix}-${index}`,
        fileName: `${prefix}-${index}.png`,
        thumbnailUrl: null,
      }));
    const firstService = createService({
      listAssets: vi
        .fn()
        .mockResolvedValue({ items: createAssets("first"), nextCursor: null }),
      getAssetThumbnail,
    });
    const secondService = createService({
      listAssets: vi
        .fn()
        .mockResolvedValue({ items: createAssets("second"), nextCursor: null }),
      getAssetThumbnail,
    });
    const thirdService = createService({
      listAssets: vi
        .fn()
        .mockResolvedValue({ items: createAssets("third"), nextCursor: null }),
      getAssetThumbnail,
    });

    let view: ReturnType<typeof render> | null = null;
    try {
      view = render(
        <>
          <LibraryPage service={firstService} onOpenSettings={vi.fn()} />
          <LibraryPage service={secondService} onOpenSettings={vi.fn()} />
          <LibraryPage service={thirdService} onOpenSettings={vi.fn()} />
        </>,
      );
      await act(async () => {
        for (let index = 0; index < 10; index += 1) await Promise.resolve();
      });
      expect(screen.getByText("third-15.png")).toBeInTheDocument();

      // 两个任务运行时，其余任务只允许在 32 个等待槽内排队。
      expect(getAssetThumbnail).toHaveBeenCalledTimes(2);

      await act(async () => {
        deferred.forEach((resolve) => resolve("blob:thumbnail-initial"));
        for (let index = 0; index < 50; index += 1) await Promise.resolve();
      });
      expect(getAssetThumbnail).toHaveBeenCalledTimes(34);
    } finally {
      view?.unmount();
      if (widthDescriptor) {
        Object.defineProperty(
          HTMLElement.prototype,
          "clientWidth",
          widthDescriptor,
        );
      } else {
        delete (HTMLElement.prototype as { clientWidth?: number }).clientWidth;
      }
    }
  });

  it("分类与回收站隐藏页头写入按钮，并可切换管理元数据预设", async () => {
    const service = createService();
    render(<LibraryPage service={service} onOpenSettings={vi.fn()} />);

    fireEvent.click(await screen.findByRole("button", { name: "分类与标签" }));
    await screen.findByText(text.library.taxonomy.description);
    expect(
      screen.queryByRole("button", { name: text.library.addProject }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: text.library.importMedia }),
    ).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "元数据预设" }));
    expect(await screen.findByText("Flux")).toBeInTheDocument();
    expect(screen.getByText("Local")).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("模型预设名称"), {
      target: { value: "Seedream 5.0 Lite" },
    });
    fireEvent.click(screen.getAllByRole("button", { name: "添加预设" })[0]);
    await waitFor(() =>
      expect(service.createMetadataPreset).toHaveBeenCalledWith(
        "model",
        "Seedream 5.0 Lite",
      ),
    );

    fireEvent.click(screen.getByRole("button", { name: "回收站" }));
    await screen.findByText(text.library.trash.description);
    expect(
      screen.queryByRole("button", { name: text.library.addProject }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: text.library.importMedia }),
    ).not.toBeInTheDocument();
  });

  it("作品名默认模糊搜索，模型和平台筛选使用预设下拉项", async () => {
    const listAssetPage = vi.fn().mockResolvedValue({
      items: [asset],
      page: 1,
      pageSize: 25,
      totalCount: 1,
      totalPages: 1,
    });
    const service = createService({ listAssetPage });
    render(<LibraryPage service={service} onOpenSettings={vi.fn()} />);

    const search =
      await screen.findByPlaceholderText("按作品名搜索（不区分大小写）");
    fireEvent.change(search, { target: { value: "晨雾城市.png" } });
    fireEvent.click(screen.getByRole("button", { name: "模型" }));
    fireEvent.click(await screen.findByRole("option", { name: "Flux" }));
    fireEvent.click(screen.getByRole("button", { name: "平台" }));
    fireEvent.click(await screen.findByRole("option", { name: "Local" }));

    await waitFor(() =>
      expect(listAssetPage).toHaveBeenLastCalledWith(
        expect.objectContaining({
          keyword: "晨雾城市.png",
          exactMatch: false,
          model: "Flux",
          platform: "Local",
        }),
      ),
    );
  });
});
