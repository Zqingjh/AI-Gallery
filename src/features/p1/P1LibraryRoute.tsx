import { useMemo } from "react";
import type { CommandClient } from "../../services/command-client";
import { createTauriLibraryService } from "../../services/tauri-library-service";
import type { P1SavedAssetFilter } from "../../services/p1-library-service";
import { createTauriMediaIntegrityService } from "../../services/tauri-media-integrity-service";
import { createTauriP1LibraryService } from "../../services/tauri-p1-library-service";
import { createTauriAiService } from "../../services/tauri-ai-service";
import type { WorkspaceAccessMode } from "../../services/workspace-management-service";
import { P1LibraryPage } from "./P1LibraryPage";

export default function P1LibraryRoute({
  commandClient,
  workspaceRoot,
  accessMode,
  initialMediaAssetId,
  onBack,
  onApplyFilter,
}: {
  readonly commandClient?: CommandClient;
  readonly workspaceRoot: string | null;
  readonly accessMode: WorkspaceAccessMode;
  readonly initialMediaAssetId?: string | null;
  readonly onBack: () => void;
  readonly onApplyFilter: (filter: P1SavedAssetFilter) => void;
}) {
  const service = useMemo(
    () =>
      commandClient
        ? createTauriP1LibraryService(commandClient, () => workspaceRoot)
        : undefined,
    [commandClient, workspaceRoot],
  );
  const mediaIntegrityService = useMemo(
    () =>
      commandClient
        ? createTauriMediaIntegrityService(commandClient, () => workspaceRoot)
        : undefined,
    [commandClient, workspaceRoot],
  );
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
  const taxonomyService = useMemo(() => {
    if (!commandClient || !workspaceRoot) return undefined;
    const libraryService = createTauriLibraryService(
      commandClient,
      () => accessMode,
    );
    return {
      async listTaxonomy() {
        await libraryService.connectWorkspace(workspaceRoot);
        return libraryService.listTaxonomy();
      },
      async listMetadataPresets() {
        await libraryService.connectWorkspace(workspaceRoot);
        return libraryService.listMetadataPresets();
      },
    };
  }, [accessMode, commandClient, workspaceRoot]);

  if (!service || !taxonomyService) {
    return (
      <p className="route-loading">
        请在桌面应用中连接工作区后使用创作效率功能。
      </p>
    );
  }

  return (
    <P1LibraryPage
      service={service}
      taxonomyService={taxonomyService}
      aiService={aiService}
      mediaIntegrityService={mediaIntegrityService}
      accessMode={accessMode}
      initialMediaAssetId={initialMediaAssetId}
      onBack={onBack}
      onApplyFilter={onApplyFilter}
    />
  );
}
