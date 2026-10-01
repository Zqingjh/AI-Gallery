import {
  lazy,
  Suspense,
  type ReactNode,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { text } from "../../app/texts";
import type { CommandClient } from "../../services/command-client";
import type { P1SavedAssetFilter } from "../../services/p1-library-service";
import {
  createTauriLibraryService,
  type DisposableLibraryService,
} from "../../services/tauri-library-service";
import { createTauriMediaIntegrityService } from "../../services/tauri-media-integrity-service";
import { createWorkspaceAccessReader } from "../../services/workspace-access-reader";
import type { WorkspaceAccessMode } from "../../services/workspace-management-service";
import { disposeAssetPreviewSession } from "../library/AssetCardGrid";
import { LibraryLanding } from "../library/LibraryLanding";

const SettingsRoute = lazy(() => import("../settings/SettingsRoute"));
const AiReviewRoute = lazy(() => import("../ai/AiReviewRoute"));
const P1LibraryRoute = lazy(() => import("../p1/P1LibraryRoute"));

type PrivateView = "library" | "settings" | "review" | "efficiency";

function readStoredThemeSnapshot(): {
  readonly available: boolean;
  readonly value: string | null;
} {
  try {
    return {
      available: true,
      value: window.localStorage.getItem("ai-gallery-theme"),
    };
  } catch {
    return { available: false, value: null };
  }
}

export default function NsfwLibraryRoute({
  commandClient,
  normalWorkspaceRoot,
  initialWorkspaceRoot,
  onWorkspaceRootChanged,
  onExit,
}: {
  readonly commandClient?: CommandClient;
  readonly normalWorkspaceRoot: string | null;
  readonly initialWorkspaceRoot: string | null;
  readonly onWorkspaceRootChanged: (rootPath: string | null) => void;
  readonly onExit: () => void;
}) {
  const [view, setView] = useState<PrivateView>("library");
  const [workspaceRoot, setWorkspaceRoot] = useState<string | null>(null);
  const [accessMode, setAccessMode] =
    useState<WorkspaceAccessMode>("readWrite");
  const [savedAssetFilter, setSavedAssetFilter] =
    useState<P1SavedAssetFilter | null>(null);
  const [initialMediaAssetId, setInitialMediaAssetId] = useState<string | null>(
    null,
  );
  const [autoConnecting, setAutoConnecting] = useState(
    Boolean(initialWorkspaceRoot),
  );
  const accessModeRef = useRef(accessMode);
  const workspaceRootRef = useRef(workspaceRoot);
  const autoConnectRef = useRef<{
    readonly rootPath: string;
    readonly service: DisposableLibraryService;
    readonly promise: Promise<WorkspaceAccessMode>;
  } | null>(null);
  const releaseTimerRef = useRef<number | null>(null);
  const releasedRef = useRef(false);
  const originalThemeRef = useRef(
    document.documentElement.dataset.theme ?? "dark",
  );
  const originalStoredThemeRef = useRef(readStoredThemeSnapshot());
  accessModeRef.current = accessMode;
  workspaceRootRef.current = workspaceRoot;

  const libraryService = useMemo<DisposableLibraryService | undefined>(
    () =>
      commandClient
        ? createTauriLibraryService(
            commandClient,
            () => accessModeRef.current,
            { isolatedFromRoot: () => normalWorkspaceRoot },
          )
        : undefined,
    [commandClient, normalWorkspaceRoot],
  );
  const mediaIntegrityService = useMemo(
    () =>
      commandClient
        ? createTauriMediaIntegrityService(
            commandClient,
            () => workspaceRootRef.current,
          )
        : undefined,
    [commandClient],
  );
  const workspaceAccessReader = useMemo(
    () =>
      commandClient ? createWorkspaceAccessReader(commandClient) : undefined,
    [commandClient],
  );

  const restoreNormalTheme = useCallback(() => {
    document.documentElement.dataset.theme = originalThemeRef.current;
    try {
      const original = originalStoredThemeRef.current;
      if (original.available) {
        if (original.value === null) {
          window.localStorage.removeItem("ai-gallery-theme");
        } else {
          window.localStorage.setItem("ai-gallery-theme", original.value);
        }
      }
    } catch {
      // 本地偏好不可写不影响私密会话退出。
    }
  }, []);

  const releaseSession = useCallback(async () => {
    if (releasedRef.current) return;
    releasedRef.current = true;
    restoreNormalTheme();
    if (libraryService) {
      disposeAssetPreviewSession(libraryService);
      await libraryService.dispose();
    }
  }, [libraryService, restoreNormalTheme]);

  useEffect(() => {
    if (releaseTimerRef.current !== null) {
      window.clearTimeout(releaseTimerRef.current);
      releaseTimerRef.current = null;
    }
    return () => {
      releaseTimerRef.current = window.setTimeout(
        () => void releaseSession(),
        0,
      );
    };
  }, [releaseSession]);

  useEffect(() => {
    if (!initialWorkspaceRoot || !libraryService || !workspaceAccessReader) {
      setAutoConnecting(false);
      return;
    }
    let connection = autoConnectRef.current;
    if (
      !connection ||
      connection.rootPath !== initialWorkspaceRoot ||
      connection.service !== libraryService
    ) {
      connection = {
        rootPath: initialWorkspaceRoot,
        service: libraryService,
        promise: libraryService
          .connectWorkspace(initialWorkspaceRoot)
          .then(() =>
            workspaceAccessReader.getAccessMode(initialWorkspaceRoot),
          ),
      };
      autoConnectRef.current = connection;
    }
    let active = true;
    void connection.promise
      .then((mode) => {
        if (!active) return;
        setWorkspaceRoot(initialWorkspaceRoot);
        setAccessMode(mode);
      })
      .catch(() => {
        if (!active) return;
        setWorkspaceRoot(null);
        onWorkspaceRootChanged(null);
      })
      .finally(() => {
        if (active) setAutoConnecting(false);
      });
    return () => {
      active = false;
    };
  }, [
    initialWorkspaceRoot,
    libraryService,
    onWorkspaceRootChanged,
    workspaceAccessReader,
  ]);

  const handleWorkspaceChanged = useCallback(
    (rootPath: string | null) => {
      setWorkspaceRoot(rootPath);
      onWorkspaceRootChanged(rootPath);
      if (!rootPath || !workspaceAccessReader) {
        setAccessMode("readWrite");
        return;
      }
      setAccessMode("readOnly");
      void workspaceAccessReader
        .getAccessMode(rootPath)
        .then(setAccessMode)
        .catch(() => setAccessMode("readOnly"));
    },
    [onWorkspaceRootChanged, workspaceAccessReader],
  );

  const exit = () => {
    restoreNormalTheme();
    onExit();
  };

  if (!libraryService) {
    return (
      <main className="nsfw-realm nsfw-activation-screen">
        <p>{text.library.loadFailed}</p>
        <button type="button" onClick={exit}>
          {text.nsfw.exit}
        </button>
      </main>
    );
  }

  if (autoConnecting) {
    return (
      <main className="nsfw-realm nsfw-activation-screen" role="status">
        <span className="nsfw-activation-orbit" aria-hidden="true" />
        <p className="eyebrow">PRIVATE SESSION</p>
        <h1>{text.nsfw.activating}</h1>
        <button className="nsfw-exit-button" type="button" onClick={exit}>
          {text.nsfw.exit}
        </button>
      </main>
    );
  }

  let content: ReactNode;
  if (view === "settings") {
    content = (
      <SettingsRoute
        commandClient={commandClient}
        workspaceRoot={workspaceRoot}
        accessMode={accessMode}
        onAccessModeChanged={setAccessMode}
        onBack={() => setView("library")}
        showThemeToggle={false}
      />
    );
  } else if (view === "review") {
    content = (
      <AiReviewRoute
        commandClient={commandClient}
        workspaceRoot={workspaceRoot}
        accessMode={accessMode}
        onBack={() => setView("library")}
        showThemeToggle={false}
      />
    );
  } else if (view === "efficiency") {
    content = (
      <P1LibraryRoute
        commandClient={commandClient}
        workspaceRoot={workspaceRoot}
        accessMode={accessMode}
        initialMediaAssetId={initialMediaAssetId}
        onBack={() => setView("library")}
        onApplyFilter={(filter) => {
          setSavedAssetFilter(filter);
          setView("library");
        }}
      />
    );
  } else {
    content = (
      <LibraryLanding
        service={libraryService}
        mediaIntegrityService={mediaIntegrityService}
        onOpenSettings={() => setView("settings")}
        onOpenAiReview={() => setView("review")}
        onOpenP1={(assetId) => {
          setInitialMediaAssetId(assetId ?? null);
          setView("efficiency");
        }}
        onWorkspaceChanged={handleWorkspaceChanged}
        accessMode={accessMode}
        savedAssetFilter={savedAssetFilter}
        mode="nsfw"
        onExitPrivate={exit}
      />
    );
  }

  return (
    <div className="nsfw-realm">
      {view !== "library" ? (
        <button className="nsfw-floating-exit" type="button" onClick={exit}>
          {text.nsfw.exit}
        </button>
      ) : null}
      <Suspense
        fallback={<p className="route-loading">{text.nsfw.activating}</p>}
      >
        {content}
      </Suspense>
    </div>
  );
}
