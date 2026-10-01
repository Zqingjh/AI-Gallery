import type { CommandClient } from "./command-client";
import {
  ServiceError,
  serviceErrorCode,
  toServiceError,
} from "./service-error";

export interface DatabaseStatus {
  readonly schemaVersion: number;
  readonly migrated: boolean;
  readonly backupCreated: boolean;
}

export interface DatabaseService {
  prepareWorkspaceDatabase(rootPath: string): Promise<DatabaseStatus>;
}

const prepareDatabaseCommand = "prepare_workspace_database";
const supportedSchemaVersion = 3;

function parseDatabaseStatus(value: unknown): DatabaseStatus {
  if (
    typeof value !== "object" ||
    value === null ||
    !("schemaVersion" in value) ||
    value.schemaVersion !== supportedSchemaVersion ||
    !("migrated" in value) ||
    typeof value.migrated !== "boolean" ||
    !("backupCreated" in value) ||
    typeof value.backupCreated !== "boolean"
  ) {
    throw new ServiceError(
      serviceErrorCode.invalidResponse,
      "桌面服务返回了无法识别的数据库状态。",
    );
  }

  return {
    schemaVersion: value.schemaVersion,
    migrated: value.migrated,
    backupCreated: value.backupCreated,
  };
}

/** 数据库准备只能显式触发；此 service 不被应用首屏静态导入。 */
export function createDatabaseService(
  commandClient: CommandClient,
): DatabaseService {
  return {
    async prepareWorkspaceDatabase(rootPath: string): Promise<DatabaseStatus> {
      try {
        const response = await commandClient.invoke<unknown>(
          prepareDatabaseCommand,
          { request: { rootPath } },
        );
        return parseDatabaseStatus(response);
      } catch (error: unknown) {
        throw toServiceError(error);
      }
    },
  };
}
