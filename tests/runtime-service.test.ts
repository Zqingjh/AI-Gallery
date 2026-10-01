import { describe, expect, it, vi } from "vitest";
import type { CommandClient } from "../src/services/command-client";
import { createRuntimeService } from "../src/services/runtime-service";
import { ServiceError, serviceErrorCode } from "../src/services/service-error";
import { createTauriCommandClient } from "../src/services/tauri-command-client";

function commandClientReturning(value: unknown): CommandClient {
  return {
    invoke: vi.fn().mockResolvedValue(value),
  };
}

describe("运行时服务", () => {
  it("通过注入的 command client 获取类型化运行时信息", async () => {
    const commandClient = commandClientReturning({
      appVersion: "0.1.0",
      platform: "windows",
      architecture: "x86_64",
    });

    await expect(
      createRuntimeService(commandClient).getRuntimeInfo(),
    ).resolves.toEqual({
      appVersion: "0.1.0",
      platform: "windows",
      architecture: "x86_64",
    });
    expect(commandClient.invoke).toHaveBeenCalledWith("get_runtime_info");
  });

  it("保留 Rust 返回的稳定错误 DTO", async () => {
    const commandClient: CommandClient = {
      invoke: vi.fn().mockRejectedValue({
        code: "RUNTIME_INFO_UNAVAILABLE",
        message: "无法获取运行环境信息。",
      }),
    };

    await expect(
      createRuntimeService(commandClient).getRuntimeInfo(),
    ).rejects.toMatchObject({
      name: "ServiceError",
      code: "RUNTIME_INFO_UNAVAILABLE",
      message: "无法获取运行环境信息。",
    });
  });

  it("将未知拒绝值归一化且不暴露原始信息", async () => {
    const commandClient: CommandClient = {
      invoke: vi
        .fn()
        .mockRejectedValue(new Error("D:\\private\\workspace\\secret.db")),
    };

    const error = await createRuntimeService(commandClient)
      .getRuntimeInfo()
      .catch((reason: unknown) => reason);

    expect(error).toBeInstanceOf(ServiceError);
    expect(error).toMatchObject({ code: serviceErrorCode.commandFailed });
    expect(String(error)).not.toContain("secret.db");
  });

  it("不信任已知错误码携带的远端消息", async () => {
    const commandClient: CommandClient = {
      invoke: vi.fn().mockRejectedValue({
        code: "RUNTIME_INFO_UNAVAILABLE",
        message: "D:\\private\\workspace\\secret.db",
      }),
    };

    await expect(
      createRuntimeService(commandClient).getRuntimeInfo(),
    ).rejects.toMatchObject({
      code: "RUNTIME_INFO_UNAVAILABLE",
      message: "无法获取运行环境信息。",
    });
  });

  it("拒绝未知远端错误码并使用安全回退", async () => {
    const commandClient: CommandClient = {
      invoke: vi.fn().mockRejectedValue({
        code: "UNRECOGNIZED_ERROR",
        message: "D:\\private\\workspace\\secret.db",
      }),
    };

    await expect(
      createRuntimeService(commandClient).getRuntimeInfo(),
    ).rejects.toMatchObject({ code: serviceErrorCode.commandFailed });
  });

  it("拒绝不符合契约的 command 响应", async () => {
    const commandClient = commandClientReturning({ appVersion: "0.1.0" });

    await expect(
      createRuntimeService(commandClient).getRuntimeInfo(),
    ).rejects.toMatchObject({ code: serviceErrorCode.invalidResponse });
  });
});

describe("Tauri command adapter", () => {
  it("只转发命令名并保持泛型结果", async () => {
    const invokeCommand = vi.fn().mockResolvedValue({ appVersion: "0.1.0" });
    const client = createTauriCommandClient(invokeCommand);

    await expect(client.invoke("get_runtime_info")).resolves.toEqual({
      appVersion: "0.1.0",
    });
    expect(invokeCommand).toHaveBeenCalledWith("get_runtime_info");
  });

  it("原样转发命令参数，不改写工作区请求 DTO", async () => {
    const invokeCommand = vi.fn().mockResolvedValue({
      displayName: "作品库",
      formatVersion: 1,
      ready: true,
    });
    const client = createTauriCommandClient(invokeCommand);
    const arguments_ = { request: { rootPath: "D:\\Gallery" } };

    await client.invoke("open_workspace", arguments_);

    expect(invokeCommand).toHaveBeenCalledTimes(1);
    expect(invokeCommand).toHaveBeenCalledWith("open_workspace", arguments_);
  });
});
