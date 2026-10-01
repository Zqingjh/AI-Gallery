import { useCallback, useEffect, useState } from "react";
import { text } from "../../app/texts";
import type {
  LibraryService,
  PageResult,
  TrashItem,
} from "../../services/library-service";
import { LibraryDialog } from "./LibraryDialog";

export default function TrashPanel({
  service,
  onRestored,
}: {
  readonly service: LibraryService;
  readonly onRestored?: () => void;
}) {
  const [page, setPage] = useState<PageResult<TrashItem>>({
    items: [],
    nextCursor: null,
  });
  const [permanent, setPermanent] = useState<TrashItem | null>(null);
  const [selectedIds, setSelectedIds] = useState<ReadonlySet<string>>(
    () => new Set(),
  );
  const [permanentSelected, setPermanentSelected] = useState<
    readonly string[] | null
  >(null);
  const [operationError, setOperationError] = useState<string | null>(null);
  const load = useCallback(async () => {
    setPage(await service.listTrash({ limit: 50 }));
    setSelectedIds(new Set());
  }, [service]);
  const loadMore = useCallback(async () => {
    if (!page.nextCursor) return;
    const next = await service.listTrash({
      cursor: page.nextCursor,
      limit: 50,
    });
    setPage((current) => ({
      items: [...current.items, ...next.items],
      nextCursor: next.nextCursor,
    }));
  }, [page.nextCursor, service]);
  useEffect(() => {
    void load();
  }, [load]);
  const toggle = (id: string) =>
    setSelectedIds((current) => {
      if (!current.has(id) && current.size >= 100) {
        setOperationError(text.library.detail.selectAllLimit);
        return current;
      }
      const next = new Set(current);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  const toggleAll = () => {
    const ids = page.items.map((item) => item.id);
    const allSelected =
      ids.length > 0 && ids.every((id) => selectedIds.has(id));
    if (!allSelected && ids.length > 100) {
      setOperationError(text.library.detail.selectAllLimit);
      return;
    }
    setSelectedIds((current) => {
      const next = new Set(current);
      ids.forEach((id) => (allSelected ? next.delete(id) : next.add(id)));
      return next;
    });
  };
  const restoreSelected = async () => {
    try {
      await service.restoreTrashItems([...selectedIds]);
      await load();
      onRestored?.();
      setOperationError(null);
    } catch {
      setOperationError("批量恢复失败，未修改任何记录。");
    }
  };
  const purgeSelected = async (ids: readonly string[]) => {
    try {
      await service.permanentlyDeleteTrashItems(ids);
      setPermanentSelected(null);
      await load();
      setOperationError(null);
    } catch {
      setPermanentSelected(null);
      setOperationError("批量永久删除失败，未修改任何记录。");
    }
  };
  return (
    <section className="management-panel">
      <header>
        <div>
          <h2>{text.library.trash.title}</h2>
          <p>{text.library.trash.description}</p>
        </div>
        <button
          className="secondary-button"
          type="button"
          onClick={() => void service.undoLastTrash().then(load)}
        >
          {text.library.trash.undo}
        </button>
      </header>
      {page.items.length === 0 ? (
        <p className="management-empty">{text.library.trash.empty}</p>
      ) : (
        <>
          <div className="asset-selection-bar trash-selection-bar">
            <label className="check-field">
              <input
                type="checkbox"
                checked={
                  page.items.length > 0 &&
                  page.items.every((item) => selectedIds.has(item.id))
                }
                onChange={toggleAll}
              />
              {text.library.trash.selectAllPage}
            </label>
            <span>{text.library.trash.selectedItems(selectedIds.size)}</span>
            <button
              className="secondary-button"
              type="button"
              disabled={selectedIds.size === 0}
              onClick={() => void restoreSelected()}
            >
              {text.library.trash.restoreSelected}
            </button>
            <button
              className="danger-button"
              type="button"
              disabled={selectedIds.size === 0}
              onClick={() => setPermanentSelected([...selectedIds])}
            >
              {text.library.trash.permanentSelected}
            </button>
          </div>
          <div className="trash-list">
            {page.items.map((item) => (
              <article key={item.id}>
                <label className="trash-item-select">
                  <input
                    type="checkbox"
                    checked={selectedIds.has(item.id)}
                    onChange={() => toggle(item.id)}
                    aria-label={`选择回收站记录：${item.title}`}
                  />
                </label>
                <div className="trash-thumb">
                  {item.thumbnailUrl ? (
                    <img src={item.thumbnailUrl} alt="" />
                  ) : (
                    <span>×</span>
                  )}
                </div>
                <div>
                  <h3>{item.title}</h3>
                  <p>
                    {item.originalMediaPreserved
                      ? text.library.trash.preserved
                      : ""}
                  </p>
                </div>
                <button
                  className="secondary-button"
                  type="button"
                  onClick={() =>
                    void service.restoreTrashItem(item.id).then(async () => {
                      await load();
                      onRestored?.();
                    })
                  }
                >
                  {text.library.trash.restore}
                </button>
                <button
                  className="danger-text-button"
                  type="button"
                  onClick={() => setPermanent(item)}
                >
                  {text.library.trash.permanent}
                </button>
              </article>
            ))}
            {page.nextCursor ? (
              <button
                className="secondary-button"
                type="button"
                onClick={() => void loadMore()}
              >
                {text.library.loadMore}
              </button>
            ) : null}
          </div>
        </>
      )}
      {operationError ? (
        <p className="form-error" role="alert">
          {operationError}
        </p>
      ) : null}
      {permanent ? (
        <LibraryDialog
          title={text.library.trash.permanentTitle}
          description={text.library.trash.permanentDescription}
          onClose={() => setPermanent(null)}
          danger
        >
          <div className="dialog-actions">
            <button
              className="secondary-button"
              type="button"
              onClick={() => setPermanent(null)}
            >
              {text.library.form.cancel}
            </button>
            <button
              className="danger-button"
              type="button"
              onClick={() =>
                void service
                  .permanentlyDeleteTrashItem(permanent.id)
                  .then(() => {
                    setPermanent(null);
                    return load();
                  })
              }
            >
              {text.library.trash.permanentAgain}
            </button>
          </div>
        </LibraryDialog>
      ) : null}
      {permanentSelected ? (
        <LibraryDialog
          title={text.library.trash.permanentSelectedTitle(
            permanentSelected.length,
          )}
          description={text.library.trash.permanentDescription}
          onClose={() => setPermanentSelected(null)}
          danger
        >
          <div className="dialog-actions">
            <button
              className="secondary-button"
              type="button"
              onClick={() => setPermanentSelected(null)}
            >
              {text.library.form.cancel}
            </button>
            <button
              className="danger-button"
              type="button"
              onClick={() => void purgeSelected(permanentSelected)}
            >
              {text.library.trash.permanentAgain}
            </button>
          </div>
        </LibraryDialog>
      ) : null}
    </section>
  );
}
