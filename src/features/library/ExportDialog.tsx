import { useState } from "react";
import { text } from "../../app/texts";
import type { ExportMode } from "../../services/library-service";

export function ExportDialog({
  count,
  busy,
  onClose,
  onExport,
}: {
  readonly count: number;
  readonly busy: boolean;
  readonly onClose: () => void;
  readonly onExport: (mode: ExportMode) => void;
}) {
  const [mode, setMode] = useState<ExportMode>("complete");

  return (
    <div className="dialog-backdrop" role="presentation">
      <section
        className="library-dialog export-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="export-dialog-title"
      >
        <p className="eyebrow">LOCAL EXPORT</p>
        <h2 id="export-dialog-title">{text.library.export.title}</h2>
        <p>{text.library.export.description}</p>
        <fieldset className="export-mode-options" disabled={busy}>
          <legend>{text.library.export.selectedItems(count)}</legend>
          <label className="export-mode-option">
            <input
              type="radio"
              name="export-mode"
              value="complete"
              checked={mode === "complete"}
              onChange={() => setMode("complete")}
            />
            <span>
              <strong>{text.library.export.complete}</strong>
              <small>{text.library.export.completeDescription}</small>
            </span>
          </label>
          <label className="export-mode-option">
            <input
              type="radio"
              name="export-mode"
              value="prompts"
              checked={mode === "prompts"}
              onChange={() => setMode("prompts")}
            />
            <span>
              <strong>{text.library.export.prompts}</strong>
              <small>{text.library.export.promptsDescription}</small>
            </span>
          </label>
        </fieldset>
        <div className="dialog-actions">
          <button
            className="secondary-button"
            type="button"
            disabled={busy}
            onClick={onClose}
          >
            {text.library.form.cancel}
          </button>
          <button
            className="primary-button"
            type="button"
            disabled={busy}
            onClick={() => onExport(mode)}
          >
            {busy ? text.library.export.exporting : text.library.export.start}
          </button>
        </div>
      </section>
    </div>
  );
}
