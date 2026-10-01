import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { App } from "../src/app/App";
import { text } from "../src/app/texts";
import globalStyles from "../src/styles/global.css?raw";

describe("应用壳", () => {
  it("只展示轻量启动内容和工作区入口", () => {
    render(<App />);

    expect(
      screen.getByRole("heading", { name: /让每一次生成/ }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: text.shell.createWorkspace }),
    ).toBeDisabled();
    expect(
      screen.getByText(text.shell.workspaceActionsUnavailable),
    ).toBeInTheDocument();
    expect(screen.getByText(text.shell.emptyDescription)).toBeInTheDocument();
  });

  it("存储读取异常时安全回退到深色主题", async () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("存储不可用");
    });

    render(<App />);

    await waitFor(() =>
      expect(document.documentElement).toHaveAttribute("data-theme", "dark"),
    );
  });

  it("忽略非法主题值", async () => {
    window.localStorage.setItem("ai-gallery-theme", "unknown");

    render(<App />);

    await waitFor(() =>
      expect(document.documentElement).toHaveAttribute("data-theme", "dark"),
    );
  });

  it("存储写入异常不影响主题切换", async () => {
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("存储不可写");
    });
    render(<App />);

    fireEvent.click(screen.getByRole("button", { name: text.theme.light }));

    await waitFor(() =>
      expect(document.documentElement).toHaveAttribute("data-theme", "light"),
    );
  });

  it("窄屏样式仍保留唯一的设置入口", () => {
    render(<App />);

    expect(screen.getByRole("button", { name: text.nav.settings })).toHaveClass(
      "settings-button",
    );
    expect(globalStyles).toMatch(
      /\.settings-button\s*{[^}]*display:\s*inline-flex/s,
    );
  });

  it("切换主题并保存偏好", async () => {
    render(<App />);

    fireEvent.click(screen.getByRole("button", { name: text.theme.light }));

    await waitFor(() =>
      expect(document.documentElement).toHaveAttribute("data-theme", "light"),
    );
    expect(window.localStorage.getItem("ai-gallery-theme")).toBe("light");
  });

  it("按需打开低频设置页并能返回", async () => {
    render(<App />);

    fireEvent.click(screen.getByRole("button", { name: text.nav.settings }));

    expect(
      await screen.findByRole("heading", { name: text.settings.title }),
    ).toBeInTheDocument();
    expect(window.location.pathname).toBe("/settings");

    fireEvent.click(
      screen.getByRole("button", { name: new RegExp(text.settings.back) }),
    );
    expect(
      screen.getByRole("heading", { name: /让每一次生成/ }),
    ).toBeInTheDocument();
  });

  it("按需打开 AI 审核路由且未连接工作区时不发起服务调用", async () => {
    render(<App />);

    fireEvent.click(screen.getByRole("button", { name: text.nav.review }));

    expect(
      await screen.findByRole("heading", { name: text.ai.reviewTitle }),
    ).toBeInTheDocument();
    expect(window.location.pathname).toBe("/ai-review");
    expect(await screen.findByRole("alert")).toHaveTextContent(
      text.ai.noWorkspace,
    );
  });

  it("导航中不再提供公开导出入口", () => {
    render(<App />);
    expect(
      screen.queryByRole("button", { name: "公开导出" }),
    ).not.toBeInTheDocument();
  });
});
