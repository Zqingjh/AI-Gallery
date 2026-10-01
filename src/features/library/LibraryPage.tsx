import {
  lazy,
  Suspense,
  useCallback,
  useEffect,
  useRef,
  useState,
} from "react";
import { text } from "../../app/texts";
import { BrandMark } from "../../components/BrandMark";
import { FloatingSingleSelect } from "../../components/FloatingSelect";
import { ThemeToggle } from "../../components/ThemeToggle";
import type {
  AssetDetail,
  AssetSearchField,
  AssetSummary,
  DuplicateAssetGroup,
  ExportMode,
  ImportTask,
  LibraryService,
  MetadataPresets,
  NumberedAssetListRequest,
  NumberedPageResult,
  PageResult,
  ProjectDetail,
  ProjectSummary,
  WorkspaceState,
} from "../../services/library-service";
import type { WorkspaceAccessMode } from "../../services/workspace-management-service";
import type { P1SavedAssetFilter } from "../../services/p1-library-service";
import type { MediaIntegrityService } from "../../services/media-integrity-service";
import { ServiceError } from "../../services/service-error";
import {
  AssetCardGrid,
  invalidateAssetPreviewCache,
} from "./AssetCardGrid";
const CanvasProjectDetail = lazy(() =>
  import("./CanvasProjectDetail").then((module) => ({
    default: module.CanvasProjectDetail,
  })),
);
const TaxonomyPanel = lazy(() => import("./TaxonomyPanel"));
const TrashPanel = lazy(() => import("./TrashPanel"));
const AssetDetailDrawer = lazy(() =>
  import("./AssetDetailDrawer").then((module) => ({
    default: module.AssetDetailDrawer,
  })),
);
const ImportDialog = lazy(() =>
  import("./ImportDialog").then((module) => ({ default: module.ImportDialog })),
);
const AssetFormDialog = lazy(() =>
  import("./LibraryForms").then((module) => ({
    default: module.AssetFormDialog,
  })),
);
const ProjectFormDialog = lazy(() =>
  import("./LibraryForms").then((module) => ({
    default: module.ProjectFormDialog,
  })),
);
const VideoCoverDialog = lazy(() =>
  import("./VideoCoverDialog").then((module) => ({
    default: module.VideoCoverDialog,
  })),
);
const ExportDialog = lazy(() =>
  import("./ExportDialog").then((module) => ({
    default: module.ExportDialog,
  })),
);

type LibraryTab = "assets" | "projects" | "taxonomy" | "trash";
type PendingExport =
  | { readonly kind: "assets"; readonly ids: readonly string[] }
  | { readonly kind: "projects"; readonly ids: readonly string[] };

interface LibraryPageProps {
  readonly service: LibraryService;
  readonly mediaIntegrityService?: MediaIntegrityService;
  readonly onOpenSettings: () => void;
  readonly onOpenAiReview?: () => void;
  readonly onOpenP1?: (assetId?: string) => void;
  readonly onWorkspaceChanged?: (rootPath: string | null) => void;
  readonly accessMode?: WorkspaceAccessMode;
  readonly savedAssetFilter?: P1SavedAssetFilter | null;
  readonly mode?: "standard" | "nsfw";
  readonly onSecretActivate?: () => void;
  readonly onExitPrivate?: () => void;
}

const projectPageSize = 24;
const defaultAssetPageSize = 25;

const emptyAssetFilters = {
  keyword: "",
  keywordField: "title",
  mediaType: "",
  model: "",
  platform: "",
  rating: "",
  isFavorite: "",
  isPublic: "",
  createdAfter: "",
  createdBefore: "",
  minAspectRatio: "",
  maxAspectRatio: "",
  categoryIds: "",
} as const;

type AssetFilters = {
  -readonly [Key in keyof typeof emptyAssetFilters]: string;
};

function safeLibraryError(error: unknown): string {
  return error instanceof ServiceError
    ? error.message
    : text.library.loadFailed;
}

function presetFilterOptions(
  current: string,
  presets: MetadataPresets["models"],
) {
  const options = presets.map((preset) => ({
    value: preset.name,
    label: preset.name,
  }));
  return current && !presets.some((preset) => preset.name === current)
    ? [{ value: current, label: current }, ...options]
    : options;
}

function assetRequest(
  filters: AssetFilters,
  page: number,
  pageSize: 10 | 25 | 50,
): NumberedAssetListRequest {
  const dateStart = filters.createdAfter
    ? Date.parse(`${filters.createdAfter}T00:00:00`)
    : undefined;
  const dateEnd = filters.createdBefore
    ? Date.parse(`${filters.createdBefore}T23:59:59.999`)
    : undefined;
  return {
    page,
    pageSize,
    ...(filters.keyword.trim() ? { keyword: filters.keyword.trim() } : {}),
    searchField: filters.keywordField as AssetSearchField,
    ...(filters.keyword.trim() ? { exactMatch: false } : {}),
    ...(filters.mediaType
      ? {
          mediaType: filters.mediaType as AssetSummary["mediaType"],
        }
      : {}),
    ...(filters.model.trim() ? { model: filters.model.trim() } : {}),
    ...(filters.platform.trim() ? { platform: filters.platform.trim() } : {}),
    ...(filters.categoryIds.trim()
      ? {
          categoryIds: filters.categoryIds.split(/[,，\s]+/).filter(Boolean),
        }
      : {}),
    ...(filters.rating ? { rating: Number(filters.rating) } : {}),
    ...(filters.isFavorite
      ? { isFavorite: filters.isFavorite === "true" }
      : {}),
    ...(filters.isPublic ? { isPublic: filters.isPublic === "true" } : {}),
    ...(Number.isFinite(dateStart) ? { createdAfter: dateStart } : {}),
    ...(Number.isFinite(dateEnd) ? { createdBefore: dateEnd } : {}),
    ...(filters.minAspectRatio
      ? {
          minAspectRatio: Number(filters.minAspectRatio),
        }
      : {}),
    ...(filters.maxAspectRatio
      ? {
          maxAspectRatio: Number(filters.maxAspectRatio),
        }
      : {}),
  };
}

export function LibraryPage({
  service,
  mediaIntegrityService,
  onOpenAiReview,
  onOpenP1,
  onOpenSettings,
  onWorkspaceChanged,
  accessMode = "readWrite",
  savedAssetFilter,
  mode = "standard",
  onSecretActivate,
  onExitPrivate,
}: LibraryPageProps) {
  const isPrivate = mode === "nsfw";
  const tabText = isPrivate ? text.nsfw.tabs : text.library.tabs;
  const secretClicks = useRef({ count: 0, lastAt: 0, activated: false });
  const isReadOnly = accessMode === "readOnly";
  const [workspace, setWorkspace] = useState<WorkspaceState | null>(null);
  const [workspacePath, setWorkspacePath] = useState("");
  const [tab, setTab] = useState<LibraryTab>("assets");
  const [assetsPage, setAssetsPage] = useState<
    NumberedPageResult<AssetSummary>
  >({
    items: [],
    page: 1,
    pageSize: defaultAssetPageSize,
    totalCount: 0,
    totalPages: 0,
  });
  const [assetPageNumber, setAssetPageNumber] = useState(1);
  const [assetPageSize, setAssetPageSize] = useState<10 | 25 | 50>(
    defaultAssetPageSize,
  );
  const [assetFilters, setAssetFilters] = useState<AssetFilters>({
    ...emptyAssetFilters,
  });
  const [projectsPage, setProjectsPage] = useState<PageResult<ProjectSummary>>({
    items: [],
    nextCursor: null,
  });
  const [projectCursor, setProjectCursor] = useState<string | undefined>();
  const [projectCursorHistory, setProjectCursorHistory] = useState<
    readonly string[]
  >([]);
  const [selectedProject, setSelectedProject] = useState<ProjectDetail | null>(
    null,
  );
  const [projectAssetsPage, setProjectAssetsPage] = useState<
    NumberedPageResult<AssetSummary>
  >({
    items: [],
    page: 1,
    pageSize: 25,
    totalCount: 0,
    totalPages: 0,
  });
  const [projectAssetPageSize, setProjectAssetPageSize] = useState<
    10 | 25 | 50
  >(25);
  const [projectAssetsLoading, setProjectAssetsLoading] = useState(false);
  const [selectedAsset, setSelectedAsset] = useState<AssetDetail | null>(null);
  const selectedAssetRef = useRef<AssetDetail | null>(null);
  const previewCleanupTimerRef = useRef<number | null>(null);
  const libraryLoadSequenceRef = useRef(0);
  selectedAssetRef.current = selectedAsset;
  const [editingAsset, setEditingAsset] = useState<AssetDetail | "new" | null>(
    null,
  );
  const [editingProject, setEditingProject] = useState<
    ProjectDetail | "new" | null
  >(null);
  const [importOpen, setImportOpen] = useState(false);
  const [importTask, setImportTask] = useState<ImportTask | null>(null);
  const [busy, setBusy] = useState(false);
  const [loadingMore, setLoadingMore] = useState(false);
  const [duplicatePanelOpen, setDuplicatePanelOpen] = useState(false);
  const [duplicatePage, setDuplicatePage] = useState<
    PageResult<DuplicateAssetGroup>
  >({ items: [], nextCursor: null });
  const [duplicateLoading, setDuplicateLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [pendingTrash, setPendingTrash] = useState<
    | { readonly kind: "asset"; readonly item: AssetDetail }
    | { readonly kind: "assetBatch"; readonly ids: readonly string[] }
    | { readonly kind: "project"; readonly item: ProjectSummary }
    | null
  >(null);
  const [selectedAssetIds, setSelectedAssetIds] = useState<ReadonlySet<string>>(
    () => new Set(),
  );
  const [projectSelectionMode, setProjectSelectionMode] = useState(false);
  const [selectedProjectIds, setSelectedProjectIds] = useState<
    ReadonlySet<string>
  >(() => new Set());
  const [pendingExport, setPendingExport] = useState<PendingExport | null>(null);
  const [exportBusy, setExportBusy] = useState(false);
  const [metadataPresets, setMetadataPresets] = useState<MetadataPresets>({
    models: [],
    platforms: [],
  });
  const [coverAssetId, setCoverAssetId] = useState<number | null>(null);

  useEffect(() => {
    if (!savedAssetFilter) return;
    const dateValue = (value?: number) =>
      value === undefined ? "" : new Date(value).toISOString().slice(0, 10);
    setAssetFilters({
      ...emptyAssetFilters,
      keyword: savedAssetFilter.keyword ?? "",
      mediaType: savedAssetFilter.mediaType ?? "",
      model: savedAssetFilter.model ?? "",
      platform: savedAssetFilter.platform ?? "",
      categoryIds: (savedAssetFilter.categoryIds ?? []).join(","),
      rating: savedAssetFilter.rating?.toString() ?? "",
      isFavorite:
        savedAssetFilter.isFavorite === undefined
          ? ""
          : String(savedAssetFilter.isFavorite),
      isPublic:
        savedAssetFilter.isPublic === undefined
          ? ""
          : String(savedAssetFilter.isPublic),
      createdAfter: dateValue(savedAssetFilter.createdAfter),
      createdBefore: dateValue(savedAssetFilter.createdBefore),
      minAspectRatio: savedAssetFilter.minAspectRatio?.toString() ?? "",
      maxAspectRatio: savedAssetFilter.maxAspectRatio?.toString() ?? "",
    });
    setAssetPageNumber(1);
    setTab("assets");
    setNotice("已应用智能集合筛选。");
  }, [savedAssetFilter]);

  useEffect(() => {
    if (previewCleanupTimerRef.current !== null) {
      window.clearTimeout(previewCleanupTimerRef.current);
      previewCleanupTimerRef.current = null;
    }
    return () => {
      const previewUrl = selectedAssetRef.current?.previewUrl;
      if (previewUrl?.startsWith("blob:")) {
        // 延迟到本轮末尾，避免 React StrictMode 的模拟卸载提前撤销仍在使用的地址。
        previewCleanupTimerRef.current = window.setTimeout(
          () => URL.revokeObjectURL(previewUrl),
          0,
        );
      }
    };
  }, []);

  useEffect(() => {
    if (isReadOnly && (tab === "taxonomy" || tab === "trash")) {
      setTab("assets");
    }
  }, [isReadOnly, tab]);

  const loadFirstPage = useCallback(async () => {
    const sequence = libraryLoadSequenceRef.current + 1;
    libraryLoadSequenceRef.current = sequence;
    setBusy(true);
    setError(null);
    try {
      const [assets, projects] = await Promise.all([
        service.listAssetPage(
          assetRequest(assetFilters, assetPageNumber, assetPageSize),
        ),
        service.listProjects({ limit: projectPageSize }),
      ]);
      if (libraryLoadSequenceRef.current !== sequence) return;
      const lastAvailablePage = Math.max(1, assets.totalPages);
      if (assetPageNumber > lastAvailablePage) {
        setAssetPageNumber(lastAvailablePage);
        return;
      }
      setAssetsPage(assets);
      setProjectsPage(projects);
      setProjectCursor(undefined);
      setProjectCursorHistory([]);
    } catch {
      if (libraryLoadSequenceRef.current === sequence) {
        setError(text.library.loadFailed);
      }
    } finally {
      if (libraryLoadSequenceRef.current === sequence) setBusy(false);
    }
  }, [assetFilters, assetPageNumber, assetPageSize, service]);

  const handleImportTaskChange = useCallback(
    (task: ImportTask | null) => {
      setImportTask(task);
      if (task?.state === "completed") void loadFirstPage();
    },
    [loadFirstPage],
  );

  useEffect(() => {
    let active = true;
    void service
      .getWorkspaceState()
      .then((state) => {
        if (!active) return;
        setWorkspace(state);
      })
      .catch(() => {
        if (active) setWorkspace({ isOpen: false, displayName: null });
      });
    return () => {
      active = false;
    };
  }, [service]);

  useEffect(() => {
    if (workspace?.isOpen) void loadFirstPage();
  }, [loadFirstPage, workspace?.isOpen]);

  useEffect(() => {
    let active = true;
    if (workspace?.isOpen && tab === "assets") {
      void service
        .listMetadataPresets()
        .then((value) => {
          if (active) setMetadataPresets(value);
        })
        .catch(() => {
          if (active) setError(text.library.loadFailed);
        });
    }
    return () => {
      active = false;
    };
  }, [service, tab, workspace?.isOpen]);

  useEffect(() => {
    if (!notice) return;
    const timeout = window.setTimeout(() => setNotice(null), 2400);
    return () => window.clearTimeout(timeout);
  }, [notice]);

  async function connectWorkspace(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!workspacePath.trim()) {
      setError(text.library.workspacePathRequired);
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const rootPath = workspacePath.trim();
      const state = await service.connectWorkspace(rootPath);
      setWorkspace(state);
      onWorkspaceChanged?.(rootPath);
      setWorkspacePath("");
    } catch (error) {
      setError(safeLibraryError(error));
    } finally {
      setBusy(false);
    }
  }

  async function createWorkspace() {
    if (!workspacePath.trim()) {
      setError(text.library.workspacePathRequired);
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const rootPath = workspacePath.trim();
      const state = await service.createWorkspace(rootPath);
      setWorkspace(state);
      onWorkspaceChanged?.(rootPath);
      setWorkspacePath("");
    } catch (error) {
      setError(safeLibraryError(error));
    } finally {
      setBusy(false);
    }
  }

  async function selectWorkspaceDirectory() {
    try {
      const selected = await service.selectWorkspaceDirectory();
      if (selected) setWorkspacePath(selected);
    } catch {
      setError(text.library.loadFailed);
    }
  }

  async function loadMoreProjects() {
    if (!projectsPage.nextCursor) return;
    setLoadingMore(true);
    try {
      const next = await service.listProjects({
        cursor: projectsPage.nextCursor,
        limit: projectPageSize,
      });
      setProjectCursorHistory((history) => [...history, projectCursor ?? ""]);
      setProjectCursor(projectsPage.nextCursor);
      setProjectsPage(next);
    } catch {
      setError(text.library.loadFailed);
    } finally {
      setLoadingMore(false);
    }
  }

  async function loadPreviousProjects() {
    const previous = projectCursorHistory.at(-1);
    if (previous === undefined) return;
    setLoadingMore(true);
    try {
      const page = await service.listProjects({
        ...(previous ? { cursor: previous } : {}),
        limit: projectPageSize,
      });
      setProjectCursor(previous || undefined);
      setProjectCursorHistory((history) => history.slice(0, -1));
      setProjectsPage(page);
    } catch {
      setError(text.library.loadFailed);
    } finally {
      setLoadingMore(false);
    }
  }

  async function openAsset(asset: AssetSummary) {
    setError(null);
    try {
      replaceSelectedAsset(await service.getAssetDetail(asset.id));
    } catch {
      setError(text.library.loadFailed);
    }
  }

  function replaceSelectedAsset(
    next: AssetDetail | null,
    preservePreviewForSameAsset = false,
  ) {
    setSelectedAsset((current) => {
      const resolvedNext =
        preservePreviewForSameAsset && current && next?.id === current.id
          ? {
              ...next,
              previewUrl: next.previewUrl ?? current.previewUrl,
              previewError: next.previewUrl
                ? next.previewError
                : current.previewError,
            }
          : next;
      if (
        current?.previewUrl?.startsWith("blob:") &&
        current.previewUrl !== resolvedNext?.previewUrl
      ) {
        URL.revokeObjectURL(current.previewUrl);
      }
      return resolvedNext;
    });
  }

  function closeSelectedAsset() {
    replaceSelectedAsset(null);
  }

  async function loadDuplicateGroups(cursor?: string) {
    setDuplicateLoading(true);
    setError(null);
    try {
      const page = await service.listDuplicateGroups({
        ...(cursor ? { cursor } : {}),
        limit: projectPageSize,
      });
      setDuplicatePage((current) =>
        cursor
          ? {
              items: [...current.items, ...page.items],
              nextCursor: page.nextCursor,
            }
          : page,
      );
    } catch {
      setError(text.library.loadFailed);
    } finally {
      setDuplicateLoading(false);
    }
  }

  function toggleDuplicatePanel() {
    const nextOpen = !duplicatePanelOpen;
    setDuplicatePanelOpen(nextOpen);
    if (nextOpen && duplicatePage.items.length === 0) {
      void loadDuplicateGroups();
    }
  }

  async function editProject(project: ProjectSummary) {
    setError(null);
    try {
      setEditingProject(await service.getProjectDetail(project.id));
    } catch {
      setError(text.library.loadFailed);
    }
  }

  async function openProject(
    project: ProjectSummary,
    page = 1,
    pageSize = projectAssetPageSize,
  ) {
    setProjectAssetsLoading(true);
    setSelectedAssetIds(new Set());
    setError(null);
    try {
      const detail = await service.getProjectDetail(project.id);
      setSelectedProject(detail);
      if (detail.kind === "canvas") {
        setProjectAssetsPage({
          items: [],
          page: 1,
          pageSize,
          totalCount: detail.assetCount,
          totalPages:
            detail.assetCount === 0
              ? 0
              : Math.ceil(detail.assetCount / pageSize),
        });
      } else {
        setProjectAssetsPage(
          await service.listAssetPage({
            projectId: project.id,
            page,
            pageSize,
          }),
        );
      }
    } catch {
      setError(text.library.loadFailed);
    } finally {
      setProjectAssetsLoading(false);
    }
  }

  async function removeSelectedAssetsFromProject() {
    if (!selectedProject || selectedAssetIds.size === 0) return;
    const assetIds = [...selectedAssetIds];
    if (!window.confirm(text.library.project.removeConfirm(assetIds.length))) {
      return;
    }
    setProjectAssetsLoading(true);
    setError(null);
    try {
      await service.removeAssetsFromProject(selectedProject.id, assetIds);
      setSelectedAssetIds(new Set());
      const targetPage =
        assetIds.length >= projectAssetsPage.items.length &&
        projectAssetsPage.page > 1
          ? projectAssetsPage.page - 1
          : projectAssetsPage.page;
      await openProject(
        selectedProject,
        targetPage,
        projectAssetsPage.pageSize,
      );
    } catch {
      setError(text.library.project.removeFailed);
      setProjectAssetsLoading(false);
    }
  }

  async function loadProjectAssets(
    page: number,
    pageSize = projectAssetPageSize,
  ) {
    if (!selectedProject) return;
    setProjectAssetsLoading(true);
    setError(null);
    try {
      setProjectAssetsPage(
        await service.listAssetPage({
          projectId: selectedProject.id,
          page,
          pageSize,
        }),
      );
    } catch {
      setError(text.library.loadFailed);
    } finally {
      setProjectAssetsLoading(false);
    }
  }

  async function confirmTrash() {
    if (!pendingTrash) return;
    try {
      if (pendingTrash.kind === "asset") {
        await service.moveAssetToTrash(pendingTrash.item.id);
        invalidateAssetPreviewCache(service, [pendingTrash.item.id]);
        closeSelectedAsset();
      } else if (pendingTrash.kind === "assetBatch") {
        await service.moveAssetsToTrash(pendingTrash.ids);
        invalidateAssetPreviewCache(service, pendingTrash.ids);
        setSelectedAssetIds(new Set());
      } else {
        await service.moveProjectToTrash(pendingTrash.item.id);
        if (selectedProject?.id === pendingTrash.item.id) {
          setSelectedProject(null);
        }
        setProjectsPage((page) => ({
          ...page,
          items: page.items.filter(
            (project) => project.id !== pendingTrash.item.id,
          ),
        }));
        setSelectedProjectIds((current) => {
          if (!current.has(pendingTrash.item.id)) return current;
          const next = new Set(current);
          next.delete(pendingTrash.item.id);
          return next;
        });
      }
      setPendingTrash(null);
      await loadFirstPage();
      setNotice(text.library.trash.confirmDescription);
    } catch {
      setPendingTrash(null);
      setError(text.library.loadFailed);
    }
  }

  async function exportSelection(mode: ExportMode) {
    if (!pendingExport) return;
    setExportBusy(true);
    setError(null);
    try {
      const targetDirectory = await service.selectExportDirectory();
      if (!targetDirectory) return;
      const name =
        pendingExport.kind === "assets"
          ? await service.exportAssets(
              pendingExport.ids,
              mode,
              targetDirectory,
            )
          : await service.exportProjects(
              pendingExport.ids,
              mode,
              targetDirectory,
            );
      setPendingExport(null);
      setNotice(text.library.export.completed(name));
    } catch {
      setNotice(text.library.export.failed);
    } finally {
      setExportBusy(false);
    }
  }

  function toggleAssetSelection(assetId: string) {
    if (!selectedAssetIds.has(assetId) && selectedAssetIds.size >= 100) {
      setNotice(text.library.detail.selectAllLimit);
      return;
    }
    setSelectedAssetIds((current) => {
      const next = new Set(current);
      if (next.has(assetId)) next.delete(assetId);
      else next.add(assetId);
      return next;
    });
  }

  function toggleCurrentPageSelection() {
    const pageIds = assetsPage.items.map((asset) => asset.id);
    const allSelected =
      pageIds.length > 0 && pageIds.every((id) => selectedAssetIds.has(id));
    const additionalCount = pageIds.filter(
      (id) => !selectedAssetIds.has(id),
    ).length;
    if (!allSelected && selectedAssetIds.size + additionalCount > 100) {
      setNotice(text.library.detail.selectAllLimit);
      return;
    }
    setSelectedAssetIds((current) => {
      const next = new Set(current);
      pageIds.forEach((id) => (allSelected ? next.delete(id) : next.add(id)));
      return next;
    });
  }

  function toggleProjectSelection(projectId: string) {
    if (!selectedProjectIds.has(projectId) && selectedProjectIds.size >= 100) {
      setNotice(text.library.export.selectionLimit);
      return;
    }
    setSelectedProjectIds((current) => {
      const next = new Set(current);
      if (next.has(projectId)) next.delete(projectId);
      else next.add(projectId);
      return next;
    });
  }

  function toggleCurrentPageProjectSelection() {
    const pageIds = projectsPage.items.map((project) => project.id);
    const allSelected =
      pageIds.length > 0 && pageIds.every((id) => selectedProjectIds.has(id));
    const additionalCount = pageIds.filter(
      (id) => !selectedProjectIds.has(id),
    ).length;
    if (!allSelected && selectedProjectIds.size + additionalCount > 100) {
      setNotice(text.library.export.selectionLimit);
      return;
    }
    setSelectedProjectIds((current) => {
      const next = new Set(current);
      pageIds.forEach((id) =>
        allSelected ? next.delete(id) : next.add(id),
      );
      return next;
    });
  }

  async function selectAllFilteredAssets() {
    if (assetsPage.totalCount > 100) {
      setNotice(text.library.detail.selectAllLimit);
      return;
    }
    try {
      const pages = await Promise.all(
        Array.from({ length: assetsPage.totalPages }, (_, index) =>
          service.listAssetPage(
            assetRequest(assetFilters, index + 1, assetPageSize),
          ),
        ),
      );
      setSelectedAssetIds(
        new Set(pages.flatMap((page) => page.items.map((item) => item.id))),
      );
    } catch {
      setError(text.library.loadFailed);
    }
  }

  function activateSecretMode() {
    const state = secretClicks.current;
    if (!onSecretActivate || state.activated) return;
    const now = Date.now();
    state.count =
      state.lastAt > 0 && now - state.lastAt <= 1_000 ? state.count + 1 : 1;
    state.lastAt = now;
    if (state.count < 5) return;
    state.count = 0;
    state.activated = true;
    onSecretActivate();
  }

  if (workspace === null) {
    return <p className="route-loading">{text.library.loadingLibrary}</p>;
  }

  if (!workspace.isOpen) {
    return (
      <main className="workspace-gate">
        <div className="workspace-gate-brand">
          <BrandMark />
        </div>
        <p className="eyebrow">
          {isPrivate
            ? text.nsfw.workspaceEyebrow
            : text.library.workspaceEyebrow}
        </p>
        <h1>
          {isPrivate ? text.nsfw.workspaceTitle : text.library.workspaceTitle}
        </h1>
        <p>
          {isPrivate
            ? text.nsfw.workspaceDescription
            : text.library.workspaceDescription}
        </p>
        <form onSubmit={(event) => void connectWorkspace(event)}>
          <label htmlFor="workspace-path">{text.library.workspacePath}</label>
          <div className="workspace-path-row">
            <input
              id="workspace-path"
              value={workspacePath}
              placeholder={text.library.workspacePathPlaceholder}
              autoComplete="off"
              onChange={(event) => setWorkspacePath(event.target.value)}
            />
            <button
              className="secondary-button"
              type="button"
              onClick={() => void selectWorkspaceDirectory()}
            >
              {text.library.selectWorkspaceDirectory}
            </button>
            <button
              className="secondary-button"
              type="button"
              disabled={busy}
              onClick={() => void createWorkspace()}
            >
              {text.library.createWorkspace}
            </button>
            <button className="primary-button" type="submit" disabled={busy}>
              {busy
                ? text.library.connectingWorkspace
                : text.library.connectWorkspace}
            </button>
          </div>
        </form>
        {error ? (
          <p className="form-error" role="alert">
            {error}
          </p>
        ) : null}
      </main>
    );
  }

  return (
    <div
      className={`app-shell library-shell${isPrivate ? " nsfw-library-shell" : ""}`}
      data-library-mode={mode}
    >
      <header className="topbar library-topbar">
        <a className="brand" href="/" aria-label={text.productName}>
          <BrandMark />
          <span>
            <strong>{isPrivate ? text.nsfw.brand : text.productName}</strong>
            <small>{workspace.displayName}</small>
          </span>
        </a>
        <nav className="library-tabs" aria-label={text.library.tabsLabel}>
          {(isReadOnly
            ? (["assets", "projects"] as const)
            : (["assets", "projects", "taxonomy", "trash"] as const)
          ).map((item) => (
            <button
              key={item}
              className={tab === item ? "is-active" : ""}
              type="button"
              aria-pressed={tab === item}
              onClick={() => setTab(item)}
            >
              {tabText[item]}
            </button>
          ))}
        </nav>
        <div className="topbar-actions">
          <span className="privacy-pill">
            <i />
            {isPrivate ? text.nsfw.localOnly : text.library.localOnly}
          </span>
          {isReadOnly ? (
            <span className="readonly-pill">
              {text.workspace.readOnlyActive}
            </span>
          ) : null}
          {isPrivate ? null : <ThemeToggle />}
          {onOpenP1 ? (
            <button
              className="text-button"
              type="button"
              onClick={() => onOpenP1()}
            >
              {text.nav.efficiency}
            </button>
          ) : null}
          {onOpenAiReview && !isReadOnly ? (
            <button
              className="text-button"
              type="button"
              onClick={onOpenAiReview}
            >
              {text.nav.review}
            </button>
          ) : null}
          <button
            className="text-button settings-button"
            type="button"
            onClick={onOpenSettings}
          >
            {text.nav.settings}
          </button>
          {isPrivate && onExitPrivate ? (
            <button
              className="nsfw-exit-button"
              type="button"
              onClick={onExitPrivate}
            >
              {text.nsfw.exit}
            </button>
          ) : null}
        </div>
      </header>

      <main className="library-main">
        <header className="library-heading">
          <div>
            <p className="eyebrow">
              {isPrivate
                ? text.nsfw.workspaceReady
                : text.library.workspaceReady}{" "}
              · {workspace.displayName}
            </p>
            {tab === "assets" && !isPrivate && onSecretActivate ? (
              <h1>
                <button
                  className="library-title-frame"
                  type="button"
                  onClick={activateSecretMode}
                >
                  {tabText[tab]}
                </button>
              </h1>
            ) : (
              <h1>{tabText[tab]}</h1>
            )}
          </div>
          {isReadOnly ? (
            <p className="readonly-notice" role="status">
              {text.workspace.readOnlyDescription}
            </p>
          ) : null}
          <div className="library-actions">
            {tab === "assets" ? (
              <button
                className="secondary-button"
                type="button"
                aria-expanded={duplicatePanelOpen}
                onClick={toggleDuplicatePanel}
              >
                {duplicatePanelOpen
                  ? text.library.closePanel
                  : text.library.duplicateGroups}
              </button>
            ) : null}
            {!isReadOnly &&
            tab === "projects" &&
            selectedProject === null ? (
              <button
                className="secondary-button"
                type="button"
                aria-pressed={projectSelectionMode}
                onClick={() => {
                  setProjectSelectionMode((current) => !current);
                  if (projectSelectionMode) setSelectedProjectIds(new Set());
                }}
              >
                {projectSelectionMode
                  ? text.library.export.cancelProjectSelection
                  : text.library.export.chooseProjects}
              </button>
            ) : null}
            {!isReadOnly && (tab === "assets" || tab === "projects") ? (
              <>
                <button
                  className="secondary-button"
                  type="button"
                  onClick={() => setEditingProject("new")}
                >
                  {isPrivate ? text.nsfw.addProject : text.library.addProject}
                </button>
                <button
                  className="primary-button"
                  type="button"
                  onClick={() => setImportOpen(true)}
                >
                  {isPrivate ? text.nsfw.importMedia : text.library.importMedia}
                </button>
              </>
            ) : null}
          </div>
        </header>

        {error ? (
          <div className="inline-error" role="alert">
            <span>{error}</span>
            <button type="button" onClick={() => void loadFirstPage()}>
              {text.library.retry}
            </button>
          </div>
        ) : null}

        {tab === "assets" ? (
          <>
            <AssetFiltersBar
              filters={assetFilters}
              metadataPresets={metadataPresets}
              onChange={(next) => {
                setAssetFilters(next);
                setAssetPageNumber(1);
                setSelectedAssetIds(new Set());
              }}
            />
            <div className="asset-selection-bar">
              <label className="check-field">
                <input
                  type="checkbox"
                  checked={
                    assetsPage.items.length > 0 &&
                    assetsPage.items.every((asset) =>
                      selectedAssetIds.has(asset.id),
                    )
                  }
                  onChange={toggleCurrentPageSelection}
                />
                {text.library.detail.selectAllPage}
              </label>
              <button
                className="secondary-button"
                type="button"
                disabled={assetsPage.totalCount === 0}
                onClick={() => void selectAllFilteredAssets()}
              >
                {text.library.detail.selectAllResults}
              </button>
              <span>
                {text.library.detail.selectedAssets(selectedAssetIds.size)}
              </span>
              {!isReadOnly ? (
                <>
                  <button
                    className="secondary-button"
                    type="button"
                    disabled={selectedAssetIds.size === 0}
                    onClick={() =>
                      setPendingExport({
                        kind: "assets",
                        ids: [...selectedAssetIds],
                      })
                    }
                  >
                    {text.library.export.exportSelected}
                  </button>
                  <button
                    className="danger-button"
                    type="button"
                    aria-label={text.library.detail.moveSelectedToTrashAria}
                    disabled={selectedAssetIds.size === 0}
                    onClick={() =>
                      setPendingTrash({
                        kind: "assetBatch",
                        ids: [...selectedAssetIds],
                      })
                    }
                  >
                    {text.library.detail.moveSelectedToTrash}
                  </button>
                </>
              ) : null}
            </div>
            <AssetGrid
              assets={assetsPage.items}
              busy={busy}
              service={service}
              page={assetsPage.page}
              pageSize={assetPageSize}
              totalCount={assetsPage.totalCount}
              totalPages={assetsPage.totalPages}
              selectedIds={selectedAssetIds}
              onToggleSelection={toggleAssetSelection}
              onOpen={(asset) => void openAsset(asset)}
              onPageChange={setAssetPageNumber}
              onPageSizeChange={(next) => {
                setAssetPageSize(next);
                setAssetPageNumber(1);
              }}
            />
            {duplicatePanelOpen ? (
              <DuplicateGroupsPanel
                groups={duplicatePage.items}
                loading={duplicateLoading}
                nextCursor={duplicatePage.nextCursor}
                onLoadMore={() =>
                  duplicatePage.nextCursor
                    ? void loadDuplicateGroups(duplicatePage.nextCursor)
                    : undefined
                }
                onOpen={(group) =>
                  void openAsset({
                    id: group.representativeAssetId,
                    projectId: null,
                    fileName: group.representativeFileName,
                    mediaType: "image",
                    thumbnailUrl: null,
                    modelName: "",
                    platformName: "",
                    rating: 0,
                    isFavorite: false,
                    isPublic: false,
                    width: null,
                    height: null,
                    durationMs: null,
                    updatedAt: group.updatedAt,
                  })
                }
              />
            ) : null}
          </>
        ) : null}

        {tab === "projects" ? (
          selectedProject ? (
            <ProjectDetailPage
              project={selectedProject}
              assets={projectAssetsPage}
              loading={projectAssetsLoading}
              service={service}
              selectedIds={selectedAssetIds}
              onToggleSelection={toggleAssetSelection}
              onOpenAsset={(asset) => void openAsset(asset)}
              onBack={() => {
                setSelectedProject(null);
                setSelectedAssetIds(new Set());
              }}
              onEdit={() => void editProject(selectedProject)}
              onRemoveSelected={removeSelectedAssetsFromProject}
              readOnly={isReadOnly}
              onPageChange={(page) => void loadProjectAssets(page)}
              onPageSizeChange={(pageSize) => {
                setProjectAssetPageSize(pageSize);
                void loadProjectAssets(1, pageSize);
              }}
            />
          ) : (
            <>
              {projectSelectionMode && !isReadOnly ? (
                <div className="asset-selection-bar">
                  <label className="check-field">
                    <input
                      type="checkbox"
                      checked={
                        projectsPage.items.length > 0 &&
                        projectsPage.items.every((project) =>
                          selectedProjectIds.has(project.id),
                        )
                      }
                      onChange={toggleCurrentPageProjectSelection}
                    />
                    {text.library.export.selectPageProjects}
                  </label>
                  <span>
                    {text.library.export.selectedProjects(
                      selectedProjectIds.size,
                    )}
                  </span>
                  <button
                    className="secondary-button"
                    type="button"
                    disabled={selectedProjectIds.size === 0}
                    onClick={() =>
                      setPendingExport({
                        kind: "projects",
                        ids: [...selectedProjectIds],
                      })
                    }
                  >
                    {text.library.export.exportSelected}
                  </button>
                </div>
              ) : null}
              <ProjectGrid
                projects={projectsPage.items}
                onOpen={(project) => void openProject(project)}
                onEdit={(project) => void editProject(project)}
                onDelete={(project) =>
                  setPendingTrash({ kind: "project", item: project })
                }
                selectionMode={projectSelectionMode && !isReadOnly}
                selectedIds={selectedProjectIds}
                onToggleSelection={toggleProjectSelection}
                readOnly={isReadOnly}
                nextCursor={projectsPage.nextCursor}
                hasPrevious={projectCursorHistory.length > 0}
                loading={loadingMore}
                onNext={() => void loadMoreProjects()}
                onPrevious={() => void loadPreviousProjects()}
              />
            </>
          )
        ) : null}

        {tab === "taxonomy" ? (
          <Suspense
            fallback={<p className="route-loading">{text.shell.loading}</p>}
          >
            <TaxonomyPanel
              service={service}
              onOpenAsset={(asset) => void openAsset(asset)}
            />
          </Suspense>
        ) : null}

        {tab === "trash" ? (
          <Suspense
            fallback={<p className="route-loading">{text.shell.loading}</p>}
          >
            <TrashPanel
              service={service}
              onRestored={() => void loadFirstPage()}
            />
          </Suspense>
        ) : null}
      </main>

      {selectedAsset ? (
        <AssetDetailDrawer
          asset={selectedAsset}
          service={service}
          onClose={closeSelectedAsset}
          readOnly={isReadOnly}
          onEdit={() => setEditingAsset(selectedAsset)}
          onTrash={() =>
            setPendingTrash({ kind: "asset", item: selectedAsset })
          }
          onNotice={setNotice}
          onManageVideoCover={
            selectedAsset.mediaType === "video" && mediaIntegrityService
              ? () => setCoverAssetId(Number(selectedAsset.id))
              : undefined
          }
          onUpdateOrder={async (targetPosition, mode) => {
            try {
              const updated = await service.updateAssetDisplayOrder(
                selectedAsset.id,
                {
                  targetPosition,
                  mode,
                },
              );
              replaceSelectedAsset(updated, true);
              await loadFirstPage();
              setNotice(text.library.form.saved);
            } catch {
              setError(text.library.loadFailed);
            }
          }}
        />
      ) : null}
      {coverAssetId !== null && mediaIntegrityService ? (
        <Suspense fallback={null}>
          <VideoCoverDialog
            assetId={coverAssetId}
            service={mediaIntegrityService}
            onClose={() => setCoverAssetId(null)}
            onSaved={() => void loadFirstPage()}
          />
        </Suspense>
      ) : null}

      {editingProject ? (
        <ProjectFormDialog
          service={service}
          project={editingProject === "new" ? null : editingProject}
          onClose={() => setEditingProject(null)}
          onSaved={(project) => {
            setProjectsPage((page) => ({
              ...page,
              items: [
                project,
                ...page.items.filter((item) => item.id !== project.id),
              ],
            }));
            if (selectedProject?.id === project.id) {
              void openProject(project, 1, projectAssetPageSize);
            }
            setEditingProject(null);
          }}
        />
      ) : null}

      {editingAsset ? (
        <AssetFormDialog
          service={service}
          asset={editingAsset === "new" ? null : editingAsset}
          projects={projectsPage.items}
          onClose={() => setEditingAsset(null)}
          onSaved={(asset) => {
            setAssetsPage((page) => ({
              ...page,
              items: page.items.map((item) =>
                item.id === asset.id ? asset : item,
              ),
            }));
            replaceSelectedAsset(asset, true);
            void loadFirstPage();
            setEditingAsset(null);
            setNotice(text.library.form.saved);
          }}
        />
      ) : null}

      {importOpen ? (
        <ImportDialog
          service={service}
          projects={projectsPage.items}
          currentTask={importTask}
          onTaskChange={handleImportTaskChange}
          onClose={() => {
            setImportOpen(false);
            if (
              importTask &&
              ["completed", "cancelled", "failed"].includes(importTask.state)
            ) {
              setImportTask(null);
            }
          }}
        />
      ) : null}

      {pendingExport ? (
        <Suspense fallback={null}>
          <ExportDialog
            count={pendingExport.ids.length}
            busy={exportBusy}
            onClose={() => setPendingExport(null)}
            onExport={(mode) => void exportSelection(mode)}
          />
        </Suspense>
      ) : null}

      {pendingTrash ? (
        <ConfirmTrashDialog
          entityType={pendingTrash.kind === "project" ? "project" : "asset"}
          count={
            pendingTrash.kind === "assetBatch" ? pendingTrash.ids.length : 1
          }
          onCancel={() => setPendingTrash(null)}
          onConfirm={() => void confirmTrash()}
        />
      ) : null}

      {notice ? (
        <div className="toast" role="status">
          {notice}
        </div>
      ) : null}
    </div>
  );
}

interface AssetGridProps {
  readonly assets: readonly AssetSummary[];
  readonly busy: boolean;
  readonly service: LibraryService;
  readonly page: number;
  readonly pageSize: 10 | 25 | 50;
  readonly totalCount: number;
  readonly totalPages: number;
  readonly selectedIds: ReadonlySet<string>;
  readonly onToggleSelection: (assetId: string) => void;
  readonly selectable?: boolean;
  readonly onOpen: (asset: AssetSummary) => void;
  readonly onPageChange: (page: number) => void;
  readonly onPageSizeChange: (pageSize: 10 | 25 | 50) => void;
}

function AssetGrid({
  assets,
  busy,
  service,
  page,
  pageSize,
  totalCount,
  totalPages,
  selectedIds,
  onToggleSelection,
  selectable = true,
  onOpen,
  onPageChange,
  onPageSizeChange,
}: AssetGridProps) {
  if (busy && assets.length === 0)
    return <p className="route-loading">{text.library.loadingLibrary}</p>;
  if (assets.length === 0)
    return (
      <EmptyState
        title={text.library.emptyAssets}
        description={text.library.emptyAssetsHint}
      />
    );
  return (
    <>
      <AssetCardGrid
        assets={assets}
        service={service}
        onOpen={onOpen}
        selectedIds={selectedIds}
        onToggleSelection={selectable ? onToggleSelection : undefined}
      />
      <nav className="numbered-pagination" aria-label={text.library.pagination}>
        <p>{text.library.paginationSummary(totalCount)}</p>
        <div className="page-buttons">
          <button
            className="secondary-button"
            type="button"
            disabled={page <= 1}
            onClick={() => onPageChange(page - 1)}
          >
            {text.library.previousPage}
          </button>
          {paginationItems(page, totalPages).map((item, index) =>
            item === "ellipsis" ? (
              <span className="page-ellipsis" key={`ellipsis-${index}`}>
                …
              </span>
            ) : (
              <button
                className="page-number"
                type="button"
                aria-current={item === page ? "page" : undefined}
                aria-label={text.library.pageLabel(item)}
                key={item}
                onClick={() => onPageChange(item)}
              >
                {item}
              </button>
            ),
          )}
          <button
            className="secondary-button"
            type="button"
            disabled={page >= totalPages}
            onClick={() => onPageChange(page + 1)}
          >
            {text.library.nextPage}
          </button>
        </div>
        <label className="page-size-select">
          <span>{text.library.pageSize}</span>
          <select
            value={pageSize}
            onChange={(event) =>
              onPageSizeChange(Number(event.target.value) as 10 | 25 | 50)
            }
          >
            {[10, 25, 50].map((size) => (
              <option key={size} value={size}>
                {size}
              </option>
            ))}
          </select>
        </label>
      </nav>
    </>
  );
}

function paginationItems(
  currentPage: number,
  totalPages: number,
): readonly (number | "ellipsis")[] {
  if (totalPages <= 7) {
    return Array.from({ length: totalPages }, (_, index) => index + 1);
  }
  const pages = new Set([1, totalPages]);
  for (
    let page = Math.max(2, currentPage - 1);
    page <= Math.min(totalPages - 1, currentPage + 1);
    page += 1
  ) {
    pages.add(page);
  }
  const ordered = [...pages].sort((left, right) => left - right);
  const result: (number | "ellipsis")[] = [];
  ordered.forEach((page, index) => {
    if (index > 0 && page - ordered[index - 1] > 1) result.push("ellipsis");
    result.push(page);
  });
  return result;
}

function AssetFiltersBar({
  filters,
  metadataPresets,
  onChange,
}: {
  readonly filters: AssetFilters;
  readonly metadataPresets: MetadataPresets;
  readonly onChange: (next: AssetFilters) => void;
}) {
  const update = (key: keyof AssetFilters, value: string) =>
    onChange({ ...filters, [key]: value });
  const hasFilters = Object.entries(filters).some(
    ([key, value]) => key !== "keywordField" && Boolean(value),
  );
  return (
    <section className="asset-filters" aria-label={text.library.filters}>
      <div className="asset-filter-top">
        <label className="asset-search-field">
          <span>{text.library.searchField}</span>
          <select
            aria-label={text.library.searchField}
            value={filters.keywordField}
            onChange={(event) => update("keywordField", event.target.value)}
          >
            <option value="title">{text.library.searchTitle}</option>
            <option value="prompt">{text.library.searchPrompt}</option>
            <option value="notes">{text.library.searchNotes}</option>
          </select>
        </label>
        <label className="asset-search">
          <span className="sr-only">{text.library.search}</span>
          <input
            value={filters.keyword}
            placeholder={text.library.searchPlaceholder(
              filters.keywordField as AssetSearchField,
            )}
            onChange={(event) => update("keyword", event.target.value)}
          />
        </label>
        {hasFilters ? (
          <button
            className="text-button"
            type="button"
            onClick={() => onChange({ ...emptyAssetFilters })}
          >
            {text.library.clearFilters}
          </button>
        ) : null}
      </div>
      <div className="filter-group">
        <p className="filter-group-title">{text.library.filterBasics}</p>
        <div className="filter-fields filter-fields-basics">
          <label>
            <span>{text.library.filterType}</span>
            <select
              value={filters.mediaType}
              onChange={(event) => update("mediaType", event.target.value)}
            >
              <option value="">{text.library.any}</option>
              <option value="image">{text.library.image}</option>
              <option value="video">{text.library.video}</option>
            </select>
          </label>
          <label>
            <span>{text.library.filterModel}</span>
            <FloatingSingleSelect
              label={text.library.filterModel}
              value={filters.model}
              options={presetFilterOptions(
                filters.model,
                metadataPresets.models,
              )}
              onChange={(value) => update("model", value)}
            />
          </label>
          <label>
            <span>{text.library.filterPlatform}</span>
            <FloatingSingleSelect
              label={text.library.filterPlatform}
              value={filters.platform}
              options={presetFilterOptions(
                filters.platform,
                metadataPresets.platforms,
              )}
              onChange={(value) => update("platform", value)}
            />
          </label>
          <label>
            <span>{text.library.filterRating}</span>
            <select
              value={filters.rating}
              onChange={(event) => update("rating", event.target.value)}
            >
              <option value="">{text.library.any}</option>
              {[1, 2, 3, 4, 5].map((rating) => (
                <option key={rating} value={rating}>
                  {text.library.ratingAtLeast(rating)}
                </option>
              ))}
            </select>
          </label>
          <BooleanFilter
            label={text.library.filterFavorite}
            value={filters.isFavorite}
            onChange={(value) => update("isFavorite", value)}
          />
          <BooleanFilter
            label={text.library.filterPublic}
            value={filters.isPublic}
            onChange={(value) => update("isPublic", value)}
          />
        </div>
      </div>
      <div className="filter-group">
        <p className="filter-group-title">{text.library.filterTimeAndShape}</p>
        <div className="filter-fields filter-fields-range">
          <label>
            <span>{text.library.dateFrom}</span>
            <input
              type="date"
              max={filters.createdBefore || undefined}
              value={filters.createdAfter}
              onChange={(event) => update("createdAfter", event.target.value)}
            />
          </label>
          <label>
            <span>{text.library.dateTo}</span>
            <input
              type="date"
              min={filters.createdAfter || undefined}
              value={filters.createdBefore}
              onChange={(event) => update("createdBefore", event.target.value)}
            />
          </label>
          <label>
            <span>{text.library.minAspectRatio}</span>
            <input
              type="number"
              min="0.1"
              step="0.1"
              value={filters.minAspectRatio}
              onChange={(event) => update("minAspectRatio", event.target.value)}
            />
          </label>
          <label>
            <span>{text.library.maxAspectRatio}</span>
            <input
              type="number"
              min="0.1"
              step="0.1"
              value={filters.maxAspectRatio}
              onChange={(event) => update("maxAspectRatio", event.target.value)}
            />
          </label>
        </div>
      </div>
    </section>
  );
}

function BooleanFilter({
  label,
  value,
  onChange,
}: {
  readonly label: string;
  readonly value: string;
  readonly onChange: (value: string) => void;
}) {
  return (
    <label>
      <span>{label}</span>
      <select value={value} onChange={(event) => onChange(event.target.value)}>
        <option value="">{text.library.any}</option>
        <option value="true">{text.library.yes}</option>
        <option value="false">{text.library.no}</option>
      </select>
    </label>
  );
}

function DuplicateGroupsPanel({
  groups,
  loading,
  nextCursor,
  onLoadMore,
  onOpen,
}: {
  readonly groups: readonly DuplicateAssetGroup[];
  readonly loading: boolean;
  readonly nextCursor: string | null;
  readonly onLoadMore: () => void;
  readonly onOpen: (group: DuplicateAssetGroup) => void;
}) {
  return (
    <section
      className="duplicate-groups-panel"
      aria-label={text.library.duplicateGroupsTitle}
    >
      <header>
        <div>
          <p className="eyebrow">HASH MATCH</p>
          <h2>{text.library.duplicateGroupsTitle}</h2>
        </div>
        <p>{text.library.duplicateGroupsDescription}</p>
      </header>
      {!loading && groups.length === 0 ? (
        <p className="management-empty">{text.library.duplicateGroupsEmpty}</p>
      ) : null}
      <div className="duplicate-group-list">
        {groups.map((group) => (
          <button
            key={group.contentHash}
            type="button"
            onClick={() => onOpen(group)}
          >
            <strong>{group.representativeFileName}</strong>
            <span>{text.library.duplicateGroupCount(group.assetCount)}</span>
          </button>
        ))}
      </div>
      {nextCursor ? (
        <button
          className="secondary-button"
          type="button"
          disabled={loading}
          onClick={onLoadMore}
        >
          {loading ? text.library.loadingMore : text.library.loadMore}
        </button>
      ) : null}
    </section>
  );
}

function ProjectDetailPage({
  project,
  assets,
  loading,
  service,
  selectedIds,
  onToggleSelection,
  onOpenAsset,
  onBack,
  onEdit,
  onRemoveSelected,
  onPageChange,
  onPageSizeChange,
  readOnly,
}: {
  readonly project: ProjectDetail;
  readonly assets: NumberedPageResult<AssetSummary>;
  readonly loading: boolean;
  readonly service: LibraryService;
  readonly selectedIds: ReadonlySet<string>;
  readonly onToggleSelection: (assetId: string) => void;
  readonly onOpenAsset: (asset: AssetSummary) => void;
  readonly onBack: () => void;
  readonly onEdit: () => void;
  readonly onRemoveSelected: () => Promise<void>;
  readonly onPageChange: (page: number) => void;
  readonly onPageSizeChange: (pageSize: 10 | 25 | 50) => void;
  readonly readOnly: boolean;
}) {
  return (
    <section
      className="project-detail-page"
      aria-labelledby="project-detail-title"
    >
      <header className="project-detail-heading">
        <div>
          <button className="text-button" type="button" onClick={onBack}>
            ← {text.library.project.back}
          </button>
          <p className="project-detail-meta">
            {text.library.project.idLabel} {project.id} ·{" "}
            {text.library.assetCount(project.assetCount)}
          </p>
          <h2 id="project-detail-title">{project.title}</h2>
          <p>{project.description || "—"}</p>
        </div>
        {!readOnly ? (
          <button className="secondary-button" type="button" onClick={onEdit}>
            {text.library.project.edit}
          </button>
        ) : null}
      </header>
      {project.notes ? (
        <p className="project-detail-notes">{project.notes}</p>
      ) : null}
      <div className="project-detail-divider">
        <span>{text.library.project.includedWorks}</span>
        <strong>{assets.totalCount}</strong>
      </div>
      {project.kind === "canvas" ? (
        <Suspense
          fallback={<p className="route-loading">{text.shell.loading}</p>}
        >
          <CanvasProjectDetail
            project={project}
            service={service}
            readOnly={readOnly}
            selectedIds={selectedIds}
            onToggleSelection={onToggleSelection}
            onOpenAsset={onOpenAsset}
            onRemoveSelected={onRemoveSelected}
          />
        </Suspense>
      ) : null}
      {project.kind === "simple" && !readOnly && assets.items.length > 0 ? (
        <div className="asset-selection-bar project-selection-bar">
          <label className="check-field">
            <input
              type="checkbox"
              checked={
                assets.items.length > 0 &&
                assets.items.every((asset) => selectedIds.has(asset.id))
              }
              onChange={() => {
                const allSelected = assets.items.every((asset) =>
                  selectedIds.has(asset.id),
                );
                for (const asset of assets.items) {
                  if (allSelected === selectedIds.has(asset.id)) {
                    onToggleSelection(asset.id);
                  }
                }
              }}
            />
            {text.library.project.selectPage}
          </label>
          <span>{text.library.project.selectedCount(selectedIds.size)}</span>
          <button
            type="button"
            className="danger-button"
            aria-label={text.library.project.removeSelectedLabel}
            disabled={selectedIds.size === 0 || loading}
            onClick={() => void onRemoveSelected()}
          >
            {text.library.project.removeSelected}
          </button>
        </div>
      ) : null}
      {project.kind === "simple" && !loading && assets.items.length === 0 ? (
        <p className="management-empty">
          {text.library.project.includedWorksEmpty}
        </p>
      ) : project.kind === "simple" ? (
        <AssetGrid
          assets={assets.items}
          busy={loading}
          service={service}
          page={assets.page}
          pageSize={assets.pageSize}
          totalCount={assets.totalCount}
          totalPages={assets.totalPages}
          selectedIds={selectedIds}
          onToggleSelection={onToggleSelection}
          selectable={!readOnly}
          onOpen={onOpenAsset}
          onPageChange={onPageChange}
          onPageSizeChange={onPageSizeChange}
        />
      ) : null}
    </section>
  );
}

function ProjectGrid({
  projects,
  onOpen,
  onEdit,
  onDelete,
  selectionMode,
  selectedIds,
  onToggleSelection,
  nextCursor,
  hasPrevious,
  loading,
  onNext,
  onPrevious,
  readOnly,
}: {
  readonly projects: readonly ProjectSummary[];
  readonly onOpen: (project: ProjectSummary) => void;
  readonly onEdit: (project: ProjectSummary) => void;
  readonly onDelete: (project: ProjectSummary) => void;
  readonly selectionMode: boolean;
  readonly selectedIds: ReadonlySet<string>;
  readonly onToggleSelection: (projectId: string) => void;
  readonly nextCursor: string | null;
  readonly hasPrevious: boolean;
  readonly loading: boolean;
  readonly onNext: () => void;
  readonly onPrevious: () => void;
  readonly readOnly: boolean;
}) {
  if (projects.length === 0)
    return (
      <>
        <ProjectIntroduction />
        <EmptyState
          title={text.library.project.empty}
          description={text.library.project.emptyHint}
        />
      </>
    );
  return (
    <>
      <ProjectIntroduction />
      <section className="project-grid">
        {projects.map((project) => (
          <article
            className={`project-card${selectedIds.has(project.id) ? " is-selected" : ""}`}
            key={project.id}
          >
            {selectionMode ? (
              <label className="project-select-field">
                <input
                  type="checkbox"
                  checked={selectedIds.has(project.id)}
                  aria-label={`${text.library.export.selectThisProject}：${project.title}`}
                  onChange={() => onToggleSelection(project.id)}
                />
                {text.library.export.selectThisProject}
              </label>
            ) : null}
            <button
              className="project-card-open"
              type="button"
              aria-label={`${text.library.project.open}：${project.title}`}
              onClick={() => onOpen(project)}
            >
              <span className="project-kind-badge">
                {project.kind === "canvas"
                  ? text.library.project.canvasKind
                  : text.library.project.simpleKind}
              </span>
              <h2>{project.title || text.library.projectUntitled}</h2>
              <p>{project.description}</p>
            </button>
            <small className="project-id">
              {text.library.project.idLabel} {project.id}
            </small>
            <footer>
              <span>{text.library.assetCount(project.assetCount)}</span>
              {!readOnly ? (
                <>
                  <button
                    className="text-button"
                    type="button"
                    onClick={() => onEdit(project)}
                  >
                    {text.library.project.edit}
                  </button>
                  <button
                    className="danger-text-button"
                    type="button"
                    onClick={() => onDelete(project)}
                  >
                    {text.library.project.delete}
                  </button>
                </>
              ) : null}
            </footer>
          </article>
        ))}
      </section>
      {hasPrevious || nextCursor ? (
        <div className="load-more">
          {hasPrevious ? (
            <button
              className="secondary-button"
              type="button"
              disabled={loading}
              onClick={onPrevious}
            >
              {text.library.previousPage}
            </button>
          ) : null}
          {nextCursor ? (
            <button
              className="secondary-button"
              type="button"
              disabled={loading}
              onClick={onNext}
            >
              {text.library.loadMore}
            </button>
          ) : null}
        </div>
      ) : null}
    </>
  );
}

function ProjectIntroduction() {
  return (
    <section className="project-introduction">
      <p className="eyebrow">{text.library.project.heading}</p>
      <p>{text.library.project.description}</p>
    </section>
  );
}

function EmptyState({
  title,
  description,
}: {
  readonly title: string;
  readonly description: string;
}) {
  return (
    <section className="library-empty">
      <BrandMark />
      <h2>{title}</h2>
      <p>{description}</p>
    </section>
  );
}

function ConfirmTrashDialog({
  entityType,
  count,
  onCancel,
  onConfirm,
}: {
  readonly entityType: "project" | "asset";
  readonly count: number;
  readonly onCancel: () => void;
  readonly onConfirm: () => void;
}) {
  return (
    <div className="dialog-backdrop" role="presentation">
      <section
        className="library-dialog confirm-dialog is-danger"
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="trash-confirm-title"
      >
        <p className="eyebrow">SAFE DELETE</p>
        <h2 id="trash-confirm-title">
          {entityType === "project"
            ? text.library.trash.confirmProjectTitle
            : count > 1
              ? `确认将 ${count} 件作品移入回收站？`
              : text.library.trash.confirmTitle}
        </h2>
        <p>
          {entityType === "project"
            ? text.library.trash.confirmProjectDescription
            : text.library.trash.confirmDescription}
        </p>
        <div className="dialog-actions">
          <button className="secondary-button" type="button" onClick={onCancel}>
            {text.library.form.cancel}
          </button>
          <button className="danger-button" type="button" onClick={onConfirm}>
            {text.library.trash.confirmMove}
          </button>
        </div>
      </section>
    </div>
  );
}
