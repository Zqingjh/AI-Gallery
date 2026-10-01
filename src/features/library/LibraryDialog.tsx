import type { ReactNode } from "react";

interface LibraryDialogProps {
  readonly title: string;
  readonly description?: string;
  readonly onClose: () => void;
  readonly children: ReactNode;
  readonly danger?: boolean;
}

export function LibraryDialog({
  title,
  description,
  onClose,
  children,
  danger = false,
}: LibraryDialogProps) {
  return (
    <div className="dialog-backdrop" role="presentation" onMouseDown={onClose}>
      <section
        className={`library-dialog${danger ? " is-danger" : ""}`}
        role="dialog"
        aria-modal="true"
        aria-labelledby="library-dialog-title"
        onMouseDown={(event) => event.stopPropagation()}
      >
        <header className="dialog-header">
          <div>
            <p className="eyebrow">AI GALLERY · LOCAL</p>
            <h2 id="library-dialog-title">{title}</h2>
            {description ? <p>{description}</p> : null}
          </div>
          <button
            className="icon-button"
            type="button"
            aria-label="关闭"
            onClick={onClose}
          >
            ×
          </button>
        </header>
        {children}
      </section>
    </div>
  );
}
