import { describe, expect, it, vi } from "vitest";
import type { CommandClient } from "../src/services/command-client";
import { createDatabaseService } from "../src/services/database-service";
import { serviceErrorCode } from "../src/services/service-error";

function commandClientReturning(value: unknown): CommandClient {
  return { invoke: vi.fn().mockResolvedValue(value) };
}

describe("数据库服务", () => {
  it("发送固定命令并解析最小状态 DTO", async () => {
    const client = commandClientReturning({
      schemaVersion: 3,
      migrated: true,
      backupCreated: false,
    });

    await expect(
      createDatabaseService(client).prepareWorkspaceDatabase("D:\\Gallery"),
    ).resolves.toEqual({
      schemaVersion: 3,
      migrated: true,
      backupCreated: false,
    });
    expect(client.invoke).toHaveBeenCalledWith("prepare_workspace_database", {
      request: { rootPath: "D:\\Gallery" },
    });
  });

  it.each([
    null,
    {},
    { schemaVersion: 0, migrated: true, backupCreated: false },
    { schemaVersion: 2, migrated: true, backupCreated: false },
    { schemaVersion: 3, migrated: "yes", backupCreated: false },
    { schemaVersion: 3, migrated: true, backupCreated: null },
  ])("拒绝不符合契约的状态 %#", async (response) => {
    const service = createDatabaseService(commandClientReturning(response));

    await expect(
      service.prepareWorkspaceDatabase("D:\\Gallery"),
    ).rejects.toMatchObject({ code: serviceErrorCode.invalidResponse });
  });

  it.each([
    ["DATABASE_PATH_UNSAFE", "数据库位置不安全，无法打开工作区。"],
    ["DATABASE_OPEN_FAILED", "无法打开工作区数据库。"],
    ["DATABASE_BACKUP_FAILED", "数据库迁移前备份失败，未执行迁移。"],
    ["DATABASE_MIGRATION_FAILED", "数据库升级失败，已撤销本次结构变更。"],
    ["DATABASE_VERSION_UNSUPPORTED", "当前版本无法打开此数据库。"],
    ["DATABASE_SCHEMA_INVALID", "数据库结构不完整或已损坏。"],
  ])("错误码 %s 只使用本地安全消息", async (code, message) => {
    const client: CommandClient = {
      invoke: vi.fn().mockRejectedValue({
        code,
        message: "D:\\private\\secret.sqlite3",
      }),
    };

    await expect(
      createDatabaseService(client).prepareWorkspaceDatabase("D:\\Gallery"),
    ).rejects.toMatchObject({ code, message });
  });
});
