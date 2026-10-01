import { lazy, Suspense, useCallback, useMemo, useRef, useState } from "react";
import { LibraryLanding } from "../features/library/LibraryLanding";
import { createTauriCommandClient } from "../services/tauri-command-client";
import { createTauriLibraryService } from "../services/tauri-library-service";
import { createTauriMediaIntegrityService } from "../services/tauri-media-integrity-service";
import type { P1SavedAssetFilter } from "../services/p1-library-service";
import { createWorkspaceAccessReader } from "../services/workspace-access-reader";
import type { WorkspaceAccessMode } from "../services/workspace-management-service";
import { text } from "./texts";
import { useRoute } from "./useRoute";

const SettingsRoute = lazy(() => import("../features/settings/SettingsRoute"));
const AiReviewRoute = lazy(() => import("../features/ai/AiReviewRoute"));
const P1LibraryRoute = lazy(() => import("../features/p1/P1LibraryRoute"));
const NsfwLibraryRoute = lazy(
  () => import("../features/nsfw/NsfwLibraryRoute"),
);
const commandClient =
  "__TAURI_INTERNALS__" in window ? createTauriCommandClient() : undefined;

export function App() {
  const { route, navigate } = useRoute();
  const [workspaceRoot, setWorkspaceRoot] = useState<string | null>(null);
  const [accessMode, setAccessMode] =
    useState<WorkspaceAccessMode>("readWrite");
  const [savedAssetFilter, setSavedAssetFilter] =
    useState<P1SavedAssetFilter | null>(null);
  const [initialMediaAssetId, setInitialMediaAssetId] = useState<string | null>(
    null,
  );
  const [nsfwActive, setNsfwActive] = useState(false);
  const [nsfwWorkspaceRoot, setNsfwWorkspaceRoot] = useState<string | null>(
    null,
  );
  const workspaceRootRef = useRef(workspaceRoot);
  const accessModeRef = useRef(accessMode);
  workspaceRootRef.current = workspaceRoot;
  accessModeRef.current = accessMode;
  const workspaceAccessReader = useMemo(
    () =>
      commandClient ? createWorkspaceAccessReader(commandClient) : undefined,
    [],
  );
  const libraryService = useMemo(
    () =>
      commandClient
        ? createTauriLibraryService(commandClient, () => accessModeRef.current)
        : undefined,
    [],
  );
  const mediaIntegrityService = useMemo(
    () =>
      commandClient
        ? createTauriMediaIntegrityService(
            commandClient,
            () => workspaceRootRef.current,
          )
        : undefined,
    [],
  );
  const handleWorkspaceChanged = useCallback(
    (rootPath: string | null) => {
      setWorkspaceRoot(rootPath);
      if (!rootPath || !workspaceAccessReader) {
        setAccessMode("readWrite");
        return;
      }
      setAccessMode("readOnly");
      void workspaceAccessReader
        .getAccessMode(rootPath)
        .then(setAccessMode)
        // 无法确认权限时以前端只读降级，后端仍负责最终拒绝所有写入。
        .catch(() => setAccessMode("readOnly"));
    },
    [workspaceAccessReader],
  );

  if (nsfwActive) {
    return (
      <Suspense
        fallback={
          <main className="nsfw-realm nsfw-activation-screen" role="status">
            <span className="nsfw-activation-orbit" aria-hidden="true" />
            <p className="eyebrow">PRIVATE SESSION</p>
            <h1>{text.nsfw.activating}</h1>
          </main>
        }
      >
        <NsfwLibraryRoute
          commandClient={commandClient}
          normalWorkspaceRoot={workspaceRoot}
          initialWorkspaceRoot={nsfwWorkspaceRoot}
          onWorkspaceRootChanged={setNsfwWorkspaceRoot}
          onExit={() => setNsfwActive(false)}
        />
      </Suspense>
    );
  }

  if (route === "/settings") {
    return (
      <Suspense
        fallback={<p className="route-loading">{text.shell.loading}</p>}
      >
        <SettingsRoute
          commandClient={commandClient}
          workspaceRoot={workspaceRoot}
          accessMode={accessMode}
          onAccessModeChanged={setAccessMode}
          onBack={() => navigate("/")}
        />
      </Suspense>
    );
  }

  if (route === "/ai-review") {
    return (
      <Suspense
        fallback={<p className="route-loading">{text.shell.loading}</p>}
      >
        <AiReviewRoute
          commandClient={commandClient}
          workspaceRoot={workspaceRoot}
          accessMode={accessMode}
          onBack={() => navigate("/")}
        />
      </Suspense>
    );
  }

  if (route === "/efficiency") {
    return (
      <Suspense
        fallback={<p className="route-loading">{text.shell.loading}</p>}
      >
        <P1LibraryRoute
          commandClient={commandClient}
          workspaceRoot={workspaceRoot}
          accessMode={accessMode}
          initialMediaAssetId={initialMediaAssetId}
          onBack={() => navigate("/")}
          onApplyFilter={(filter) => {
            setSavedAssetFilter(filter);
            navigate("/");
          }}
        />
      </Suspense>
    );
  }

  return (
    <LibraryLanding
      service={libraryService}
      mediaIntegrityService={mediaIntegrityService}
      onOpenSettings={() => navigate("/settings")}
      onOpenAiReview={() => navigate("/ai-review")}
      onOpenP1={(assetId) => {
        setInitialMediaAssetId(assetId ?? null);
        navigate("/efficiency");
      }}
      onWorkspaceChanged={handleWorkspaceChanged}
      accessMode={accessMode}
      savedAssetFilter={savedAssetFilter}
      onSecretActivate={() => setNsfwActive(true)}
    />
  );
}
