import type { CommandClient } from "./command-client";
import {
  ServiceError,
  serviceErrorCode,
  toServiceError,
} from "./service-error";
import type { WorkspaceAccessMode } from "./workspace-management-service";

export interface WorkspaceAccessReader {
  getAccessMode(rootPath: string): Promise<WorkspaceAccessMode>;
}

export function createWorkspaceAccessReader(
  commandClient: CommandClient,
): WorkspaceAccessReader {
  return {
    async getAccessMode(rootPath) {
      try {
        const value: unknown = await commandClient.invoke(
          "get_workspace_access_mode",
          { request: { rootPath } },
        );
        if (!value || typeof value !== "object" || Array.isArray(value)) {
          throw new ServiceError(
            serviceErrorCode.invalidResponse,
            "桌面服务返回了无法识别的访问模式。",
          );
        }
        const mode = (value as Record<string, unknown>).mode;
        if (mode !== "readWrite" && mode !== "readOnly") {
          throw new ServiceError(
            serviceErrorCode.invalidResponse,
            "桌面服务返回了无法识别的访问模式。",
          );
        }
        return mode;
      } catch (error) {
        throw toServiceError(error);
      }
    },
  };
}
