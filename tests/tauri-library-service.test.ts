import { afterEach, describe, expect, it, vi } from "vitest";
import type {
  CommandArguments,
  CommandClient,
} from "../src/services/command-client";
import type { LibraryService } from "../src/services/library-service";
import { ServiceError, serviceErrorCode } from "../src/services/service-error";
import { createTauriLibraryService } from "../src/services/tauri-library-service";

const workspaceRoot = "D:\\Private\\AI-Gallery";
const privateMediaPath = "D:\\Private\\originals\\secret.png";

function commandClient(): {
  readonly client: CommandClient;
  readonly invoke: ReturnType<typeof vi.fn>;
} {
  const invoke = vi.fn();
  return { client: { invoke }, invoke };
}

async function connect(service: LibraryService): Promise<void> {
  await service.connectWorkspace(workspaceRoot);
}

function projectResponse(overrides: Record<string, unknown> = {}) {
  return {
    id: 11,
    title: "城市实验",
    description: "光线与建筑实验",
    rating: 4,
    isFavorite: false,
    isPublic: false,
    notes: "",
    prompt: { promptZh: "", promptEn: "", negativePrompt: "" },
    categoryIds: [],
    tagIds: [],
    assetCount: 3,
    updatedAt: 1_720_000_000,
    ...overrides,
  };
}

function assetResponse(overrides: Record<string, unknown> = {}) {
  return {
    id: 7,
    projectId: null,
    fileName: "晨雾城市.png",
    mediaType: "image",
    pathKind: "external",
    storedPath: privateMediaPath,
    mimeType: "image/png",
    fileSize: 3,
    contentHash: "hash",
    width: 1280,
    height: 960,
    durationMs: null,
    frameRate: null,
    hasAudio: null,
    model: "Flux",
    platform: "Local",
    generationParams: { seed: 42 },
    rating: 4,
    isFavorite: true,
    isPublic: false,
    notes: "",
    updatedAt: 1_720_000_000,
    prompt: {
      promptZh: "清晨薄雾中的未来城市",
      promptEn: "future city in morning fog",
      negativePrompt: "low quality",
    },
    categoryIds: [2],
    tagIds: [3],
    ...overrides,
  };
}

function importTaskResponse(overrides: Record<string, unknown> = {}) {
  return {
    id: "import-1",
    state: "running",
    completed: 1,
    total: 2,
    currentFileName: "晨雾城市.png",
    message: null,
    ...overrides,
  };
}

function withWorkspaceCommands(
  command: string,
  response: unknown,
): ReturnType<typeof vi.fn> {
  return vi.fn(async (name: string) => {
    if (name === "open_workspace") return { displayName: "作品库" };
    if (name === "prepare_workspace_database") return {};
    if (name === command) return response;
    throw new Error(`测试未配置命令：${name}`);
  });
}

afterEach(() => {
  vi.useRealTimers();
});

describe("真实 Tauri 作品库服务", () => {
  it("隔离服务在连接前由后端拒绝与正常工作区相同或嵌套的根目录", async () => {
    const { client, invoke } = commandClient();
    invoke.mockImplementation(async (name: string) => {
      if (name === "validate_isolated_workspace") return {};
      if (name === "open_workspace") return { displayName: "私密作品库" };
      if (name === "prepare_workspace_database") return {};
      throw new Error(`测试未配置命令：${name}`);
    });
    const service = createTauriLibraryService(client, () => "readWrite", {
      isolatedFromRoot: () => "D:\\Normal-Gallery",
    });

    await service.connectWorkspace("D:\\Private-Gallery");

    expect(invoke).toHaveBeenNthCalledWith(1, "validate_isolated_workspace", {
      request: {
        primaryRootPath: "D:\\Normal-Gallery",
        isolatedRootPath: "D:\\Private-Gallery",
      },
    });
    expect(invoke).toHaveBeenNthCalledWith(2, "open_workspace", {
      request: { rootPath: "D:\\Private-Gallery" },
    });
  });

  it("正常与私密服务实例始终使用各自的工作区根目录", async () => {
    const normalRoot = "D:\\Normal-Gallery";
    const isolatedRoot = "D:\\Private-Gallery";
    const invoke = vi.fn(async (name: string, arguments_?: unknown) => {
      if (name === "validate_isolated_workspace") return {};
      if (name === "open_workspace") return { displayName: "作品库" };
      if (name === "prepare_workspace_database") return {};
      if (name === "library_list_assets_numbered") {
        const request = (arguments_ as { request: { rootPath: string } })
          .request;
        const totalCount = request.rootPath === normalRoot ? 40 : 1;
        return {
          items: [],
          page: 1,
          pageSize: 25,
          totalCount,
          totalPages: Math.ceil(totalCount / 25),
        };
      }
      throw new Error(`测试未配置命令：${name}`);
    });
    const client: CommandClient = {
      invoke: async <TResult>(command: string, arguments_?: CommandArguments) =>
        (await invoke(command, arguments_)) as TResult,
    };
    const normalService = createTauriLibraryService(client);
    const isolatedService = createTauriLibraryService(
      client,
      () => "readWrite",
      { isolatedFromRoot: () => normalRoot },
    );

    await normalService.connectWorkspace(normalRoot);
    await isolatedService.connectWorkspace(isolatedRoot);

    await expect(
      normalService.listAssetPage({ page: 1, pageSize: 25 }),
    ).resolves.toMatchObject({ totalCount: 40 });
    await expect(
      isolatedService.listAssetPage({ page: 1, pageSize: 25 }),
    ).resolves.toMatchObject({ totalCount: 1 });

    const listRoots = invoke.mock.calls
      .filter(([name]) => name === "library_list_assets_numbered")
      .map(
        ([, arguments_]) =>
          (arguments_ as { request: { rootPath: string } }).request.rootPath,
      );
    expect(listRoots).toEqual([normalRoot, isolatedRoot]);
  });

  it("释放私密服务时取消由该实例启动且仍在运行的导入任务", async () => {
    const { client, invoke } = commandClient();
    invoke.mockImplementation(async (name: string) => {
      if (name === "open_workspace") return { displayName: "私密作品库" };
      if (name === "prepare_workspace_database") return {};
      if (name === "start_media_import") return importTaskResponse();
      if (name === "cancel_media_import") return {};
      throw new Error(`测试未配置命令：${name}`);
    });
    const service = createTauriLibraryService(client);
    await service.connectWorkspace(workspaceRoot);
    await service.startImport({
      sourcePaths: [privateMediaPath],
      mode: "copy",
      projectId: null,
    });

    await service.dispose();

    expect(invoke).toHaveBeenCalledWith("cancel_media_import", {
      request: { taskId: "import-1" },
    });
  });

  it("退出恰好发生在导入任务创建期间时，拿到任务 ID 后仍会立即取消", async () => {
    const { client, invoke } = commandClient();
    let resolveStart!: (value: ReturnType<typeof importTaskResponse>) => void;
    const startResponse = new Promise<ReturnType<typeof importTaskResponse>>(
      (resolve) => {
        resolveStart = resolve;
      },
    );
    invoke.mockImplementation(async (name: string) => {
      if (name === "open_workspace") return { displayName: "私密作品库" };
      if (name === "prepare_workspace_database") return {};
      if (name === "start_media_import") return await startResponse;
      if (name === "cancel_media_import") return {};
      throw new Error(`测试未配置命令：${name}`);
    });
    const service = createTauriLibraryService(client);
    await service.connectWorkspace(workspaceRoot);

    const starting = service.startImport({
      sourcePaths: [privateMediaPath],
      mode: "copy",
      projectId: null,
    });
    await vi.waitFor(() =>
      expect(invoke).toHaveBeenCalledWith(
        "start_media_import",
        expect.anything(),
      ),
    );
    await service.dispose();
    resolveStart(importTaskResponse());

    await expect(starting).rejects.toMatchObject({
      message: "当前作品库会话已经关闭。",
    });
    expect(invoke).toHaveBeenCalledWith("cancel_media_import", {
      request: { taskId: "import-1" },
    });
  });

  it("前端服务在只读模式下提前拒绝写命令", async () => {
    const { client, invoke } = commandClient();
    const service = createTauriLibraryService(client, () => "readOnly");

    await expect(
      service.createProject({
        kind: "simple",
        title: "只读测试",
        description: "",
        rating: 0,
        isFavorite: false,
        isPublic: false,
        promptZh: "",
        promptEn: "",
        negativePrompt: "",
        notes: "",
        categoryIds: [],
        tagIds: [],
      }),
    ).rejects.toMatchObject({ code: "WORKSPACE_READ_ONLY" });
    expect(invoke).not.toHaveBeenCalled();
  });

  it("项目详情更新保留提示词、备注和分类标签", async () => {
    const { client, invoke } = commandClient();
    invoke
      .mockResolvedValueOnce({ displayName: "作品库" })
      .mockResolvedValueOnce({})
      .mockResolvedValueOnce(
        projectResponse({
          prompt: {
            promptZh: "中文主提示词",
            promptEn: "main prompt",
            negativePrompt: "blur",
          },
          notes: "不得丢失",
          categoryIds: [2],
          tagIds: [3],
        }),
      )
      .mockResolvedValueOnce(projectResponse({ title: "新标题" }));
    const service = createTauriLibraryService(client);
    await connect(service);

    const detail = await service.getProjectDetail("11");
    expect(detail.kind).toBe("simple");
    await service.updateProject("11", { ...detail, title: "新标题" });

    expect(invoke).toHaveBeenLastCalledWith("library_update_project", {
      request: expect.objectContaining({
        input: expect.objectContaining({
          kind: "simple",
          prompt: {
            promptZh: "中文主提示词",
            promptEn: "main prompt",
            negativePrompt: "blur",
          },
          notes: "不得丢失",
          categoryIds: [2],
          tagIds: [3],
        }),
      }),
    });
  });

  it("画布成员使用分页摘要，并在角色与提示词写入时明确确认", async () => {
    const { client, invoke } = commandClient();
    invoke
      .mockResolvedValueOnce({ displayName: "作品库" })
      .mockResolvedValueOnce({})
      .mockResolvedValueOnce({
        items: [
          {
            assetId: 7,
            displayOrder: 2,
            fileName: "输出.png",
            mediaType: "image",
            modelName: null,
            platformName: null,
            width: 1024,
            height: 768,
            durationMs: null,
            updatedAt: 1_720_000_000,
            role: "output",
            referenceName: null,
            promptZh: "",
            promptEn: "",
            negativePrompt: "",
          },
        ],
        nextCursor: null,
      })
      .mockResolvedValueOnce({})
      .mockResolvedValueOnce({});
    const service = createTauriLibraryService(client);
    await connect(service);

    const page = await service.listCanvasMembers("11", {
      limit: 25,
      role: "output",
    });
    expect(page.items[0]).toMatchObject({ modelName: "", platformName: "" });
    await service.setCanvasMember("11", "7", "reference", "构图");
    await service.updateCanvasOutputPrompt("11", "7", {
      promptZh: "@构图 城市",
      promptEn: "",
      negativePrompt: "",
    });

    expect(invoke).toHaveBeenCalledWith("library_list_canvas_members", {
      request: {
        rootPath: workspaceRoot,
        projectId: 11,
        cursor: undefined,
        limit: 25,
        role: "output",
      },
    });

    expect(invoke).toHaveBeenCalledWith("library_set_canvas_member", {
      request: {
        rootPath: workspaceRoot,
        projectId: 11,
        assetId: 7,
        role: "reference",
        referenceName: "构图",
        confirmed: true,
      },
    });
    expect(invoke).toHaveBeenCalledWith("library_update_canvas_output_prompt", {
      request: {
        rootPath: workspaceRoot,
        projectId: 11,
        assetId: 7,
        prompt: {
          promptZh: "@构图 城市",
          promptEn: "",
          negativePrompt: "",
        },
        confirmed: true,
      },
    });
  });

  it("解析分页游标并在下一页请求中原样恢复稳定键", async () => {
    const { client, invoke } = commandClient();
    invoke
      .mockResolvedValueOnce({ displayName: "作品库" })
      .mockResolvedValueOnce({})
      .mockResolvedValueOnce({
        items: [projectResponse()],
        nextCursor: { updatedAt: 1_720_000_000, id: 11 },
      })
      .mockResolvedValueOnce({ items: [], nextCursor: null });
    const service = createTauriLibraryService(client);
    await connect(service);

    const firstPage = await service.listProjects({ limit: 24 });
    expect(firstPage.nextCursor).toBe("1720000000:11");

    await service.listProjects({
      cursor: firstPage.nextCursor ?? undefined,
      limit: 24,
    });
    expect(invoke).toHaveBeenLastCalledWith("library_list_projects", {
      request: {
        rootPath: workspaceRoot,
        cursor: { updatedAt: 1_720_000_000, id: 11 },
        limit: 24,
      },
    });
  });

  it.each(["missing-separator", "1.5:2", "1:2.5", "9007199254740992:1"])(
    "在发出 IPC 前拒绝非法分页游标：%s",
    async (cursor) => {
      const { client, invoke } = commandClient();
      invoke
        .mockResolvedValueOnce({ displayName: "作品库" })
        .mockResolvedValueOnce({});
      const service = createTauriLibraryService(client);
      await connect(service);

      await expect(
        service.listAssets({ cursor, limit: 24 }),
      ).rejects.toMatchObject({ code: serviceErrorCode.invalidResponse });
      expect(invoke).toHaveBeenCalledTimes(2);
    },
  );

  it.each([
    { items: "not-an-array", nextCursor: null },
    { items: [{}], nextCursor: null },
    { items: [projectResponse({ rating: "4" })], nextCursor: null },
    { items: [projectResponse()], nextCursor: { updatedAt: "bad", id: 11 } },
  ])("拒绝不符合项目分页契约的 unknown DTO %#", async (response) => {
    const { client } = commandClient();
    client.invoke = withWorkspaceCommands("library_list_projects", response);
    const service = createTauriLibraryService(client);
    await connect(service);

    await expect(service.listProjects({ limit: 24 })).rejects.toMatchObject({
      code: serviceErrorCode.invalidResponse,
    });
  });

  it("列表摘要丢弃外部绝对路径且只保留轻量字段", async () => {
    const { client } = commandClient();
    client.invoke = withWorkspaceCommands("library_list_assets", {
      items: [assetResponse()],
      nextCursor: null,
    });
    const service = createTauriLibraryService(client);
    await connect(service);

    const page = await service.listAssets({ limit: 24 });
    expect(page.items[0]).toMatchObject({
      id: "7",
      fileName: "晨雾城市.png",
      thumbnailUrl: null,
      coverUrl: null,
    });
    expect(JSON.stringify(page)).not.toContain(privateMediaPath);
    expect(page.items[0]).not.toHaveProperty("storedPath");
  });

  it("数字分页把组合筛选映射到固定 IPC 契约并严格解析总数", async () => {
    const { client, invoke } = commandClient();
    invoke.mockImplementation(async (name: string) => {
      if (name === "open_workspace") return { displayName: "作品库" };
      if (name === "prepare_workspace_database") return {};
      if (name === "library_list_assets_numbered") {
        return {
          items: [assetResponse()],
          page: 2,
          pageSize: 10,
          totalCount: 21,
          totalPages: 3,
        };
      }
      throw new Error(`测试未配置命令：${name}`);
    });
    const service = createTauriLibraryService(client);
    await connect(service);

    await expect(
      service.listAssetPage({
        projectId: "9",
        page: 2,
        pageSize: 10,
        keyword: "夜景",
        exactMatch: false,
        mediaType: "image",
        categoryIds: ["3", "4"],
        rating: 4,
        isFavorite: true,
        isPublic: false,
        minAspectRatio: 1,
        maxAspectRatio: 2,
      }),
    ).resolves.toMatchObject({ page: 2, pageSize: 10, totalPages: 3 });
    expect(invoke).toHaveBeenLastCalledWith("library_list_assets_numbered", {
      request: {
        rootPath: workspaceRoot,
        projectId: 9,
        mediaType: "image",
        keyword: "夜景",
        searchField: "title",
        exactMatch: false,
        model: null,
        platform: null,
        categoryIds: [3, 4],
        rating: 4,
        isFavorite: true,
        isPublic: false,
        createdAfter: null,
        createdBefore: null,
        minAspectRatio: 1,
        maxAspectRatio: 2,
        page: 2,
        pageSize: 10,
      },
    });
  });

  it("详情按需把二进制响应转换为 blob 预览，且不暴露媒体路径", async () => {
    const { client, invoke } = commandClient();
    let createdBlob: Blob | null = null;
    Object.defineProperty(URL, "createObjectURL", {
      configurable: true,
      value: vi.fn((value: Blob | MediaSource) => {
        if (value instanceof Blob) createdBlob = value;
        return "blob:m2-preview";
      }),
    });
    invoke.mockImplementation(async (name: string) => {
      if (name === "open_workspace") return { displayName: "作品库" };
      if (name === "prepare_workspace_database") return {};
      if (name === "library_get_asset") return assetResponse();
      if (name === "library_read_asset_preview") return [137, 80, 78];
      throw new Error(`测试未配置命令：${name}`);
    });
    const service = createTauriLibraryService(client);
    await connect(service);

    const detail = await service.getAssetDetail("7");
    expect(detail.previewUrl).toBe("blob:m2-preview");
    expect(createdBlob).toBeInstanceOf(Blob);
    expect(createdBlob).toMatchObject({ size: 3, type: "image/png" });
    expect(JSON.stringify(detail)).not.toContain(privateMediaPath);
    expect(invoke).toHaveBeenLastCalledWith("library_read_asset_preview", {
      request: { rootPath: workspaceRoot, id: 7 },
    });
    Reflect.deleteProperty(URL, "createObjectURL");
  });

  it("旧记录缺少 MIME 时从受支持的文件扩展名安全推断预览类型", async () => {
    let createdBlob: Blob | null = null;
    Object.defineProperty(URL, "createObjectURL", {
      configurable: true,
      value: vi.fn((value: Blob | MediaSource) => {
        if (value instanceof Blob) createdBlob = value;
        return "blob:legacy-preview";
      }),
    });
    const { client, invoke } = commandClient();
    invoke.mockImplementation(async (name: string) => {
      if (name === "open_workspace") return { displayName: "作品库" };
      if (name === "prepare_workspace_database") return {};
      if (name === "library_get_asset") {
        return assetResponse({ mimeType: null, fileName: "旧作品.jpg" });
      }
      if (name === "library_read_asset_preview") return [255, 216, 255];
      throw new Error(`测试未配置命令：${name}`);
    });
    const service = createTauriLibraryService(client);
    await connect(service);

    await expect(service.getAssetDetail("7")).resolves.toMatchObject({
      previewUrl: "blob:legacy-preview",
      previewError: null,
    });
    expect(createdBlob).toMatchObject({ type: "image/jpeg" });
    Reflect.deleteProperty(URL, "createObjectURL");
  });

  it("预览读取失败时返回无预览详情，且不传播底层绝对路径", async () => {
    const { client, invoke } = commandClient();
    invoke.mockImplementation(async (name: string) => {
      if (name === "open_workspace") return { displayName: "作品库" };
      if (name === "prepare_workspace_database") return {};
      if (name === "library_get_asset") return assetResponse();
      if (name === "library_read_asset_preview") {
        throw new Error(privateMediaPath);
      }
      throw new Error(`测试未配置命令：${name}`);
    });
    const service = createTauriLibraryService(client);
    await connect(service);

    const detail = await service.getAssetDetail("7");
    expect(detail.previewUrl).toBeNull();
    expect(JSON.stringify(detail)).not.toContain(privateMediaPath);
  });

  it("轮询导入任务直到终态并在完成后停止计时器", async () => {
    vi.useFakeTimers();
    const { client, invoke } = commandClient();
    let pollCount = 0;
    invoke.mockImplementation(async (name: string) => {
      if (name === "open_workspace") return { displayName: "作品库" };
      if (name === "prepare_workspace_database") return {};
      if (name === "get_media_import_task") {
        pollCount += 1;
        return pollCount === 1
          ? importTaskResponse()
          : importTaskResponse({ state: "completed", completed: 2 });
      }
      throw new Error(`测试未配置命令：${name}`);
    });
    const service = createTauriLibraryService(client);
    await connect(service);
    const listener = vi.fn();

    const unsubscribe = service.subscribeImport("import-1", listener);
    await vi.advanceTimersByTimeAsync(0);
    expect(listener).toHaveBeenLastCalledWith(importTaskResponse());

    await vi.advanceTimersByTimeAsync(250);
    expect(listener).toHaveBeenLastCalledWith(
      importTaskResponse({ state: "completed", completed: 2 }),
    );
    expect(pollCount).toBe(2);

    await vi.advanceTimersByTimeAsync(1_000);
    expect(pollCount).toBe(2);
    unsubscribe();
  });

  it("导入轮询失败时发送安全失败终态并停止，不泄露底层路径", async () => {
    vi.useFakeTimers();
    const { client, invoke } = commandClient();
    let pollCount = 0;
    invoke.mockImplementation(async (name: string) => {
      if (name === "open_workspace") return { displayName: "作品库" };
      if (name === "prepare_workspace_database") return {};
      if (name === "get_media_import_task") {
        pollCount += 1;
        throw new Error(privateMediaPath);
      }
      throw new Error(`测试未配置命令：${name}`);
    });
    const service = createTauriLibraryService(client);
    await connect(service);
    const listener = vi.fn();

    service.subscribeImport("import-1", listener);
    await vi.advanceTimersByTimeAsync(0);
    expect(listener).toHaveBeenCalledWith({
      id: "import-1",
      state: "failed",
      completed: 0,
      total: 0,
      currentFileName: null,
      message: "无法获取导入进度。",
    });
    expect(JSON.stringify(listener.mock.calls)).not.toContain(privateMediaPath);

    await vi.advanceTimersByTimeAsync(1_000);
    expect(pollCount).toBe(1);
  });

  it("启动和取消导入使用窄请求 DTO，返回值不泄露源路径", async () => {
    const { client, invoke } = commandClient();
    invoke.mockImplementation(async (name: string) => {
      if (name === "open_workspace") return { displayName: "作品库" };
      if (name === "prepare_workspace_database") return {};
      if (name === "start_media_import") {
        return importTaskResponse({ state: "queued", completed: 0 });
      }
      if (name === "cancel_media_import") return null;
      throw new Error(`测试未配置命令：${name}`);
    });
    const service = createTauriLibraryService(client);
    await connect(service);

    const task = await service.startImport({
      sourcePaths: [privateMediaPath],
      mode: "copy",
      projectId: "11",
    });
    expect(JSON.stringify(task)).not.toContain(privateMediaPath);
    expect(invoke).toHaveBeenCalledWith("start_media_import", {
      request: {
        rootPath: workspaceRoot,
        sourcePaths: [privateMediaPath],
        mode: "copy",
        projectId: 11,
      },
    });

    await service.cancelImport(task.id);
    expect(invoke).toHaveBeenLastCalledWith("cancel_media_import", {
      request: { taskId: "import-1" },
    });
  });

  it("回收站返回并消费下一页游标，避免超过首屏的记录不可达", async () => {
    const { client, invoke } = commandClient();
    invoke
      .mockResolvedValueOnce({ displayName: "作品库" })
      .mockResolvedValueOnce({})
      .mockResolvedValueOnce({
        items: [
          {
            id: 31,
            entityType: "asset",
            entityId: 7,
            displayName: "晨雾城市.png",
            mediaType: "image",
            deletedAt: 1_720_000_000,
          },
        ],
        nextCursor: { updatedAt: 1_720_000_000, id: 31 },
      })
      .mockResolvedValueOnce({ items: [], nextCursor: null });
    const service = createTauriLibraryService(client);
    await connect(service);

    const firstPage = await service.listTrash({ limit: 24 });
    expect(firstPage).toMatchObject({
      items: [
        {
          id: "31",
          entityType: "asset",
          entityId: "7",
          title: "晨雾城市.png",
          originalMediaPreserved: true,
        },
      ],
      nextCursor: "1720000000:31",
    });

    await service.listTrash({
      cursor: firstPage.nextCursor ?? undefined,
      limit: 24,
    });
    expect(invoke).toHaveBeenLastCalledWith("library_list_trash", {
      request: {
        rootPath: workspaceRoot,
        cursor: { updatedAt: 1_720_000_000, id: 31 },
        limit: 24,
      },
    });
  });

  it.each([
    importTaskResponse({ state: "unknown" }),
    importTaskResponse({ total: "2" }),
    importTaskResponse({ currentFileName: 123 }),
  ])("拒绝不符合导入任务契约的 unknown DTO %#", async (response) => {
    const { client } = commandClient();
    client.invoke = withWorkspaceCommands("start_media_import", response);
    const service = createTauriLibraryService(client);
    await connect(service);

    await expect(
      service.startImport({
        sourcePaths: [privateMediaPath],
        mode: "reference",
        projectId: null,
      }),
    ).rejects.toMatchObject({ code: serviceErrorCode.invalidResponse });
  });

  it("已知错误码只使用本地白名单消息，不信任远端路径", async () => {
    const { client, invoke } = commandClient();
    invoke.mockImplementation(async (name: string) => {
      if (name === "open_workspace") return { displayName: "作品库" };
      if (name === "prepare_workspace_database") return {};
      if (name === "library_list_projects") {
        throw {
          code: "LIBRARY_DATA_INVALID",
          message: privateMediaPath,
        };
      }
      throw new Error(`测试未配置命令：${name}`);
    });
    const service = createTauriLibraryService(client);
    await connect(service);

    const error = await service
      .listProjects({ limit: 24 })
      .catch((reason: unknown) => reason);
    expect(error).toBeInstanceOf(ServiceError);
    expect(error).toMatchObject({
      code: "LIBRARY_DATA_INVALID",
      message: "作品库数据不完整或已损坏。",
    });
    expect(String(error)).not.toContain(privateMediaPath);
  });

  it("按展示编号批量归入项目时发送显式确认", async () => {
    const { client, invoke } = commandClient();
    invoke.mockImplementation(async (name: string) => {
      if (name === "open_workspace") return { displayName: "作品库" };
      if (name === "prepare_workspace_database") return {};
      if (name === "library_assign_assets_to_project") return 3;
      throw new Error(`测试未配置命令：${name}`);
    });
    const service = createTauriLibraryService(client);
    await connect(service);

    await expect(service.assignAssetsToProject("8", [1, 2, 15])).resolves.toBe(
      3,
    );
    expect(invoke).toHaveBeenLastCalledWith(
      "library_assign_assets_to_project",
      {
        request: {
          rootPath: workspaceRoot,
          projectId: 8,
          displayNumbers: [1, 2, 15],
          confirmed: true,
        },
      },
    );
  });

  it("按内部作品 ID 批量移出项目时发送显式确认", async () => {
    const { client, invoke } = commandClient();
    invoke.mockImplementation(async (name: string) => {
      if (name === "open_workspace") return { displayName: "作品库" };
      if (name === "prepare_workspace_database") return {};
      if (name === "library_remove_assets_from_project") return 2;
      throw new Error(`测试未配置命令：${name}`);
    });
    const service = createTauriLibraryService(client);
    await connect(service);

    await expect(
      service.removeAssetsFromProject("8", ["11", "12"]),
    ).resolves.toBe(2);
    expect(invoke).toHaveBeenLastCalledWith(
      "library_remove_assets_from_project",
      {
        request: {
          rootPath: workspaceRoot,
          projectId: 8,
          assetIds: [11, 12],
          confirmed: true,
        },
      },
    );
  });

  it("未知远端错误码降级为安全错误且不泄露绝对路径", async () => {
    const { client, invoke } = commandClient();
    invoke.mockImplementation(async (name: string) => {
      if (name === "open_workspace") return { displayName: "作品库" };
      if (name === "prepare_workspace_database") return {};
      throw { code: "UNKNOWN_LIBRARY_ERROR", message: privateMediaPath };
    });
    const service = createTauriLibraryService(client);
    await connect(service);

    const error = await service
      .listProjects({ limit: 24 })
      .catch((reason: unknown) => reason);
    expect(error).toBeInstanceOf(ServiceError);
    expect(error).toMatchObject({ code: serviceErrorCode.commandFailed });
    expect(String(error)).not.toContain(privateMediaPath);
  });

  it("组合筛选被转换为受控的资产列表请求", async () => {
    const { client, invoke } = commandClient();
    invoke.mockImplementation(
      withWorkspaceCommands("library_list_assets", {
        items: [],
        nextCursor: null,
      }),
    );
    const service = createTauriLibraryService(client);
    await connect(service);
    await service.listAssets({
      limit: 24,
      keyword: "晨雾",
      mediaType: "image",
      model: "Flux",
      platform: "Local",
      rating: 4,
      isFavorite: true,
      isPublic: false,
    });
    expect(invoke).toHaveBeenLastCalledWith("library_list_assets", {
      request: {
        rootPath: workspaceRoot,
        projectId: null,
        mediaType: "image",
        keyword: "晨雾",
        searchField: "title",
        exactMatch: false,
        model: "Flux",
        platform: "Local",
        categoryIds: [],
        rating: 4,
        isFavorite: true,
        isPublic: false,
        createdAfter: null,
        createdBefore: null,
        minAspectRatio: null,
        maxAspectRatio: null,
        cursor: undefined,
        limit: 24,
      },
    });
  });

  it("缩略图 pending/unavailable 不创建 URL，也不泄露媒体路径", async () => {
    const { client, invoke } = commandClient();
    invoke.mockImplementation(async (name: string) => {
      if (name === "open_workspace") return { displayName: "作品库" };
      if (name === "prepare_workspace_database") return {};
      if (name === "library_read_asset_thumbnail") return { state: "pending" };
      throw new Error(`测试未配置命令：${name}`);
    });
    const service = createTauriLibraryService(client);
    await connect(service);
    await expect(service.getAssetThumbnail("7")).resolves.toBeNull();
    expect(JSON.stringify(invoke.mock.calls)).not.toContain(privateMediaPath);
  });

  it("缩略图 ready 响应按真实 camelCase 契约创建对象 URL", async () => {
    const { client, invoke } = commandClient();
    const createObjectUrl = vi.fn(() => "blob:thumbnail-ready");
    Object.defineProperty(URL, "createObjectURL", {
      configurable: true,
      value: createObjectUrl,
    });
    invoke.mockImplementation(async (name: string) => {
      if (name === "open_workspace") return { displayName: "作品库" };
      if (name === "prepare_workspace_database") return {};
      if (name === "library_read_asset_thumbnail") {
        return {
          state: "ready",
          bytes: [137, 80, 78, 71],
          mimeType: "image/png",
        };
      }
      throw new Error(`测试未配置命令：${name}`);
    });
    const service = createTauriLibraryService(client);
    await connect(service);

    await expect(service.getAssetThumbnail("7")).resolves.toBe(
      "blob:thumbnail-ready",
    );
    expect(createObjectUrl).toHaveBeenCalledOnce();
  });

  it("预览安全错误保留为可理解消息，不泄漏底层路径", async () => {
    const { client, invoke } = commandClient();
    invoke.mockImplementation(async (name: string) => {
      if (name === "open_workspace") return { displayName: "作品库" };
      if (name === "prepare_workspace_database") return {};
      if (name === "library_get_asset") return assetResponse();
      if (name === "library_read_asset_preview") {
        throw {
          code: "MEDIA_PREVIEW_TOO_LARGE",
          message: `${privateMediaPath} is too large`,
        };
      }
      throw new Error(`测试未配置命令：${name}`);
    });
    const service = createTauriLibraryService(client);
    await connect(service);

    const value = await service.getAssetDetail("7");
    expect(value.previewUrl).toBeNull();
    expect(value.previewError).toBe("媒体文件过大，无法在详情中直接预览。");
    expect(JSON.stringify(value)).not.toContain(privateMediaPath);
  });
});
