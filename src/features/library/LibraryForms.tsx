import { useEffect, useState } from "react";
import { text } from "../../app/texts";
import type {
  AssetDetail,
  AssetDraft,
  CategoryDimension,
  LibraryService,
  MetadataPresets,
  ProjectDetail,
  ProjectDraft,
  ProjectSummary,
  TagItem,
} from "../../services/library-service";
import { LibraryDialog } from "./LibraryDialog";

const emptyProject: ProjectDraft = {
  kind: "simple",
  title: "",
  description: "",
  rating: 0,
  isFavorite: false,
  isPublic: false,
  promptZh: "",
  promptEn: "",
  negativePrompt: "",
  notes: "",
  categoryIds: [],
  tagIds: [],
};

function projectDraft(project: ProjectDetail | null): ProjectDraft {
  return project
    ? {
        kind: project.kind ?? "simple",
        title: project.title,
        description: project.description,
        rating: project.rating,
        isFavorite: project.isFavorite,
        isPublic: project.isPublic,
        promptZh: project.promptZh,
        promptEn: project.promptEn,
        negativePrompt: project.negativePrompt,
        notes: project.notes,
        categoryIds: project.categoryIds,
        tagIds: project.tagIds,
      }
    : emptyProject;
}

function parseProjectAssetNumbers(value: string): readonly number[] | null {
  if (!value.trim()) return [];
  const tokens = value.split(/[,，]/).map((token) => token.trim());
  if (tokens.some((token) => !/^\d+$/.test(token))) return null;
  const numbers = [...new Set(tokens.map(Number))];
  if (
    numbers.length > 100 ||
    numbers.some((number) => !Number.isSafeInteger(number) || number <= 0)
  ) {
    return null;
  }
  return numbers;
}

export function ProjectFormDialog({
  service,
  project,
  onClose,
  onSaved,
}: {
  readonly service: LibraryService;
  readonly project: ProjectDetail | null;
  readonly onClose: () => void;
  readonly onSaved: (value: ProjectSummary) => void;
}) {
  const [draft, setDraft] = useState<ProjectDraft>(() => projectDraft(project));
  const [assetNumbers, setAssetNumbers] = useState("");
  const taxonomy = useTaxonomy(service);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  function requestClose() {
    if (
      (JSON.stringify(draft) !== JSON.stringify(projectDraft(project)) ||
        assetNumbers.trim()) &&
      !window.confirm(text.library.form.confirmDiscard)
    ) {
      return;
    }
    onClose();
  }
  async function submit(event: React.FormEvent) {
    event.preventDefault();
    if (!draft.title.trim()) return setError(text.library.form.required);
    const displayNumbers = parseProjectAssetNumbers(assetNumbers);
    if (displayNumbers === null) {
      return setError(text.library.form.invalidProjectAssets);
    }
    if (
      project &&
      displayNumbers.length > 0 &&
      !window.confirm(
        text.library.form.confirmProjectAssets(displayNumbers.length),
      )
    ) {
      return;
    }
    setSaving(true);
    try {
      const saved = project
        ? await service.updateProject(project.id, draft)
        : await service.createProject(draft);
      if (project && displayNumbers.length > 0) {
        try {
          await service.assignAssetsToProject(project.id, displayNumbers);
        } catch {
          setError(text.library.form.projectSavedAssetsFailed);
          setSaving(false);
          return;
        }
      }
      onSaved(saved);
    } catch {
      setError(text.library.loadFailed);
      setSaving(false);
    }
  }
  return (
    <LibraryDialog
      title={
        project
          ? text.library.form.editProject
          : text.library.form.createProject
      }
      onClose={requestClose}
    >
      <form className="library-form" onSubmit={(event) => void submit(event)}>
        <Field label={text.library.form.projectKind} wide>
          <select
            value={draft.kind}
            onChange={(event) =>
              setDraft({
                ...draft,
                kind: event.target.value as ProjectDraft["kind"],
              })
            }
          >
            <option value="simple">{text.library.project.simpleKind}</option>
            <option value="canvas">{text.library.project.canvasKind}</option>
          </select>
          <small>{text.library.form.projectKindHint}</small>
        </Field>
        <Field label={text.library.form.title}>
          <input
            value={draft.title}
            onChange={(event) =>
              setDraft({ ...draft, title: event.target.value })
            }
            autoFocus
          />
        </Field>
        <Field label={text.library.form.description}>
          <textarea
            value={draft.description}
            onChange={(event) =>
              setDraft({ ...draft, description: event.target.value })
            }
          />
        </Field>
        <Field label={text.library.form.promptZh}>
          <textarea
            value={draft.promptZh}
            onChange={(event) =>
              setDraft({ ...draft, promptZh: event.target.value })
            }
          />
        </Field>
        <Field label={text.library.form.promptEn}>
          <textarea
            value={draft.promptEn}
            onChange={(event) =>
              setDraft({ ...draft, promptEn: event.target.value })
            }
          />
        </Field>
        <Field label={text.library.form.negativePrompt} wide>
          <textarea
            value={draft.negativePrompt}
            onChange={(event) =>
              setDraft({ ...draft, negativePrompt: event.target.value })
            }
          />
        </Field>
        <Field label={text.library.form.notes} wide>
          <textarea
            value={draft.notes}
            onChange={(event) =>
              setDraft({ ...draft, notes: event.target.value })
            }
          />
        </Field>
        {project ? (
          <Field label={text.library.form.projectAssets} wide>
            <input
              value={assetNumbers}
              placeholder={text.library.form.projectAssetsPlaceholder}
              onChange={(event) => setAssetNumbers(event.target.value)}
            />
            <small>{text.library.form.projectAssetsHint}</small>
          </Field>
        ) : null}
        <TaxonomyFields
          dimensions={taxonomy.dimensions}
          tags={taxonomy.tags}
          categoryIds={draft.categoryIds}
          tagIds={draft.tagIds}
          onCategories={(categoryIds) => setDraft({ ...draft, categoryIds })}
          onTags={(tagIds) => setDraft({ ...draft, tagIds })}
        />
        <RatingAndFlags value={draft} onChange={setDraft} />
        {error ? (
          <p role="alert" className="form-error">
            {error}
          </p>
        ) : null}
        <FormActions saving={saving} onClose={requestClose} />
      </form>
    </LibraryDialog>
  );
}

function emptyAsset(): AssetDraft {
  return {
    title: "",
    projectId: null,
    modelName: "",
    platformName: "",
    promptZh: "",
    promptEn: "",
    negativePrompt: "",
    generationParamsJson: "",
    notes: "",
    rating: 0,
    isFavorite: false,
    isPublic: false,
    categoryIds: [],
    tagIds: [],
  };
}

interface PendingRecognition {
  readonly promptZh: string;
  readonly promptEn: string;
  readonly negativePrompt: string;
  readonly generationParams: Record<string, unknown>;
}

function readAssetRecognition(asset: AssetDetail | null): {
  readonly pending: PendingRecognition | null;
  readonly formalParamsJson: string;
} {
  if (!asset?.generationParamsJson) {
    return { pending: null, formalParamsJson: "" };
  }
  try {
    const parsed: unknown = JSON.parse(asset.generationParamsJson);
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
      return { pending: null, formalParamsJson: asset.generationParamsJson };
    }
    const params = { ...(parsed as Record<string, unknown>) };
    const value = params._pendingRecognition;
    delete params._pendingRecognition;
    const formalParamsJson = Object.keys(params).length
      ? JSON.stringify(params, null, 2)
      : "";
    if (!value || typeof value !== "object" || Array.isArray(value)) {
      return { pending: null, formalParamsJson };
    }
    const pending = value as Record<string, unknown>;
    const generationParams =
      pending.generationParams &&
      typeof pending.generationParams === "object" &&
      !Array.isArray(pending.generationParams)
        ? (pending.generationParams as Record<string, unknown>)
        : {};
    return {
      pending: {
        promptZh: typeof pending.promptZh === "string" ? pending.promptZh : "",
        promptEn: typeof pending.promptEn === "string" ? pending.promptEn : "",
        negativePrompt:
          typeof pending.negativePrompt === "string"
            ? pending.negativePrompt
            : "",
        generationParams,
      },
      formalParamsJson,
    };
  } catch {
    return { pending: null, formalParamsJson: asset.generationParamsJson };
  }
}

function parseFormalParams(value: string): Record<string, unknown> {
  if (!value.trim()) return {};
  const parsed: unknown = JSON.parse(value);
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
    throw new Error("生成参数必须是 JSON 对象");
  }
  return parsed as Record<string, unknown>;
}

function assetDraft(asset: AssetDetail | null): AssetDraft {
  const recognition = readAssetRecognition(asset);
  return asset
    ? {
        title: asset.title,
        projectId: asset.projectId,
        modelName: asset.modelName,
        platformName: asset.platformName,
        promptZh: asset.promptZh,
        promptEn: asset.promptEn,
        negativePrompt: asset.negativePrompt,
        generationParamsJson: recognition.formalParamsJson,
        notes: asset.notes,
        rating: asset.rating,
        isFavorite: asset.isFavorite,
        isPublic: asset.isPublic,
        categoryIds: asset.categoryIds,
        tagIds: asset.tagIds,
      }
    : emptyAsset();
}

export function AssetFormDialog({
  service,
  asset,
  projects,
  onClose,
  onSaved,
}: {
  readonly service: LibraryService;
  readonly asset: AssetDetail | null;
  readonly projects: readonly ProjectSummary[];
  readonly onClose: () => void;
  readonly onSaved: (value: AssetDetail) => void;
}) {
  const [draft, setDraft] = useState<AssetDraft>(() => assetDraft(asset));
  const [pendingRecognition] = useState<PendingRecognition | null>(
    () => readAssetRecognition(asset).pending,
  );
  const [recognitionHandled, setRecognitionHandled] = useState(false);
  const taxonomy = useTaxonomy(service);
  const metadataPresets = useMetadataPresets(service);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  function requestClose() {
    if (
      (JSON.stringify(draft) !== JSON.stringify(assetDraft(asset)) ||
        recognitionHandled) &&
      !window.confirm(text.library.form.confirmDiscard)
    ) {
      return;
    }
    onClose();
  }
  async function submit(event: React.FormEvent) {
    event.preventDefault();
    if (!draft.title.trim()) return setError(text.library.form.required);
    setSaving(true);
    try {
      const draftToSave =
        pendingRecognition && !recognitionHandled
          ? {
              ...draft,
              generationParamsJson: JSON.stringify({
                ...parseFormalParams(draft.generationParamsJson),
                _pendingRecognition: pendingRecognition,
              }),
            }
          : draft;
      onSaved(
        asset
          ? await service.updateAsset(asset.id, draftToSave)
          : await service.createAsset(draftToSave),
      );
    } catch {
      setError(text.library.loadFailed);
      setSaving(false);
    }
  }
  function acceptRecognition() {
    if (!pendingRecognition) return;
    try {
      const formalParams = parseFormalParams(draft.generationParamsJson);
      setDraft({
        ...draft,
        promptZh: draft.promptZh || pendingRecognition.promptZh,
        promptEn: draft.promptEn || pendingRecognition.promptEn,
        negativePrompt:
          draft.negativePrompt || pendingRecognition.negativePrompt,
        generationParamsJson: JSON.stringify(
          { ...pendingRecognition.generationParams, ...formalParams },
          null,
          2,
        ),
      });
      setRecognitionHandled(true);
    } catch {
      setError(text.library.form.invalidGenerationParams);
    }
  }
  return (
    <LibraryDialog
      title={
        asset ? text.library.form.editAsset : text.library.form.createAsset
      }
      onClose={requestClose}
    >
      <form
        className="library-form two-column-form"
        onSubmit={(event) => void submit(event)}
      >
        {pendingRecognition && !recognitionHandled ? (
          <section className="recognition-review form-wide" aria-live="polite">
            <h3>{text.library.form.recognitionTitle}</h3>
            <p>{text.library.form.recognitionDescription}</p>
            <pre>
              {pendingRecognition.promptZh ||
                pendingRecognition.promptEn ||
                JSON.stringify(pendingRecognition.generationParams, null, 2)}
            </pre>
            <div className="recognition-actions">
              <button type="button" onClick={acceptRecognition}>
                {text.library.form.recognitionAccept}
              </button>
              <button
                type="button"
                className="ghost-button"
                onClick={() => setRecognitionHandled(true)}
              >
                {text.library.form.recognitionReject}
              </button>
            </div>
          </section>
        ) : null}
        <Field label={text.library.form.title}>
          <input
            value={draft.title}
            onChange={(event) =>
              setDraft({ ...draft, title: event.target.value })
            }
            autoFocus
          />
        </Field>
        <Field label={text.library.form.project}>
          <select
            value={draft.projectId ?? ""}
            onChange={(event) =>
              setDraft({ ...draft, projectId: event.target.value || null })
            }
          >
            <option value="">{text.library.form.independent}</option>
            {projects.map((project) => (
              <option key={project.id} value={project.id}>
                {project.title}
              </option>
            ))}
          </select>
        </Field>
        <Field label={text.library.form.model}>
          <input
            list="asset-model-presets"
            value={draft.modelName}
            onChange={(event) =>
              setDraft({ ...draft, modelName: event.target.value })
            }
          />
          <datalist id="asset-model-presets">
            {metadataPresets.models.map((preset) => (
              <option key={preset.id} value={preset.name} />
            ))}
          </datalist>
        </Field>
        <Field label={text.library.form.platform}>
          <input
            list="asset-platform-presets"
            value={draft.platformName}
            onChange={(event) =>
              setDraft({ ...draft, platformName: event.target.value })
            }
          />
          <datalist id="asset-platform-presets">
            {metadataPresets.platforms.map((preset) => (
              <option key={preset.id} value={preset.name} />
            ))}
          </datalist>
        </Field>
        <Field label={text.library.form.promptZh} wide>
          <textarea
            value={draft.promptZh}
            onChange={(event) =>
              setDraft({ ...draft, promptZh: event.target.value })
            }
          />
        </Field>
        <Field label={text.library.form.promptEn} wide>
          <textarea
            value={draft.promptEn}
            onChange={(event) =>
              setDraft({ ...draft, promptEn: event.target.value })
            }
          />
        </Field>
        <Field label={text.library.form.negativePrompt} wide>
          <textarea
            value={draft.negativePrompt}
            onChange={(event) =>
              setDraft({ ...draft, negativePrompt: event.target.value })
            }
          />
        </Field>
        <Field label={text.library.form.generationParams} wide>
          <textarea
            className="code-input"
            value={draft.generationParamsJson}
            onChange={(event) =>
              setDraft({ ...draft, generationParamsJson: event.target.value })
            }
          />
        </Field>
        <Field label={text.library.form.notes} wide>
          <textarea
            value={draft.notes}
            onChange={(event) =>
              setDraft({ ...draft, notes: event.target.value })
            }
          />
        </Field>
        <div className="form-wide">
          <RatingAndFlags value={draft} onChange={setDraft} />
        </div>
        <TaxonomyFields
          dimensions={taxonomy.dimensions}
          tags={taxonomy.tags}
          categoryIds={draft.categoryIds}
          tagIds={draft.tagIds}
          onCategories={(categoryIds) => setDraft({ ...draft, categoryIds })}
          onTags={(tagIds) => setDraft({ ...draft, tagIds })}
        />
        {error ? (
          <p role="alert" className="form-error form-wide">
            {error}
          </p>
        ) : null}
        <div className="form-wide">
          <FormActions saving={saving} onClose={requestClose} />
        </div>
      </form>
    </LibraryDialog>
  );
}

function useTaxonomy(service: LibraryService): {
  readonly dimensions: readonly CategoryDimension[];
  readonly tags: readonly TagItem[];
} {
  const [taxonomy, setTaxonomy] = useState<{
    readonly dimensions: readonly CategoryDimension[];
    readonly tags: readonly TagItem[];
  }>({ dimensions: [], tags: [] });
  useEffect(() => {
    let active = true;
    void service.listTaxonomy().then((value) => {
      if (active) setTaxonomy(value);
    });
    return () => {
      active = false;
    };
  }, [service]);
  return taxonomy;
}

function useMetadataPresets(service: LibraryService): MetadataPresets {
  const [presets, setPresets] = useState<MetadataPresets>({
    models: [],
    platforms: [],
  });
  useEffect(() => {
    let active = true;
    void service
      .listMetadataPresets()
      .then((value) => {
        if (active) setPresets(value);
      })
      .catch(() => {
        // 预设加载失败时仍允许用户手动输入模型或平台。
      });
    return () => {
      active = false;
    };
  }, [service]);
  return presets;
}

function TaxonomyFields({
  dimensions,
  tags,
  categoryIds,
  tagIds,
  onCategories,
  onTags,
}: {
  readonly dimensions: readonly CategoryDimension[];
  readonly tags: readonly TagItem[];
  readonly categoryIds: readonly string[];
  readonly tagIds: readonly string[];
  readonly onCategories: (ids: readonly string[]) => void;
  readonly onTags: (ids: readonly string[]) => void;
}) {
  function toggle(
    ids: readonly string[],
    id: string,
    checked: boolean,
  ): readonly string[] {
    return checked ? [...ids, id] : ids.filter((value) => value !== id);
  }
  return (
    <fieldset className="form-wide taxonomy-fieldset">
      <legend>{text.library.detail.taxonomy}</legend>
      {dimensions.map((dimension) => (
        <div key={dimension.id}>
          <strong>{dimension.name}</strong>
          <div className="form-check-list">
            {dimension.categories.map((category) => (
              <label className="check-field" key={category.id}>
                <input
                  type="checkbox"
                  checked={categoryIds.includes(category.id)}
                  onChange={(event) =>
                    onCategories(
                      toggle(categoryIds, category.id, event.target.checked),
                    )
                  }
                />
                {category.name}
              </label>
            ))}
          </div>
        </div>
      ))}
      {tags.length ? (
        <div>
          <strong>{text.library.taxonomy.tags}</strong>
          <div className="form-check-list">
            {tags.map((tag) => (
              <label className="check-field" key={tag.id}>
                <input
                  type="checkbox"
                  checked={tagIds.includes(tag.id)}
                  onChange={(event) =>
                    onTags(toggle(tagIds, tag.id, event.target.checked))
                  }
                />
                #{tag.name}
              </label>
            ))}
          </div>
        </div>
      ) : null}
    </fieldset>
  );
}

function Field({
  label,
  children,
  wide = false,
}: {
  readonly label: string;
  readonly children: React.ReactNode;
  readonly wide?: boolean;
}) {
  return (
    <label className={`form-field${wide ? " form-wide" : ""}`}>
      <span>{label}</span>
      {children}
    </label>
  );
}

type Flags = {
  readonly rating: number;
  readonly isFavorite: boolean;
  readonly isPublic: boolean;
};
function RatingAndFlags<T extends Flags>({
  value,
  onChange,
}: {
  readonly value: T;
  readonly onChange: (value: T) => void;
}) {
  return (
    <div className="form-flags">
      <label>
        <span>{text.library.form.rating}</span>
        <select
          value={value.rating}
          onChange={(event) =>
            onChange({ ...value, rating: Number(event.target.value) })
          }
        >
          {[0, 1, 2, 3, 4, 5].map((rating) => (
            <option key={rating} value={rating}>
              {rating || "—"}
            </option>
          ))}
        </select>
      </label>
      <label className="check-field">
        <input
          type="checkbox"
          checked={value.isFavorite}
          onChange={(event) =>
            onChange({ ...value, isFavorite: event.target.checked })
          }
        />
        {text.library.form.favorite}
      </label>
      <label className="check-field">
        <input
          type="checkbox"
          checked={value.isPublic}
          onChange={(event) =>
            onChange({ ...value, isPublic: event.target.checked })
          }
        />
        {text.library.form.public}
      </label>
    </div>
  );
}

function FormActions({
  saving,
  onClose,
}: {
  readonly saving: boolean;
  readonly onClose: () => void;
}) {
  return (
    <div className="dialog-actions">
      <button className="secondary-button" type="button" onClick={onClose}>
        {text.library.form.cancel}
      </button>
      <button className="primary-button" type="submit" disabled={saving}>
        {saving ? text.library.form.saving : text.library.form.save}
      </button>
    </div>
  );
}
