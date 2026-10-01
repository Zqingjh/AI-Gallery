import { useCallback, useEffect, useState } from "react";
import { text } from "../../app/texts";
import { ThemeToggle } from "../../components/ThemeToggle";
import type {
  AiService,
  AiSuggestion,
  ResolveAiSuggestionInput,
} from "../../services/ai-service";
import type { WorkspaceAccessMode } from "../../services/workspace-management-service";

interface AiReviewPageProps {
  readonly service?: AiService;
  readonly accessMode: WorkspaceAccessMode;
  readonly onBack: () => void;
  readonly showThemeToggle?: boolean;
}

type ResolutionMode = "createCategory" | "convertToTag";

const pageSize = 24;

function confidence(value: number): string {
  return `${Math.round(value * 100)}%`;
}

function acceptResolution(item: AiSuggestion): ResolveAiSuggestionInput | null {
  return item.category || item.suggestedCategoryName?.trim()
    ? { kind: "accept" }
    : null;
}

export default function AiReviewPage({
  service,
  accessMode,
  onBack,
  showThemeToggle = true,
}: AiReviewPageProps) {
  const isReadOnly = accessMode === "readOnly";
  const [items, setItems] = useState<readonly AiSuggestion[]>([]);
  const [nextCursor, setNextCursor] = useState<string | null>(null);
  const [history, setHistory] = useState<readonly string[]>([]);
  const [cursor, setCursor] = useState<string | undefined>();
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [resolvingId, setResolvingId] = useState<string | null>(null);
  const [editing, setEditing] = useState<AiSuggestion | null>(null);
  const [mode, setMode] = useState<ResolutionMode>("createCategory");
  const [value, setValue] = useState("");

  const load = useCallback(
    async (next?: string, keepHistory = false) => {
      if (!service) {
        setError(text.ai.noWorkspace);
        return;
      }
      if (isReadOnly) {
        setError(text.ai.reviewReadOnly);
        return;
      }
      setLoading(true);
      setError(null);
      try {
        const page = await service.listPending({
          ...(next ? { cursor: next } : {}),
          limit: pageSize,
        });
        setItems(page.items);
        setNextCursor(page.nextCursor);
        setCursor(next);
        if (!keepHistory) setHistory([]);
      } catch {
        setError(text.ai.reviewLoadFailed);
      } finally {
        setLoading(false);
      }
    },
    [isReadOnly, service],
  );

  useEffect(() => {
    void load();
  }, [load]);

  async function resolve(item: AiSuggestion, input: ResolveAiSuggestionInput) {
    if (!service) return;
    setResolvingId(item.id);
    setError(null);
    try {
      await service.resolveSuggestion(item.id, input);
      setEditing(null);
      await load(cursor, true);
    } catch {
      setError(text.ai.operationFailed);
    } finally {
      setResolvingId(null);
    }
  }

  function openModify(item: AiSuggestion) {
    setEditing(item);
    setMode("createCategory");
    setValue(item.suggestedCategoryName ?? item.category?.name ?? "");
  }

  function submitModify(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!editing || !value.trim()) return;
    const resolution: ResolveAiSuggestionInput = {
      kind: mode,
      name: value.trim(),
    };
    void resolve(editing, resolution);
  }

  return (
    <main className="settings-page">
      <div className="settings-toolbar">
        <button className="secondary-button" type="button" onClick={onBack}>
          ← {text.settings.back}
        </button>
        {showThemeToggle ? <ThemeToggle /> : null}
      </div>
      <section className="settings-panel">
        <p className="eyebrow">{text.ai.reviewEyebrow}</p>
        <h1>{text.ai.reviewTitle}</h1>
        <p>{text.ai.reviewDescription}</p>
      </section>
      {isReadOnly ? (
        <p className="form-error" role="alert">
          {text.ai.reviewReadOnly}
        </p>
      ) : null}
      {error ? (
        <p className="form-error" role="alert">
          {error}
        </p>
      ) : null}
      {loading ? <p className="route-loading">{text.ai.loading}</p> : null}
      {!loading && items.length === 0 ? (
        <p className="management-empty">{text.ai.reviewEmpty}</p>
      ) : null}
      {!isReadOnly ? (
        <section className="management-panel" aria-label={text.ai.reviewTitle}>
          <div className="project-grid ai-review-grid">
            {items.map((item) => {
              const accepted = acceptResolution(item);
              return (
                <article className="project-card ai-review-card" key={item.id}>
                  <p className="eyebrow">
                    {item.target.type === "asset"
                      ? text.ai.asset
                      : text.ai.project}
                  </p>
                  <h2>{item.target.title}</h2>
                  <dl>
                    <div>
                      <dt>{text.ai.dimension}</dt>
                      <dd>{item.dimension.name}</dd>
                    </div>
                    <div>
                      <dt>
                        {item.category
                          ? text.ai.existingCategory
                          : text.ai.newCategory}
                      </dt>
                      <dd>
                        {item.category?.name ?? item.suggestedCategoryName}
                      </dd>
                    </div>
                    <div>
                      <dt>{text.ai.confidence}</dt>
                      <dd>{confidence(item.confidence)}</dd>
                    </div>
                    <div>
                      <dt>{text.ai.reason}</dt>
                      <dd>{item.reason}</dd>
                    </div>
                  </dl>
                  <footer className="ai-review-actions">
                    <button
                      className="primary-button"
                      type="button"
                      disabled={resolvingId === item.id || !accepted}
                      onClick={() => accepted && void resolve(item, accepted)}
                    >
                      {text.ai.accept}
                    </button>
                    <button
                      className="secondary-button"
                      type="button"
                      disabled={resolvingId === item.id}
                      onClick={() => void resolve(item, { kind: "reject" })}
                    >
                      {text.ai.reject}
                    </button>
                    <button
                      className="text-button"
                      type="button"
                      disabled={resolvingId === item.id}
                      onClick={() => openModify(item)}
                    >
                      {text.ai.modify}
                    </button>
                  </footer>
                </article>
              );
            })}
          </div>
          <div className="dialog-actions">
            <button
              className="secondary-button"
              type="button"
              disabled={history.length === 0 || loading}
              onClick={() => {
                const previous = history.at(-1);
                if (!previous) return;
                setHistory((entries) => entries.slice(0, -1));
                void load(previous, true);
              }}
            >
              {text.ai.previousPage}
            </button>
            <button
              className="secondary-button"
              type="button"
              disabled={!nextCursor || loading}
              onClick={() => {
                if (!nextCursor) return;
                setHistory((entries) => [...entries, cursor ?? ""]);
                void load(nextCursor, true);
              }}
            >
              {text.ai.nextPage}
            </button>
          </div>
        </section>
      ) : null}

      {!isReadOnly && editing ? (
        <div className="dialog-backdrop" role="presentation">
          <section
            className="library-dialog"
            role="dialog"
            aria-modal="true"
            aria-labelledby="ai-resolution-title"
          >
            <header className="dialog-header">
              <div>
                <p className="eyebrow">{text.ai.reviewEyebrow}</p>
                <h2 id="ai-resolution-title">{text.ai.resolveTitle}</h2>
                <p>{text.ai.resolveDescription}</p>
              </div>
            </header>
            <form className="library-form" onSubmit={submitModify}>
              <label className="form-field">
                <span>{text.ai.modify}</span>
                <select
                  value={mode}
                  onChange={(event) =>
                    setMode(event.target.value as ResolutionMode)
                  }
                >
                  <option value="createCategory">
                    {text.ai.createCategory}
                  </option>
                  <option value="convertToTag">{text.ai.convertToTag}</option>
                </select>
              </label>
              <label className="form-field">
                <span>{text.ai.categoryName}</span>
                <input
                  required
                  value={value}
                  onChange={(event) => setValue(event.target.value)}
                />
              </label>
              <div className="dialog-actions">
                <button
                  className="secondary-button"
                  type="button"
                  onClick={() => setEditing(null)}
                >
                  {text.ai.close}
                </button>
                <button
                  className="primary-button"
                  type="submit"
                  disabled={resolvingId === editing.id}
                >
                  {resolvingId === editing.id
                    ? text.ai.resolving
                    : text.ai.resolve}
                </button>
              </div>
            </form>
          </section>
        </div>
      ) : null}
    </main>
  );
}
