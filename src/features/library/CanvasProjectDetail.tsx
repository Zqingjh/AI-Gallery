import { useEffect, useRef, useState } from "react";
import type {
  AssetSummary,
  CanvasOutputPrompt,
  CanvasProjectMember,
  LibraryService,
  PageResult,
  ProjectDetail,
} from "../../services/library-service";
import { AssetThumbnailPreview } from "./AssetCardGrid";

const canvasPageSize = 25;
type CanvasLaneRole = "reference" | "output";

function memberAsset(member: CanvasProjectMember): AssetSummary {
  return {
    id: member.assetId,
    displayOrder: member.displayOrder,
    projectId: null,
    fileName: member.fileName,
    mediaType: member.mediaType,
    thumbnailUrl: null,
    coverUrl: null,
    modelName: member.modelName,
    platformName: member.platformName,
    rating: 0,
    isFavorite: false,
    isPublic: false,
    width: member.width,
    height: member.height,
    durationMs: member.durationMs,
    updatedAt: member.updatedAt,
  };
}

function useCanvasLane(
  projectId: string,
  role: CanvasLaneRole,
  service: LibraryService,
) {
  const [page, setPage] = useState<PageResult<CanvasProjectMember>>({
    items: [],
    nextCursor: null,
  });
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [currentCursor, setCurrentCursor] = useState<string | undefined>();
  const [cursorHistory, setCursorHistory] = useState<
    readonly (string | undefined)[]
  >([]);
  const requestVersion = useRef(0);

  async function load(cursor?: string) {
    const version = requestVersion.current + 1;
    requestVersion.current = version;
    setLoading(true);
    setError(null);
    try {
      const nextPage = await service.listCanvasMembers(projectId, {
        ...(cursor ? { cursor } : {}),
        limit: canvasPageSize,
        role,
      });
      if (requestVersion.current === version) setPage(nextPage);
    } catch {
      if (requestVersion.current === version) {
        setError(
          role === "reference"
            ? "无法读取画布参考图。"
            : "无法读取画布最终输出。",
        );
      }
    } finally {
      if (requestVersion.current === version) setLoading(false);
    }
  }

  useEffect(() => {
    setCurrentCursor(undefined);
    setCursorHistory([]);
    void load();
    // service 由工作区上下文绑定，项目或成员角色变化时重新读取首页。
    return () => {
      requestVersion.current += 1;
    };
  }, [projectId, role, service]);

  async function reloadFirstPage() {
    setCurrentCursor(undefined);
    setCursorHistory([]);
    await load();
  }

  function loadNextPage() {
    if (!page.nextCursor) return;
    setCursorHistory((current) => [...current, currentCursor]);
    setCurrentCursor(page.nextCursor);
    void load(page.nextCursor);
  }

  function loadPreviousPage() {
    const previousCursor = cursorHistory.at(-1);
    setCursorHistory((current) => current.slice(0, -1));
    setCurrentCursor(previousCursor);
    void load(previousCursor);
  }

  return {
    page,
    loading,
    error,
    cursorHistory,
    reloadFirstPage,
    loadNextPage,
    loadPreviousPage,
  } as const;
}

export function CanvasProjectDetail({
  project,
  service,
  readOnly,
  selectedIds,
  onToggleSelection,
  onOpenAsset,
  onRemoveSelected,
}: {
  readonly project: ProjectDetail;
  readonly service: LibraryService;
  readonly readOnly: boolean;
  readonly selectedIds: ReadonlySet<string>;
  readonly onToggleSelection: (assetId: string) => void;
  readonly onOpenAsset: (asset: AssetSummary) => void;
  readonly onRemoveSelected: () => Promise<void>;
}) {
  const referenceLane = useCanvasLane(project.id, "reference", service);
  const outputLane = useCanvasLane(project.id, "output", service);
  const [mutationError, setMutationError] = useState<string | null>(null);
  const references = referenceLane.page.items.filter(
    (member) => member.referenceName,
  );
  const outputs = outputLane.page.items;
  const loading = referenceLane.loading || outputLane.loading;
  const error = mutationError || referenceLane.error || outputLane.error;

  async function reloadLanes() {
    await Promise.all([
      referenceLane.reloadFirstPage(),
      outputLane.reloadFirstPage(),
    ]);
  }

  return (
    <section className="canvas-project" aria-label="画布项目关系">
      {!readOnly && (references.length > 0 || outputs.length > 0) ? (
        <div className="asset-selection-bar project-selection-bar">
          <span>已选择 {selectedIds.size} 件作品</span>
          <button
            type="button"
            className="danger-button"
            disabled={selectedIds.size === 0 || loading}
            onClick={() => void onRemoveSelected().then(reloadLanes)}
          >
            移出项目
          </button>
        </div>
      ) : null}
      {error ? (
        <p className="form-error" role="alert">
          {error}
        </p>
      ) : null}
      {!loading && references.length === 0 && outputs.length === 0 ? (
        <p className="management-empty">这个画布项目还没有归入作品。</p>
      ) : (
        <div className="canvas-relationship-layout">
          <section
            className="canvas-lane"
            aria-labelledby="canvas-reference-title"
          >
            <header>
              <span>01</span>
              <h3 id="canvas-reference-title">参考图片</h3>
              <small>用 @引用名称组成输出提示词</small>
            </header>
            <div className="canvas-member-list">
              {references.map((member) => (
                <CanvasMemberCard
                  key={member.assetId}
                  member={member}
                  allReferences={references}
                  service={service}
                  projectId={project.id}
                  readOnly={readOnly}
                  selected={selectedIds.has(member.assetId)}
                  onToggleSelection={onToggleSelection}
                  onOpenAsset={onOpenAsset}
                  onChanged={reloadLanes}
                  onError={setMutationError}
                />
              ))}
              {references.length === 0 ? (
                <p className="canvas-lane-empty">暂无参考图片</p>
              ) : null}
            </div>
            <CanvasLanePagination lane={referenceLane} label="参考图" />
          </section>
          <div className="canvas-relation-arrow" aria-hidden="true">
            <span>→</span>
          </div>
          <section
            className="canvas-lane"
            aria-labelledby="canvas-output-title"
          >
            <header>
              <span>02</span>
              <h3 id="canvas-output-title">最终输出</h3>
              <small>一个或多个项目交付作品</small>
            </header>
            <div className="canvas-member-list">
              {outputs.map((member) => (
                <CanvasMemberCard
                  key={member.assetId}
                  member={member}
                  allReferences={references}
                  service={service}
                  projectId={project.id}
                  readOnly={readOnly}
                  selected={selectedIds.has(member.assetId)}
                  onToggleSelection={onToggleSelection}
                  onOpenAsset={onOpenAsset}
                  onChanged={reloadLanes}
                  onError={setMutationError}
                />
              ))}
              {outputs.length === 0 ? (
                <p className="canvas-lane-empty">暂无最终输出</p>
              ) : null}
            </div>
            <CanvasLanePagination lane={outputLane} label="最终输出" />
          </section>
        </div>
      )}
    </section>
  );
}

function CanvasLanePagination({
  lane,
  label,
}: {
  readonly lane: ReturnType<typeof useCanvasLane>;
  readonly label: string;
}) {
  if (lane.cursorHistory.length === 0 && !lane.page.nextCursor) return null;
  return (
    <nav className="canvas-load-more" aria-label={`${label}分页`}>
      <button
        className="secondary-button"
        type="button"
        disabled={lane.loading || lane.cursorHistory.length === 0}
        onClick={lane.loadPreviousPage}
      >
        上一页
      </button>
      <span>每页最多 {canvasPageSize} 件</span>
      <button
        className="secondary-button"
        type="button"
        disabled={lane.loading || !lane.page.nextCursor}
        onClick={lane.loadNextPage}
      >
        {lane.loading ? "正在加载…" : "下一页"}
      </button>
    </nav>
  );
}

function CanvasMemberCard({
  member,
  allReferences,
  service,
  projectId,
  readOnly,
  selected,
  onToggleSelection,
  onOpenAsset,
  onChanged,
  onError,
}: {
  readonly member: CanvasProjectMember;
  readonly allReferences: readonly CanvasProjectMember[];
  readonly service: LibraryService;
  readonly projectId: string;
  readonly readOnly: boolean;
  readonly selected: boolean;
  readonly onToggleSelection: (assetId: string) => void;
  readonly onOpenAsset: (asset: AssetSummary) => void;
  readonly onChanged: () => Promise<void>;
  readonly onError: (message: string | null) => void;
}) {
  const asset = memberAsset(member);
  const [referenceName, setReferenceName] = useState(
    member.referenceName ?? "",
  );
  const [savingRole, setSavingRole] = useState(false);

  useEffect(() => setReferenceName(member.referenceName ?? ""), [member]);

  async function makeReference() {
    const name = referenceName.trim().replace(/^@+/, "");
    if (member.mediaType !== "image") {
      onError("只有图片可以设为参考图，视频只能作为最终输出。");
      return;
    }
    if (!name) {
      onError("请为参考图填写引用名称。");
      return;
    }
    if (
      allReferences.some(
        (reference) =>
          reference.assetId !== member.assetId &&
          reference.referenceName?.toLocaleLowerCase() ===
            name.toLocaleLowerCase(),
      )
    ) {
      onError(`引用名称 @${name} 已存在，请更换名称。`);
      return;
    }
    setSavingRole(true);
    onError(null);
    try {
      await service.setCanvasMember(
        projectId,
        member.assetId,
        "reference",
        name,
      );
      await onChanged();
    } catch {
      onError("无法保存参考图引用名称，请确认名称在项目内唯一。");
    } finally {
      setSavingRole(false);
    }
  }

  async function makeOutput() {
    setSavingRole(true);
    onError(null);
    try {
      await service.setCanvasMember(projectId, member.assetId, "output", null);
      await onChanged();
    } catch {
      onError("无法更改画布成员角色。");
    } finally {
      setSavingRole(false);
    }
  }

  return (
    <article className="canvas-member-card">
      {!readOnly ? (
        <label className="canvas-member-select">
          <input
            type="checkbox"
            checked={selected}
            onChange={() => onToggleSelection(member.assetId)}
            aria-label={`选择画布作品：${member.fileName}`}
          />
        </label>
      ) : null}
      <button
        className="canvas-member-preview"
        type="button"
        onClick={() => onOpenAsset(asset)}
        aria-label={`打开作品：${member.fileName}`}
      >
        <AssetThumbnailPreview asset={asset} service={service} />
        <span>
          <strong>
            #{member.displayOrder} · {member.fileName}
          </strong>
          <small>{member.modelName || "未填写模型"}</small>
        </span>
      </button>
      {member.role === "reference" && member.referenceName ? (
        <strong className="canvas-reference-name">
          @{member.referenceName}
        </strong>
      ) : null}
      {!readOnly ? (
        <div className="canvas-role-editor">
          {member.mediaType === "image" ? (
            <>
              <label>
                <span>引用名称</span>
                <div className="canvas-reference-input">
                  <b>@</b>
                  <input
                    value={referenceName}
                    onChange={(event) => setReferenceName(event.target.value)}
                    aria-label={`引用名称：${member.fileName}`}
                  />
                </div>
              </label>
              <button
                className="secondary-button"
                type="button"
                disabled={savingRole}
                onClick={() => void makeReference()}
              >
                {member.role === "reference" ? "保存引用名" : "设为参考图"}
              </button>
            </>
          ) : (
            <small>视频不能设为参考图</small>
          )}
          {member.role === "reference" ? (
            <button
              className="text-button"
              type="button"
              disabled={savingRole}
              onClick={() => void makeOutput()}
            >
              设为最终输出
            </button>
          ) : null}
        </div>
      ) : null}
      {member.role === "output" ? (
        <CanvasPromptEditor
          member={member}
          references={allReferences}
          service={service}
          projectId={projectId}
          readOnly={readOnly}
          onChanged={onChanged}
          onError={onError}
        />
      ) : null}
    </article>
  );
}

function CanvasPromptEditor({
  member,
  references,
  service,
  projectId,
  readOnly,
  onChanged,
  onError,
}: {
  readonly member: CanvasProjectMember;
  readonly references: readonly CanvasProjectMember[];
  readonly service: LibraryService;
  readonly projectId: string;
  readonly readOnly: boolean;
  readonly onChanged: () => Promise<void>;
  readonly onError: (message: string | null) => void;
}) {
  const initialPrompt = (): CanvasOutputPrompt => ({
    promptZh: member.promptZh,
    promptEn: member.promptEn,
    negativePrompt: member.negativePrompt,
  });
  const [prompt, setPrompt] = useState<CanvasOutputPrompt>(initialPrompt);
  const [saving, setSaving] = useState(false);

  useEffect(() => setPrompt(initialPrompt()), [member]);

  function insertReference(field: keyof CanvasOutputPrompt, name: string) {
    const token = `@${name}`;
    setPrompt((current) => ({
      ...current,
      [field]: current[field].trim() ? `${current[field]} ${token}` : token,
    }));
  }

  async function savePrompt() {
    setSaving(true);
    onError(null);
    try {
      await service.updateCanvasOutputPrompt(projectId, member.assetId, prompt);
      await onChanged();
    } catch {
      onError("无法保存最终输出提示词。");
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="canvas-prompt-editor">
      {(
        [
          ["promptZh", "中文提示词"],
          ["promptEn", "英文提示词"],
          ["negativePrompt", "负面提示词"],
        ] as const
      ).map(([field, label]) => (
        <label key={field}>
          <span>{label}</span>
          {!readOnly && references.length > 0 ? (
            <span className="canvas-reference-buttons">
              {references.map((reference) => (
                <button
                  type="button"
                  key={reference.assetId}
                  onClick={() =>
                    insertReference(field, reference.referenceName ?? "")
                  }
                >
                  @{reference.referenceName}
                </button>
              ))}
            </span>
          ) : null}
          {readOnly ? (
            <p>{prompt[field] || "—"}</p>
          ) : (
            <textarea
              value={prompt[field]}
              onChange={(event) =>
                setPrompt({ ...prompt, [field]: event.target.value })
              }
            />
          )}
        </label>
      ))}
      {!readOnly ? (
        <button
          className="primary-button"
          type="button"
          disabled={saving}
          onClick={() => void savePrompt()}
        >
          {saving ? "正在保存…" : "保存输出提示词"}
        </button>
      ) : null}
    </div>
  );
}
