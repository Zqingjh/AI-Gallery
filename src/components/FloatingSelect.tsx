import { useEffect, useId, useRef, useState } from "react";

export interface FloatingSelectOption {
  readonly value: string;
  readonly label: string;
  readonly group?: string;
}

function useFloatingPanel() {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const closeOutside = (event: MouseEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) setOpen(false);
    };
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    document.addEventListener("mousedown", closeOutside);
    document.addEventListener("keydown", closeOnEscape);
    return () => {
      document.removeEventListener("mousedown", closeOutside);
      document.removeEventListener("keydown", closeOnEscape);
    };
  }, [open]);

  return { open, setOpen, rootRef };
}

export function FloatingSingleSelect({
  label,
  value,
  options,
  onChange,
}: {
  readonly label: string;
  readonly value: string;
  readonly options: readonly FloatingSelectOption[];
  readonly onChange: (value: string) => void;
}) {
  const panelId = useId();
  const { open, setOpen, rootRef } = useFloatingPanel();
  const selectedLabel = options.find((option) => option.value === value)?.label;

  return (
    <div className="floating-select" ref={rootRef}>
      <button
        type="button"
        className="floating-select-trigger"
        aria-label={label}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={panelId}
        onClick={() => setOpen((current) => !current)}
      >
        <span>{selectedLabel ?? ""}</span>
        <i aria-hidden="true" />
      </button>
      {open ? (
        <div
          id={panelId}
          className="floating-select-panel"
          role="listbox"
          aria-label={label}
        >
          <button
            type="button"
            role="option"
            aria-selected={!value}
            className={!value ? "is-selected" : ""}
            onClick={() => {
              onChange("");
              setOpen(false);
            }}
          >
            全部
          </button>
          {options.map((option) => (
            <button
              type="button"
              role="option"
              aria-selected={option.value === value}
              className={option.value === value ? "is-selected" : ""}
              key={option.value}
              onClick={() => {
                onChange(option.value);
                setOpen(false);
              }}
            >
              {option.label}
            </button>
          ))}
        </div>
      ) : null}
    </div>
  );
}

export function FloatingMultiSelect({
  label,
  value,
  options,
  onChange,
}: {
  readonly label: string;
  readonly value: readonly string[];
  readonly options: readonly FloatingSelectOption[];
  readonly onChange: (value: readonly string[]) => void;
}) {
  const panelId = useId();
  const { open, setOpen, rootRef } = useFloatingPanel();
  const selected = new Set(value);
  const selectedLabels = options
    .filter((option) => selected.has(option.value))
    .map((option) => option.label);
  let previousGroup: string | undefined;

  return (
    <div className="floating-select floating-select-multiple" ref={rootRef}>
      <span className="floating-select-label">{label}</span>
      <button
        type="button"
        className="floating-select-trigger"
        aria-label={label}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={panelId}
        onClick={() => setOpen((current) => !current)}
      >
        <span>{selectedLabels.join("、")}</span>
        <i aria-hidden="true" />
      </button>
      {open ? (
        <div className="floating-select-popover">
          <div
            id={panelId}
            className="floating-select-panel floating-select-panel-multiple"
            role="listbox"
            aria-label={label}
            aria-multiselectable="true"
          >
            {options.map((option) => {
              const showGroup = Boolean(
                option.group && option.group !== previousGroup,
              );
              previousGroup = option.group;
              const checked = selected.has(option.value);
              return (
                <div key={option.value}>
                  {showGroup ? (
                    <p className="floating-select-group">{option.group}</p>
                  ) : null}
                  <button
                    type="button"
                    role="option"
                    aria-selected={checked}
                    className={checked ? "is-selected" : ""}
                    onClick={() => {
                      const next = new Set(value);
                      if (checked) next.delete(option.value);
                      else next.add(option.value);
                      onChange([...next]);
                    }}
                  >
                    <i aria-hidden="true" />
                    {option.label}
                  </button>
                </div>
              );
            })}
          </div>
          <button
            type="button"
            className="floating-select-done"
            onClick={() => setOpen(false)}
          >
            完成
          </button>
        </div>
      ) : null}
    </div>
  );
}
