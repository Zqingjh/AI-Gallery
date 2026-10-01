import type { CommandClient } from "./command-client";
import {
  ServiceError,
  serviceErrorCode,
  toServiceError,
} from "./service-error";

export interface RuntimeInfo {
  readonly appVersion: string;
  readonly platform: string;
  readonly architecture: string;
}

export interface RuntimeService {
  getRuntimeInfo(): Promise<RuntimeInfo>;
}

const runtimeInfoCommand = "get_runtime_info";

function parseRuntimeInfo(value: unknown): RuntimeInfo {
  if (
    typeof value !== "object" ||
    value === null ||
    !("appVersion" in value) ||
    typeof value.appVersion !== "string" ||
    !("platform" in value) ||
    typeof value.platform !== "string" ||
    !("architecture" in value) ||
    typeof value.architecture !== "string"
  ) {
    throw new ServiceError(
      serviceErrorCode.invalidResponse,
      "桌面服务返回了无法识别的数据。",
    );
  }

  return {
    appVersion: value.appVersion,
    platform: value.platform,
    architecture: value.architecture,
  };
}

/** 注入 command client 后可在浏览器测试，无需让 UI 直接依赖 Tauri。 */
export function createRuntimeService(
  commandClient: CommandClient,
): RuntimeService {
  return {
    async getRuntimeInfo(): Promise<RuntimeInfo> {
      try {
        const response =
          await commandClient.invoke<unknown>(runtimeInfoCommand);
        return parseRuntimeInfo(response);
      } catch (error: unknown) {
        throw toServiceError(error);
      }
    },
  };
}
