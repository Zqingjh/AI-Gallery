import { useEffect, useState } from "react";
import { text } from "../../app/texts";
import type {
  ImportMode,
  ImportTask,
  LibraryService,
  ProjectSummary,
} from "../../services/library-service";
import { LibraryDialog } from "./LibraryDialog";

export function ImportDialog({
  service,
  projects,
  currentTask,
  onTaskChange,
  onClose,
}: {
  readonly service: LibraryService;
  readonly projects: readonly ProjectSummary[];
  readonly currentTask: ImportTask | null;
  readonly onTaskChange: (task: ImportTask | null) => void;
  readonly onClose: () => void;
}) {
  const [paths, setPaths] = useState("");
  const [mode, setMode] = useState<ImportMode>("copy");
  const [projectId, setProjectId] = useState("");
  const [error, setError] = useState<string | null>(null);
  const taskId = currentTask?.id;
  const taskState = currentTask?.state;
  useEffect(() => {
    if (
      !taskId ||
      !taskState ||
      ["completed", "cancelled", "failed"].includes(taskState)
    )
      return;
    return service.subscribeImport(taskId, onTaskChange);
  }, [onTaskChange, service, taskId, taskState]);
  async function start(event: React.FormEvent) {
    event.preventDefault();
    const sourcePaths = paths
      .split(/\r?\n/u)
      .map((item) => item.trim())
      .filter(Boolean);
    if (!sourcePaths.length) return setError(text.library.import.empty);
    setError(null);
    try {
      onTaskChange(
        await service.startImport({
          sourcePaths,
          mode,
          projectId: projectId || null,
        }),
      );
    } catch {
      setError(text.library.loadFailed);
    }
  }
  function appendPaths(selected: readonly string[]) {
    if (selected.length === 0) return;
    setPaths((current) =>
      [current.trim(), ...selected].filter(Boolean).join("\n"),
    );
  }
  function acceptDrop(event: React.DragEvent<HTMLFormElement>) {
    event.preventDefault();
    const dropped = Array.from(event.dataTransfer.files)
      .map((file) => (file as File & { readonly path?: string }).path ?? "")
      .filter(Boolean);
    appendPaths(dropped);
  }
  async function selectFiles() {
    try {
      appendPaths(await service.selectImportFiles());
    } catch {
      setError(text.library.loadFailed);
    }
  }
  async function selectDirectory() {
    try {
      const selected = await service.selectImportDirectory();
      if (selected) appendPaths([selected]);
    } catch {
      setError(text.library.loadFailed);
    }
  }
  const progress =
    currentTask && currentTask.total > 0
      ? Math.round((currentTask.completed / currentTask.total) * 100)
      : 0;
  return (
    <LibraryDialog
      title={text.library.import.title}
      description={text.library.import.description}
      onClose={onClose}
    >
      <form
        className="library-form"
        onSubmit={(event) => void start(event)}
        onDragOver={(event) => event.preventDefault()}
        onDrop={acceptDrop}
      >
        <label className="form-field">
          <span>{text.library.import.sourcePaths}</span>
          <textarea
            value={paths}
            placeholder={text.library.import.sourcePlaceholder}
            onChange={(event) => setPaths(event.target.value)}
            disabled={Boolean(
              currentTask && ["queued", "running"].includes(currentTask.state),
            )}
          />
          <span className="import-source-actions">
            <button
              className="secondary-button"
              type="button"
              onClick={() => void selectFiles()}
            >
              {text.library.import.selectFiles}
            </button>
            <button
              className="secondary-button"
              type="button"
              onClick={() => void selectDirectory()}
            >
              {text.library.import.selectDirectory}
            </button>
          </span>
        </label>
        <label className="form-field">
          <span>{text.library.import.mode}</span>
          <select
            value={mode}
            onChange={(event) => setMode(event.target.value as ImportMode)}
          >
            <option value="copy">{text.library.import.copy}</option>
            <option value="reference">{text.library.import.reference}</option>
          </select>
        </label>
        <label className="form-field">
          <span>{text.library.import.targetProject}</span>
          <select
            value={projectId}
            onChange={(event) => setProjectId(event.target.value)}
          >
            <option value="">{text.library.form.independent}</option>
            {projects.map((project) => (
              <option key={project.id} value={project.id}>
                {project.title}
              </option>
            ))}
          </select>
        </label>
        {error ? (
          <p className="form-error" role="alert">
            {error}
          </p>
        ) : null}
        {currentTask ? (
          <section
            className="import-progress"
            aria-label={text.library.import.progress}
          >
            <div>
              <strong>{text.library.import[currentTask.state]}</strong>
              <span>
                {currentTask.completed} / {currentTask.total}
              </span>
            </div>
            <progress max={100} value={progress}>
              {progress}%
            </progress>
            {currentTask.currentFileName ? (
              <p>{currentTask.currentFileName}</p>
            ) : null}
            {currentTask.message ? <p>{currentTask.message}</p> : null}
            {["queued", "running"].includes(currentTask.state) ? (
              <button
                className="danger-text-button"
                type="button"
                onClick={() => void service.cancelImport(currentTask.id)}
              >
                {text.library.import.cancel}
              </button>
            ) : (
              <div className="dialog-actions">
                <button
                  className="secondary-button"
                  type="button"
                  onClick={onClose}
                >
                  {text.library.form.close}
                </button>
                <button
                  className="primary-button"
                  type="button"
                  onClick={() => onTaskChange(null)}
                >
                  {text.library.import.another}
                </button>
              </div>
            )}
          </section>
        ) : (
          <div className="dialog-actions">
            <button
              className="secondary-button"
              type="button"
              onClick={onClose}
            >
              {text.library.form.cancel}
            </button>
            <button className="primary-button" type="submit">
              {text.library.import.start}
            </button>
          </div>
        )}
      </form>
    </LibraryDialog>
  );
}
