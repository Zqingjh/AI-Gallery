import { useMemo } from "react";
import type { CommandClient } from "../../services/command-client";
import { createTauriAiService } from "../../services/tauri-ai-service";
import {
  createWorkspaceManagementService,
  type WorkspaceAccessMode,
} from "../../services/workspace-management-service";
import SettingsPage from "./SettingsPage";

export default function SettingsRoute({
  commandClient,
  workspaceRoot,
  accessMode,
  onAccessModeChanged,
  onBack,
  showThemeToggle = true,
}: {
  readonly commandClient?: CommandClient;
  readonly workspaceRoot: string | null;
  readonly accessMode: WorkspaceAccessMode;
  readonly onAccessModeChanged: (mode: WorkspaceAccessMode) => void;
  readonly onBack: () => void;
  readonly showThemeToggle?: boolean;
}) {
  const aiService = useMemo(
    () =>
      commandClient
        ? createTauriAiService(
            commandClient,
            () => workspaceRoot,
            () => accessMode,
          )
        : undefined,
    [accessMode, commandClient, workspaceRoot],
  );
  const workspaceManagementService = useMemo(
    () =>
      commandClient
        ? createWorkspaceManagementService(commandClient)
        : undefined,
    [commandClient],
  );
  return (
    <SettingsPage
      aiService={aiService}
      workspaceManagementService={workspaceManagementService}
      workspaceRoot={workspaceRoot}
      accessMode={accessMode}
      onAccessModeChanged={onAccessModeChanged}
      onBack={onBack}
      showThemeToggle={showThemeToggle}
    />
  );
}
