import { useMemo } from "react";
import type { CommandClient } from "../../services/command-client";
import { createTauriAiService } from "../../services/tauri-ai-service";
import type { WorkspaceAccessMode } from "../../services/workspace-management-service";
import AiReviewPage from "./AiReviewPage";

export default function AiReviewRoute({
  commandClient,
  workspaceRoot,
  accessMode,
  onBack,
  showThemeToggle = true,
}: {
  readonly commandClient?: CommandClient;
  readonly workspaceRoot: string | null;
  readonly accessMode: WorkspaceAccessMode;
  readonly onBack: () => void;
  readonly showThemeToggle?: boolean;
}) {
  const service = useMemo(
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
  return (
    <AiReviewPage
      service={service}
      accessMode={accessMode}
      onBack={onBack}
      showThemeToggle={showThemeToggle}
    />
  );
}
