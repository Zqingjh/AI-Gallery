import type { CommandClient } from "./command-client";
import {
  ServiceError,
  serviceErrorCode,
  toServiceError,
} from "./service-error";

export interface WorkspaceInfo {
  readonly displayName: string;
  readonly formatVersion: number;
  readonly ready: true;
}

export type StoredPathKind = "managed" | "external";
export type PathAvailability = "available" | "missing";

export interface StoredPathStatus {
  readonly kind: StoredPathKind;
  readonly availability: PathAvailability;
  readonly portablePath: string | null;
}

export interface CheckStoredPathRequest {
  readonly workspaceRoot: string;
  readonly kind: StoredPathKind;
  readonly storedPath: string;
}

export interface WorkspaceService {
  createWorkspace(rootPath: string): Promise<WorkspaceInfo>;
  openWorkspace(rootPath: string): Promise<WorkspaceInfo>;
  checkStoredPath(request: CheckStoredPathRequest): Promise<StoredPathStatus>;
}

const createWorkspaceCommand = "create_workspace";
const openWorkspaceCommand = "open_workspace";
const checkStoredPathCommand = "check_stored_path";
const supportedWorkspaceFormatVersion = 1;

function parseWorkspaceInfo(value: unknown): WorkspaceInfo {
  if (
    typeof value !== "object" ||
    value === null ||
    !("displayName" in value) ||
    typeof value.displayName !== "string" ||
    value.displayName.length === 0 ||
    !("formatVersion" in value) ||
    typeof value.formatVersion !== "number" ||
    !Number.isInteger(value.formatVersion) ||
    value.formatVersion !== supportedWorkspaceFormatVersion ||
    !("ready" in value) ||
    value.ready !== true
  ) {
    throw new ServiceError(
      serviceErrorCode.invalidResponse,
      "桌面服务返回了无法识别的工作区信息。",
    );
  }

  return {
    displayName: value.displayName,
    formatVersion: value.formatVersion,
    ready: true,
  };
}

function requestArguments(rootPath: string): Record<string, unknown> {
  return { request: { rootPath } };
}

function isPortableManagedPath(value: string): boolean {
  if (
    value.length === 0 ||
    value.startsWith("/") ||
    value.includes("\\") ||
    value.includes("\0")
  ) {
    return false;
  }

  return value
    .split("/")
    .every(
      (segment) =>
        segment.length > 0 &&
        segment !== "." &&
        segment !== ".." &&
        !/[:*?"<>|]/u.test(segment),
    );
}

function parseStoredPathStatus(value: unknown): StoredPathStatus {
  if (
    typeof value !== "object" ||
    value === null ||
    !("kind" in value) ||
    (value.kind !== "managed" && value.kind !== "external") ||
    !("availability" in value) ||
    (value.availability !== "available" && value.availability !== "missing") ||
    !("portablePath" in value) ||
    (value.portablePath !== null && typeof value.portablePath !== "string")
  ) {
    throw new ServiceError(
      serviceErrorCode.invalidResponse,
      "桌面服务返回了无法识别的路径状态。",
    );
  }

  if (
    (value.kind === "managed" &&
      (value.portablePath === null ||
        !isPortableManagedPath(value.portablePath))) ||
    (value.kind === "external" && value.portablePath !== null)
  ) {
    throw new ServiceError(
      serviceErrorCode.invalidResponse,
      "桌面服务返回了无法识别的路径状态。",
    );
  }

  return {
    kind: value.kind,
    availability: value.availability,
    portablePath: value.portablePath,
  };
}

/** 工作区路径只作为命令输入，不进入 UI 全局状态或错误消息。 */
export function createWorkspaceService(
  commandClient: CommandClient,
): WorkspaceService {
  async function invokeWorkspaceCommand(
    command: string,
    rootPath: string,
  ): Promise<WorkspaceInfo> {
    try {
      const response = await commandClient.invoke<unknown>(
        command,
        requestArguments(rootPath),
      );
      return parseWorkspaceInfo(response);
    } catch (error: unknown) {
      throw toServiceError(error);
    }
  }

  return {
    createWorkspace(rootPath: string): Promise<WorkspaceInfo> {
      return invokeWorkspaceCommand(createWorkspaceCommand, rootPath);
    },
    openWorkspace(rootPath: string): Promise<WorkspaceInfo> {
      return invokeWorkspaceCommand(openWorkspaceCommand, rootPath);
    },
    async checkStoredPath(
      request: CheckStoredPathRequest,
    ): Promise<StoredPathStatus> {
      try {
        const response = await commandClient.invoke<unknown>(
          checkStoredPathCommand,
          { request },
        );
        return parseStoredPathStatus(response);
      } catch (error: unknown) {
        throw toServiceError(error);
      }
    },
  };
}
