import { describe, expect, it, vi } from "vitest";
import type { CommandClient } from "../src/services/command-client";
import { ServiceError, serviceErrorCode } from "../src/services/service-error";
import { createWorkspaceService } from "../src/services/workspace-service";

function commandClientReturning(value: unknown): CommandClient {
  return {
    invoke: vi.fn().mockResolvedValue(value),
  };
}

describe("工作区服务", () => {
  it.each([
    ["createWorkspace", "create_workspace"],
    ["openWorkspace", "open_workspace"],
  ] as const)("%s 发送固定命令和请求 DTO", async (method, command) => {
    const client = commandClientReturning({
      displayName: "作品库",
      formatVersion: 1,
      ready: true,
    });
    const service = createWorkspaceService(client);

    await expect(service[method]("D:\\Gallery")).resolves.toEqual({
      displayName: "作品库",
      formatVersion: 1,
      ready: true,
    });
    expect(client.invoke).toHaveBeenCalledWith(command, {
      request: { rootPath: "D:\\Gallery" },
    });
  });

  it.each([
    null,
    {},
    { displayName: "", formatVersion: 1, ready: true },
    { displayName: "作品库", formatVersion: 0, ready: true },
    { displayName: "作品库", formatVersion: 2, ready: true },
    { displayName: "作品库", formatVersion: 1, ready: false },
  ])("拒绝不符合契约的响应 %#", async (response) => {
    const service = createWorkspaceService(commandClientReturning(response));

    await expect(service.openWorkspace("D:\\Gallery")).rejects.toMatchObject({
      code: serviceErrorCode.invalidResponse,
    });
  });

  it("仅使用已知工作区错误码对应的本地消息", async () => {
    const client: CommandClient = {
      invoke: vi.fn().mockRejectedValue({
        code: "WORKSPACE_NOT_FOUND",
        message: "D:\\private\\secret.db",
      }),
    };

    await expect(
      createWorkspaceService(client).openWorkspace("D:\\Gallery"),
    ).rejects.toMatchObject({
      code: "WORKSPACE_NOT_FOUND",
      message: "未找到工作区。",
    });
  });

  it("未知错误不会泄露绝对路径", async () => {
    const client: CommandClient = {
      invoke: vi.fn().mockRejectedValue({
        code: "UNKNOWN_WORKSPACE_ERROR",
        message: "D:\\private\\secret.db",
      }),
    };
    const error = await createWorkspaceService(client)
      .openWorkspace("D:\\Gallery")
      .catch((reason: unknown) => reason);

    expect(error).toBeInstanceOf(ServiceError);
    expect(error).toMatchObject({ code: serviceErrorCode.commandFailed });
    expect(String(error)).not.toContain("secret.db");
  });

  it("检查 managed 路径时只返回可迁移路径", async () => {
    const client = commandClientReturning({
      kind: "managed",
      availability: "available",
      portablePath: "media/images/example.png",
    });
    const service = createWorkspaceService(client);

    await expect(
      service.checkStoredPath({
        workspaceRoot: "D:\\Gallery",
        kind: "managed",
        storedPath: "media\\images\\example.png",
      }),
    ).resolves.toEqual({
      kind: "managed",
      availability: "available",
      portablePath: "media/images/example.png",
    });
    expect(client.invoke).toHaveBeenCalledWith("check_stored_path", {
      request: {
        workspaceRoot: "D:\\Gallery",
        kind: "managed",
        storedPath: "media\\images\\example.png",
      },
    });
  });

  it("拒绝 external 状态携带绝对路径响应", async () => {
    const service = createWorkspaceService(
      commandClientReturning({
        kind: "external",
        availability: "missing",
        portablePath: "D:\\private\\missing.mp4",
      }),
    );

    await expect(
      service.checkStoredPath({
        workspaceRoot: "D:\\Gallery",
        kind: "external",
        storedPath: "D:\\private\\missing.mp4",
      }),
    ).rejects.toMatchObject({ code: serviceErrorCode.invalidResponse });
  });

  it.each([
    "../outside.png",
    "/absolute.png",
    "C:/absolute.png",
    "media\\image.png",
    "media//image.png",
    "media/./image.png",
  ])("拒绝非规范 managed portablePath：%s", async (portablePath) => {
    const service = createWorkspaceService(
      commandClientReturning({
        kind: "managed",
        availability: "missing",
        portablePath,
      }),
    );

    await expect(
      service.checkStoredPath({
        workspaceRoot: "D:\\Gallery",
        kind: "managed",
        storedPath: portablePath,
      }),
    ).rejects.toMatchObject({ code: serviceErrorCode.invalidResponse });
  });
});
