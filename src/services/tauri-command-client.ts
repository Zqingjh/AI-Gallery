import { invoke } from "@tauri-apps/api/core";
import type {
  CommandArguments,
  CommandClient,
  CommandInvoker,
} from "./command-client";

/** Tauri API 仅停留在此 adapter，UI 和业务 service 都依赖可替换契约。 */
export function createTauriCommandClient(
  invokeCommand: CommandInvoker = invoke,
): CommandClient {
  return {
    invoke<TResult>(
      command: string,
      arguments_?: CommandArguments,
    ): Promise<TResult> {
      if (arguments_ === undefined) {
        return invokeCommand<TResult>(command);
      }

      return invokeCommand<TResult>(command, arguments_);
    },
  };
}
