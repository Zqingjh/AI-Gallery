import type { CommandClient } from "./command-client";
import {
  ServiceError,
  serviceErrorCode,
  toServiceError,
} from "./service-error";

export type WorkspaceBackupKind = "full" | "light";
export type WorkspaceAccessMode = "readWrite" | "readOnly";

export interface CreateWorkspaceBackupInput {
  readonly rootPath: string;
  readonly destinationParentPath: string;
  readonly kind: WorkspaceBackupKind;
}

export interface WorkspaceBackupResult {
  readonly kind: WorkspaceBackupKind;
  readonly backupName: string;
  readonly managedAssetCount: number;
  readonly externalAssetCount: number;
}

export interface RestoreWorkspaceBackupInput {
  readonly backupRootPath: string;
  readonly targetRootPath: string;
  readonly confirmed: true;
}

export interface RestoreWorkspaceBackupResult {
  readonly kind: WorkspaceBackupKind;
  readonly restored: true;
  readonly targetDatabaseBackedUp: boolean;
}

export interface SetWorkspaceAccessModeInput {
  readonly rootPath: string;
  readonly mode: WorkspaceAccessMode;
  readonly confirmed: boolean;
}

export interface WorkspaceManagementService {
  selectDirectory(): Promise<string | null>;
  createBackup(
    input: CreateWorkspaceBackupInput,
  ): Promise<WorkspaceBackupResult>;
  restoreBackup(
    input: RestoreWorkspaceBackupInput,
  ): Promise<RestoreWorkspaceBackupResult>;
  getAccessMode(rootPath: string): Promise<WorkspaceAccessMode>;
  setAccessMode(
    input: SetWorkspaceAccessModeInput,
  ): Promise<WorkspaceAccessMode>;
}

type UnknownRecord = Record<string, unknown>;

const createBackupCommand = "create_workspace_backup";
const restoreBackupCommand = "restore_workspace_backup";
const getAccessModeCommand = "get_workspace_access_mode";
const setAccessModeCommand = "set_workspace_access_mode";

function invalidResponse(): ServiceError {
  return new ServiceError(
    serviceErrorCode.invalidResponse,
    "桌面服务返回了无法识别的工作区管理数据。",
  );
}

function record(value: unknown): UnknownRecord {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw invalidResponse();
  }
  return value as UnknownRecord;
}

function backupKind(value: unknown): WorkspaceBackupKind {
  if (value === "full" || value === "light") return value;
  throw invalidResponse();
}

function accessMode(value: unknown): WorkspaceAccessMode {
  if (value === "readWrite" || value === "readOnly") return value;
  throw invalidResponse();
}

function nonNegativeInteger(value: unknown): number {
  if (typeof value !== "number" || !Number.isInteger(value) || value < 0) {
    throw invalidResponse();
  }
  return value;
}

function parseBackupResult(value: unknown): WorkspaceBackupResult {
  const response = record(value);
  if (typeof response.backupName !== "string" || !response.backupName) {
    throw invalidResponse();
  }
  return {
    kind: backupKind(response.kind),
    backupName: response.backupName,
    managedAssetCount: nonNegativeInteger(response.managedAssetCount),
    externalAssetCount: nonNegativeInteger(response.externalAssetCount),
  };
}

function parseRestoreResult(value: unknown): RestoreWorkspaceBackupResult {
  const response = record(value);
  if (
    response.restored !== true ||
    typeof response.targetDatabaseBackedUp !== "boolean"
  ) {
    throw invalidResponse();
  }
  return {
    kind: backupKind(response.kind),
    restored: true,
    targetDatabaseBackedUp: response.targetDatabaseBackedUp,
  };
}

/** 备份和访问模式均通过显式 IPC 调用，不进入首屏加载路径。 */
export function createWorkspaceManagementService(
  commandClient: CommandClient,
): WorkspaceManagementService {
  async function invoke<TResult>(
    command: string,
    request: UnknownRecord,
  ): Promise<TResult> {
    try {
      return await commandClient.invoke<TResult>(command, { request });
    } catch (error: unknown) {
      throw toServiceError(error);
    }
  }

  return {
    async selectDirectory() {
      try {
        const { open } = await import("@tauri-apps/plugin-dialog");
        const selected = await open({ directory: true, multiple: false });
        return typeof selected === "string" ? selected : null;
      } catch (error: unknown) {
        throw toServiceError(error);
      }
    },
    async createBackup(input) {
      return parseBackupResult(
        await invoke<unknown>(createBackupCommand, {
          rootPath: input.rootPath,
          destinationParentPath: input.destinationParentPath,
          kind: input.kind,
        }),
      );
    },
    async restoreBackup(input) {
      return parseRestoreResult(
        await invoke<unknown>(restoreBackupCommand, {
          backupRootPath: input.backupRootPath,
          targetRootPath: input.targetRootPath,
          confirmed: input.confirmed,
        }),
      );
    },
    async getAccessMode(rootPath) {
      const response = record(
        await invoke<unknown>(getAccessModeCommand, { rootPath }),
      );
      return accessMode(response.mode);
    },
    async setAccessMode(input) {
      const response = record(
        await invoke<unknown>(setAccessModeCommand, {
          rootPath: input.rootPath,
          mode: input.mode,
          confirmed: input.confirmed,
        }),
      );
      return accessMode(response.mode);
    },
  };
}
