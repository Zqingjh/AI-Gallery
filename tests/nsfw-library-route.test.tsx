import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { StrictMode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { text } from "../src/app/texts";
import NsfwLibraryRoute from "../src/features/nsfw/NsfwLibraryRoute";
import type {
  CommandArguments,
  CommandClient,
} from "../src/services/command-client";

afterEach(() => {
  vi.useRealTimers();
});

describe("NSFW 独立作品库", () => {
  it("StrictMode 预演不会重复连接已记住的私密工作区", async () => {
    const privateRoot = "D:\\Private-Gallery";
    const invoke = vi.fn(async (command: string, _arguments_?: unknown) => {
      if (command === "validate_isolated_workspace") return {};
      if (command === "open_workspace") return { displayName: "私密作品库" };
      if (command === "prepare_workspace_database") return {};
      if (command === "get_workspace_access_mode") {
        return { mode: "readWrite" };
      }
      if (command === "library_list_assets_numbered") {
        return {
          items: [],
          page: 1,
          pageSize: 25,
          totalCount: 0,
          totalPages: 0,
        };
      }
      if (command === "library_list_projects") {
        return { items: [], nextCursor: null };
      }
      if (command === "library_list_metadata_presets") {
        return { models: [], platforms: [] };
      }
      throw new Error(`测试未配置命令：${command}`);
    });
    const client: CommandClient = {
      invoke: async <TResult,>(
        command: string,
        arguments_?: CommandArguments,
      ) => (await invoke(command, arguments_)) as TResult,
    };

    render(
      <StrictMode>
        <NsfwLibraryRoute
          commandClient={client}
          normalWorkspaceRoot="D:\\Normal-Gallery"
          initialWorkspaceRoot={privateRoot}
          onWorkspaceRootChanged={vi.fn()}
          onExit={vi.fn()}
        />
      </StrictMode>,
    );

    expect(
      await screen.findByRole("heading", { name: text.nsfw.tabs.assets }),
    ).toBeInTheDocument();
    expect(
      invoke.mock.calls.filter(([command]) => command === "open_workspace"),
    ).toHaveLength(1);
    expect(
      invoke.mock.calls.filter(
        ([command]) => command === "validate_isolated_workspace",
      ),
    ).toHaveLength(1);
  });

  it("首次激活只连接独立 root，退出后恢复正常主题", async () => {
    const normalRoot = "D:\\Normal-Gallery";
    const privateRoot = "D:\\Private-Gallery";
    const invoke = vi.fn(async (command: string, _arguments_?: unknown) => {
      if (command === "validate_isolated_workspace") return {};
      if (command === "open_workspace") return { displayName: "私密作品库" };
      if (command === "prepare_workspace_database") return {};
      if (command === "get_workspace_access_mode") {
        return { mode: "readWrite" };
      }
      if (command === "library_list_assets_numbered") {
        return {
          items: [],
          page: 1,
          pageSize: 25,
          totalCount: 0,
          totalPages: 0,
        };
      }
      if (command === "library_list_projects") {
        return { items: [], nextCursor: null };
      }
      if (command === "library_list_metadata_presets") {
        return { models: [], platforms: [] };
      }
      throw new Error(`测试未配置命令：${command}`);
    });
    const client: CommandClient = {
      invoke: async <TResult,>(
        command: string,
        arguments_?: CommandArguments,
      ) => (await invoke(command, arguments_)) as TResult,
    };
    const onExit = vi.fn();
    document.documentElement.dataset.theme = "light";
    window.localStorage.setItem("ai-gallery-theme", "light");
    const { unmount } = render(
      <NsfwLibraryRoute
        commandClient={client}
        normalWorkspaceRoot={normalRoot}
        initialWorkspaceRoot={null}
        onWorkspaceRootChanged={vi.fn()}
        onExit={onExit}
      />,
    );

    fireEvent.change(await screen.findByLabelText(text.library.workspacePath), {
      target: { value: privateRoot },
    });
    fireEvent.click(
      screen.getByRole("button", { name: text.library.connectWorkspace }),
    );

    expect(
      await screen.findByRole("heading", { name: text.nsfw.tabs.assets }),
    ).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("validate_isolated_workspace", {
      request: {
        primaryRootPath: normalRoot,
        isolatedRootPath: privateRoot,
      },
    });
    const privateRequests = invoke.mock.calls.filter(
      ([command]) =>
        command === "prepare_workspace_database" ||
        String(command).startsWith("library_"),
    );
    expect(privateRequests.length).toBeGreaterThan(0);
    for (const [, arguments_] of privateRequests) {
      expect(arguments_).toMatchObject({ request: { rootPath: privateRoot } });
    }

    fireEvent.click(screen.getByRole("button", { name: text.nsfw.exit }));
    expect(onExit).toHaveBeenCalledTimes(1);
    expect(document.documentElement).toHaveAttribute("data-theme", "light");
    expect(window.localStorage.getItem("ai-gallery-theme")).toBe("light");

    document.documentElement.dataset.theme = "dark";
    window.localStorage.setItem("ai-gallery-theme", "dark");
    unmount();
    await waitFor(() =>
      expect(document.documentElement).toHaveAttribute("data-theme", "light"),
    );
    expect(window.localStorage.getItem("ai-gallery-theme")).toBe("light");
  });
});
