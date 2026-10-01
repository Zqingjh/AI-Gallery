import { describe, expect, it, vi } from "vitest";
import type { CommandClient } from "../src/services/command-client";
import {
  createWorkspaceManagementService,
  type WorkspaceManagementService,
} from "../src/services/workspace-management-service";
import { serviceErrorCode } from "../src/services/service-error";

function serviceWith(response: unknown): {
  readonly client: CommandClient;
  readonly service: WorkspaceManagementService;
} {
  const client: CommandClient = { invoke: vi.fn().mockResolvedValue(response) };
  return { client, service: createWorkspaceManagementService(client) };
}

describe("工作区管理服务", () => {
  it("以固定契约创建备份并严格解析响应", async () => {
    const { client, service } = serviceWith({
      kind: "full",
      backupName: "backup-20260714",
      managedAssetCount: 12,
      externalAssetCount: 3,
    });

    await expect(
      service.createBackup({
        rootPath: "D:\\Gallery",
        destinationParentPath: "D:\\Backups",
        kind: "full",
      }),
    ).resolves.toEqual({
      kind: "full",
      backupName: "backup-20260714",
      managedAssetCount: 12,
      externalAssetCount: 3,
    });
    expect(client.invoke).toHaveBeenCalledWith("create_workspace_backup", {
      request: {
        rootPath: "D:\\Gallery",
        destinationParentPath: "D:\\Backups",
        kind: "full",
      },
    });
  });

  it("恢复与访问模式使用固定命令和确认字段", async () => {
    const client: CommandClient = {
      invoke: vi
        .fn()
        .mockResolvedValueOnce({
          kind: "light",
          restored: true,
          targetDatabaseBackedUp: true,
        })
        .mockResolvedValueOnce({ mode: "readOnly" })
        .mockResolvedValueOnce({ mode: "readWrite" }),
    };
    const service = createWorkspaceManagementService(client);

    await expect(
      service.restoreBackup({
        backupRootPath: "D:\\Backups\\light",
        targetRootPath: "D:\\Gallery",
        confirmed: true,
      }),
    ).resolves.toMatchObject({ kind: "light", restored: true });
    await expect(service.getAccessMode("D:\\Gallery")).resolves.toBe(
      "readOnly",
    );
    await expect(
      service.setAccessMode({
        rootPath: "D:\\Gallery",
        mode: "readWrite",
        confirmed: true,
      }),
    ).resolves.toBe("readWrite");
    expect(client.invoke).toHaveBeenLastCalledWith(
      "set_workspace_access_mode",
      {
        request: {
          rootPath: "D:\\Gallery",
          mode: "readWrite",
          confirmed: true,
        },
      },
    );
  });

  it.each([
    {},
    { mode: "write" },
    {
      kind: "full",
      backupName: "",
      managedAssetCount: 1,
      externalAssetCount: 0,
    },
    {
      kind: "full",
      backupName: "safe",
      managedAssetCount: -1,
      externalAssetCount: 0,
    },
  ])("拒绝不符合运行时契约的响应 %#", async (response) => {
    const { service } = serviceWith(response);
    await expect(service.getAccessMode("D:\\Gallery")).rejects.toMatchObject({
      code: serviceErrorCode.invalidResponse,
    });
  });
});
