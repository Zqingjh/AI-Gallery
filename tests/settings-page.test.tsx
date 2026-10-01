import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import SettingsPage from "../src/features/settings/SettingsPage";
import { ServiceError } from "../src/services/service-error";
import type { WorkspaceManagementService } from "../src/services/workspace-management-service";

function managementService(
  overrides: Partial<WorkspaceManagementService> = {},
): WorkspaceManagementService {
  return {
    selectDirectory: vi.fn().mockResolvedValue(null),
    createBackup: vi.fn().mockResolvedValue({
      kind: "full",
      backupName: "backup-safe",
      managedAssetCount: 2,
      externalAssetCount: 0,
    }),
    restoreBackup: vi.fn().mockResolvedValue({
      kind: "full",
      restored: true,
      targetDatabaseBackedUp: false,
    }),
    getAccessMode: vi.fn().mockResolvedValue("readWrite"),
    setAccessMode: vi.fn().mockResolvedValue("readOnly"),
    ...overrides,
  };
}

function renderSettings(service = managementService()) {
  render(
    <SettingsPage
      workspaceManagementService={service}
      workspaceRoot="D:\\Gallery"
      accessMode="readWrite"
      onAccessModeChanged={vi.fn()}
      onBack={vi.fn()}
    />,
  );
  return service;
}

describe("设置页", () => {
  it("路径为空时给出具体提示，不伪装成工作区不可用", async () => {
    const service = renderSettings();
    fireEvent.click(screen.getByRole("button", { name: "创建备份" }));

    expect(
      await screen.findByText("请先选择备份保存目录。"),
    ).toBeInTheDocument();
    expect(service.createBackup).not.toHaveBeenCalled();
  });

  it("可通过目录选择器填写备份保存位置", async () => {
    const selectDirectory = vi.fn().mockResolvedValue("D:\\Backups");
    renderSettings(managementService({ selectDirectory }));

    fireEvent.click(screen.getByRole("button", { name: "选择备份目录" }));

    expect(selectDirectory).toHaveBeenCalledTimes(1);
    expect(await screen.findByLabelText("备份保存目录")).toHaveValue(
      "D:\\Backups",
    );
  });

  it("恢复位置选择父目录，并自动生成尚不存在的工作区子目录", async () => {
    const selectDirectory = vi.fn().mockResolvedValue("D:\\Restores");
    renderSettings(managementService({ selectDirectory }));

    fireEvent.click(screen.getByRole("button", { name: "选择恢复父目录" }));

    const restoreTarget =
      await screen.findByLabelText<HTMLInputElement>("恢复目标工作区");
    expect(restoreTarget.value).toMatch(
      /^D:\\Restores\\AI-Gallery-Restored-\d{8}-\d{6}$/,
    );
  });

  it("工作区操作显示本地白名单中的具体错误", async () => {
    const createBackup = vi
      .fn()
      .mockRejectedValue(
        new ServiceError("BACKUP_CONFLICT", "目标位置已存在或与现有内容冲突。"),
      );
    renderSettings(managementService({ createBackup }));
    fireEvent.change(screen.getByLabelText("备份保存目录"), {
      target: { value: "D:\\Backups" },
    });
    fireEvent.click(screen.getByRole("button", { name: "创建备份" }));

    await waitFor(() =>
      expect(
        screen.getByText("目标位置已存在或与现有内容冲突。"),
      ).toBeInTheDocument(),
    );
  });
});
