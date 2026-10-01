import { useEffect, useState } from "react";
import { text } from "../../app/texts";
import type {
  AssetSummary,
  CategoryDimension,
  LibraryService,
  MetadataPreset,
  MetadataPresetKind,
  NumberedPageResult,
  TagItem,
} from "../../services/library-service";
import { AssetCardGrid } from "./AssetCardGrid";
import { LibraryDialog } from "./LibraryDialog";

const emptyCategoryPage: NumberedPageResult<AssetSummary> = {
  items: [],
  page: 1,
  pageSize: 25,
  totalCount: 0,
  totalPages: 0,
};

export default function TaxonomyPanel({
  service,
  onOpenAsset,
}: {
  readonly service: LibraryService;
  readonly onOpenAsset: (asset: AssetSummary) => void;
}) {
  const [dimensions, setDimensions] = useState<readonly CategoryDimension[]>(
    [],
  );
  const [tags, setTags] = useState<readonly TagItem[]>([]);
  const [dimensionName, setDimensionName] = useState("");
  const [categoryName, setCategoryName] = useState("");
  const [categoryDimensionId, setCategoryDimensionId] = useState("");
  const [tagName, setTagName] = useState("");
  const [replacementId, setReplacementId] = useState("");
  const [selectedCategoryId, setSelectedCategoryId] = useState<string | null>(
    null,
  );
  const [categoryPage, setCategoryPage] =
    useState<NumberedPageResult<AssetSummary>>(emptyCategoryPage);
  const [categoryPageNumber, setCategoryPageNumber] = useState(1);
  const [categoryLoading, setCategoryLoading] = useState(false);
  const [editingCategory, setEditingCategory] = useState(false);
  const [editCategoryName, setEditCategoryName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [view, setView] = useState<"taxonomy" | "metadata">("taxonomy");
  const [deleting, setDeleting] = useState<{
    id: string;
    name: string;
    count: number;
  } | null>(null);
  const selectedDimension = dimensions.find((dimension) =>
    dimension.categories.some((category) => category.id === selectedCategoryId),
  );
  const selectedCategory = selectedDimension?.categories.find(
    (category) => category.id === selectedCategoryId,
  );
  useEffect(() => {
    let active = true;
    void service.listTaxonomy().then((value) => {
      if (active) {
        setDimensions(value.dimensions);
        setTags(value.tags);
        setCategoryDimensionId(
          (current) => current || value.dimensions[0]?.id || "",
        );
      }
    });
    return () => {
      active = false;
    };
  }, [service]);
  useEffect(() => {
    if (!selectedCategoryId) {
      setCategoryPage(emptyCategoryPage);
      return;
    }
    let active = true;
    setCategoryPage((page) => ({
      ...page,
      items: [],
      page: categoryPageNumber,
    }));
    setCategoryLoading(true);
    setError(null);
    void service
      .listAssetPage({
        categoryIds: [selectedCategoryId],
        page: categoryPageNumber,
        pageSize: 25,
      })
      .then((page) => {
        if (active) setCategoryPage(page);
      })
      .catch(() => {
        if (active) setError(text.library.loadFailed);
      })
      .finally(() => {
        if (active) setCategoryLoading(false);
      });
    return () => {
      active = false;
    };
  }, [categoryPageNumber, selectedCategoryId, service]);
  async function addDimension(event: React.FormEvent) {
    event.preventDefault();
    if (!dimensionName.trim()) return;
    const created = await service.createDimension(dimensionName.trim());
    setDimensions((items) => [...items, created]);
    setCategoryDimensionId((current) => current || created.id);
    setDimensionName("");
  }
  async function addCategory(event: React.FormEvent) {
    event.preventDefault();
    const dimension = dimensions.find(
      (item) => item.id === categoryDimensionId,
    );
    if (!dimension || !categoryName.trim()) return;
    const created = await service.createCategory({
      dimensionId: dimension.id,
      name: categoryName.trim(),
    });
    setDimensions((items) =>
      items.map((item) =>
        item.id === dimension.id
          ? { ...item, categories: [...item.categories, created] }
          : item,
      ),
    );
    setCategoryName("");
  }
  async function addTag(event: React.FormEvent) {
    event.preventDefault();
    if (!tagName.trim()) return;
    const created = await service.createTag(tagName.trim());
    setTags((items) => [...items, created]);
    setTagName("");
  }
  async function renameDimension(dimension: CategoryDimension) {
    const name = window
      .prompt(text.library.taxonomy.renamePrompt, dimension.name)
      ?.trim();
    if (!name || name === dimension.name) return;
    try {
      await service.updateDimension(dimension, name);
      setDimensions((items) =>
        items.map((item) =>
          item.id === dimension.id ? { ...item, name } : item,
        ),
      );
    } catch {
      setError(text.library.loadFailed);
    }
  }
  async function deleteDimension(dimension: CategoryDimension) {
    if (!window.confirm(text.library.taxonomy.deleteDimensionConfirm)) return;
    try {
      await service.deleteDimension(dimension.id);
      setDimensions((items) =>
        items.filter((item) => item.id !== dimension.id),
      );
    } catch {
      setError(text.library.taxonomy.deleteDimensionFailed);
    }
  }
  async function saveCategoryName(event: React.FormEvent) {
    event.preventDefault();
    if (!selectedCategory) return;
    const name = editCategoryName.trim();
    if (!name) return;
    try {
      await service.updateCategory(selectedCategory, name);
      setDimensions((items) =>
        items.map((dimension) => ({
          ...dimension,
          categories: dimension.categories.map((item) =>
            item.id === selectedCategory.id ? { ...item, name } : item,
          ),
        })),
      );
      setEditingCategory(false);
    } catch {
      setError(text.library.loadFailed);
    }
  }
  async function prepareCategoryRemoval(replacementCategoryId: string | null) {
    if (!selectedCategory || !selectedDimension) return;
    try {
      const count = await service.getCategoryImpact(selectedCategory.id);
      setReplacementId(replacementCategoryId ?? "");
      setEditingCategory(false);
      setDeleting({
        id: selectedCategory.id,
        name: selectedCategory.name,
        count,
      });
    } catch {
      setError(text.library.loadFailed);
    }
  }
  async function confirmCategoryRemoval() {
    if (!deleting) return;
    try {
      await service.deleteCategory({
        categoryId: deleting.id,
        replacementCategoryId: replacementId || null,
      });
      setDeleting(null);
      setSelectedCategoryId(null);
      setCategoryPageNumber(1);
      setDimensions((items) =>
        items.map((dimension) => ({
          ...dimension,
          categories: dimension.categories.filter(
            (category) => category.id !== deleting.id,
          ),
        })),
      );
      try {
        const taxonomy = await service.listTaxonomy();
        setDimensions(taxonomy.dimensions);
        setTags(taxonomy.tags);
      } catch {
        setError(text.library.loadFailed);
      }
    } catch {
      setError(text.library.loadFailed);
    }
  }
  async function renameTag(tag: TagItem) {
    const name = window
      .prompt(text.library.taxonomy.renamePrompt, tag.name)
      ?.trim();
    if (!name || name === tag.name) return;
    try {
      await service.updateTag(tag.id, name);
      setTags((items) =>
        items.map((item) => (item.id === tag.id ? { ...item, name } : item)),
      );
    } catch {
      setError(text.library.loadFailed);
    }
  }
  async function deleteTag(tag: TagItem) {
    if (!window.confirm(text.library.taxonomy.deleteTagConfirm)) return;
    try {
      await service.deleteTag(tag.id);
      setTags((items) => items.filter((item) => item.id !== tag.id));
    } catch {
      setError(text.library.loadFailed);
    }
  }
  if (selectedCategory && selectedDimension) {
    const replacement = selectedDimension.categories.find(
      (category) => category.id === replacementId,
    );
    return (
      <section className="management-panel category-detail-page">
        <header className="category-detail-heading">
          <div>
            <button
              className="text-button"
              type="button"
              onClick={() => {
                setSelectedCategoryId(null);
                setCategoryPageNumber(1);
                setCategoryPage(emptyCategoryPage);
                setError(null);
              }}
            >
              ← {text.library.taxonomy.backToTaxonomy}
            </button>
            <p className="eyebrow">{selectedDimension.name}</p>
            <h2>{selectedCategory.name}</h2>
            <p>
              {text.library.taxonomy.categoryAssets(categoryPage.totalCount)}
            </p>
          </div>
          <button
            className="secondary-button"
            type="button"
            onClick={() => {
              setEditCategoryName(selectedCategory.name);
              setReplacementId("");
              setEditingCategory(true);
            }}
          >
            {text.library.taxonomy.editCategory}
          </button>
        </header>
        {error ? (
          <p className="form-error" role="alert">
            {error}
          </p>
        ) : null}
        {categoryLoading && categoryPage.items.length === 0 ? (
          <p className="route-loading">{text.library.loadingLibrary}</p>
        ) : categoryPage.items.length === 0 ? (
          <p className="management-empty">
            {text.library.taxonomy.categoryEmpty}
          </p>
        ) : (
          <AssetCardGrid
            assets={categoryPage.items}
            service={service}
            onOpen={onOpenAsset}
          />
        )}
        <nav
          className="numbered-pagination"
          aria-label={text.library.pagination}
        >
          <p>{text.library.paginationSummary(categoryPage.totalCount)}</p>
          <div className="page-buttons">
            <button
              className="secondary-button"
              type="button"
              disabled={categoryPageNumber <= 1 || categoryLoading}
              onClick={() => setCategoryPageNumber((page) => page - 1)}
            >
              {text.library.previousPage}
            </button>
            <button
              className="secondary-button"
              type="button"
              disabled={
                categoryPageNumber >= categoryPage.totalPages || categoryLoading
              }
              onClick={() => setCategoryPageNumber((page) => page + 1)}
            >
              {text.library.nextPage}
            </button>
          </div>
        </nav>
        {editingCategory ? (
          <LibraryDialog
            title={text.library.taxonomy.editCategoryTitle(
              selectedCategory.name,
            )}
            description={text.library.taxonomy.editCategoryDescription}
            onClose={() => setEditingCategory(false)}
          >
            <form className="library-form" onSubmit={saveCategoryName}>
              <label className="form-field">
                <span>{text.library.taxonomy.categoryName}</span>
                <input
                  required
                  value={editCategoryName}
                  onChange={(event) => setEditCategoryName(event.target.value)}
                />
              </label>
              <div className="dialog-actions">
                <button className="primary-button" type="submit">
                  {text.library.taxonomy.saveCategoryName}
                </button>
              </div>
              <div className="category-merge-editor">
                <label className="form-field">
                  <span>{text.library.taxonomy.mergeTarget}</span>
                  <select
                    value={replacementId}
                    onChange={(event) => setReplacementId(event.target.value)}
                  >
                    <option value="">
                      {text.library.taxonomy.chooseMergeTarget}
                    </option>
                    {selectedDimension.categories
                      .filter((category) => category.id !== selectedCategory.id)
                      .map((category) => (
                        <option key={category.id} value={category.id}>
                          {category.name}
                        </option>
                      ))}
                  </select>
                </label>
                <button
                  className="secondary-button"
                  type="button"
                  disabled={!replacementId}
                  onClick={() => void prepareCategoryRemoval(replacementId)}
                >
                  {text.library.taxonomy.mergeCategory}
                </button>
              </div>
              <button
                className="danger-button category-delete-button"
                type="button"
                onClick={() => void prepareCategoryRemoval(null)}
              >
                {text.library.taxonomy.delete}
              </button>
            </form>
          </LibraryDialog>
        ) : null}
        {deleting ? (
          <LibraryDialog
            title={
              replacement
                ? text.library.taxonomy.mergeConfirmTitle(deleting.name)
                : `${text.library.taxonomy.delete}：${deleting.name}`
            }
            description={`${text.library.taxonomy.affected(deleting.count)}。${
              replacement
                ? text.library.taxonomy.mergeConfirmDescription
                : text.library.taxonomy.deleteConfirmDescription
            }`}
            onClose={() => setDeleting(null)}
            danger
          >
            <div className="taxonomy-delete-dialog">
              <div className="dialog-actions">
                <button
                  className="secondary-button"
                  type="button"
                  onClick={() => setDeleting(null)}
                >
                  {text.library.form.cancel}
                </button>
                <button
                  className="danger-button"
                  type="button"
                  onClick={() => void confirmCategoryRemoval()}
                >
                  {replacement
                    ? text.library.taxonomy.confirmMerge
                    : text.library.taxonomy.delete}
                </button>
              </div>
            </div>
          </LibraryDialog>
        ) : null}
      </section>
    );
  }
  return (
    <section className="management-panel">
      <header>
        <div>
          <h2>{text.library.taxonomy.title}</h2>
          <p>{text.library.taxonomy.description}</p>
        </div>
        <div
          className="panel-switch"
          data-view={view}
          role="group"
          aria-label={text.library.taxonomy.switchLabel}
        >
          <button
            type="button"
            className={view === "taxonomy" ? "is-active" : ""}
            aria-pressed={view === "taxonomy"}
            onClick={() => setView("taxonomy")}
          >
            {text.library.taxonomy.taxonomyView}
          </button>
          <button
            type="button"
            className={view === "metadata" ? "is-active" : ""}
            aria-pressed={view === "metadata"}
            onClick={() => setView("metadata")}
          >
            {text.library.taxonomy.metadataView}
          </button>
        </div>
      </header>
      {view === "metadata" ? (
        <MetadataPresetPanel service={service} />
      ) : (
        <>
          <div className="taxonomy-layout">
            <div>
              {dimensions.length === 0 ? (
                <p>{text.library.taxonomy.empty}</p>
              ) : (
                dimensions.map((dimension) => (
                  <section className="dimension-group" key={dimension.id}>
                    <header className="taxonomy-item-actions">
                      <h3>{dimension.name}</h3>
                      <button
                        type="button"
                        onClick={() => void renameDimension(dimension)}
                      >
                        {text.library.taxonomy.rename}
                      </button>
                      <button
                        className="danger-text-button"
                        type="button"
                        onClick={() => void deleteDimension(dimension)}
                      >
                        {text.library.taxonomy.deleteDimension}
                      </button>
                    </header>
                    <div className="chip-list">
                      {dimension.categories.map((category) => (
                        <span className="taxonomy-chip" key={category.id}>
                          <button
                            type="button"
                            aria-label={text.library.taxonomy.openCategory(
                              category.name,
                            )}
                            onClick={() => {
                              setSelectedCategoryId(category.id);
                              setCategoryPageNumber(1);
                              setCategoryPage(emptyCategoryPage);
                            }}
                          >
                            <i
                              style={{
                                background:
                                  category.color ?? "var(--color-accent)",
                              }}
                            />
                            {category.name}
                            <small>{category.assetCount}</small>
                          </button>
                        </span>
                      ))}
                    </div>
                  </section>
                ))
              )}
            </div>
            <aside className="management-create">
              <form onSubmit={(event) => void addDimension(event)}>
                <label>
                  {text.library.taxonomy.dimensionName}
                  <input
                    value={dimensionName}
                    onChange={(event) => setDimensionName(event.target.value)}
                  />
                </label>
                <button className="secondary-button" type="submit">
                  {text.library.taxonomy.addDimension}
                </button>
              </form>
              <form onSubmit={(event) => void addCategory(event)}>
                <label>
                  {text.library.taxonomy.targetDimension}
                  <select
                    value={categoryDimensionId}
                    onChange={(event) =>
                      setCategoryDimensionId(event.target.value)
                    }
                  >
                    {dimensions.map((dimension) => (
                      <option key={dimension.id} value={dimension.id}>
                        {dimension.name}
                      </option>
                    ))}
                  </select>
                </label>
                <label>
                  {text.library.taxonomy.categoryName}
                  <input
                    value={categoryName}
                    onChange={(event) => setCategoryName(event.target.value)}
                  />
                </label>
                <button className="secondary-button" type="submit">
                  {text.library.taxonomy.addCategory}
                </button>
              </form>
              <form onSubmit={(event) => void addTag(event)}>
                <label>
                  {text.library.taxonomy.tagName}
                  <input
                    value={tagName}
                    onChange={(event) => setTagName(event.target.value)}
                  />
                </label>
                <button className="secondary-button" type="submit">
                  {text.library.taxonomy.addTag}
                </button>
              </form>
              <div className="chip-list">
                {tags.map((tag) => (
                  <span className="tag-chip taxonomy-item-actions" key={tag.id}>
                    <button type="button" onClick={() => void renameTag(tag)}>
                      #{tag.name} <small>{tag.assetCount}</small>
                    </button>
                    <button
                      className="danger-text-button"
                      type="button"
                      aria-label={`${text.library.taxonomy.delete}：${tag.name}`}
                      onClick={() => void deleteTag(tag)}
                    >
                      ×
                    </button>
                  </span>
                ))}
              </div>
            </aside>
          </div>
          {error ? (
            <p className="form-error" role="alert">
              {error}
            </p>
          ) : null}
        </>
      )}
    </section>
  );
}

function MetadataPresetPanel({
  service,
}: {
  readonly service: LibraryService;
}) {
  const [models, setModels] = useState<readonly MetadataPreset[]>([]);
  const [platforms, setPlatforms] = useState<readonly MetadataPreset[]>([]);
  const [modelName, setModelName] = useState("");
  const [platformName, setPlatformName] = useState("");
  const [error, setError] = useState<string | null>(null);

  async function load() {
    try {
      const value = await service.listMetadataPresets();
      setModels(value.models);
      setPlatforms(value.platforms);
      setError(null);
    } catch {
      setError(text.library.loadFailed);
    }
  }

  useEffect(() => {
    void load();
  }, [service]);

  async function create(kind: MetadataPresetKind, name: string) {
    const trimmed = name.trim();
    if (!trimmed) return;
    try {
      const created = await service.createMetadataPreset(kind, trimmed);
      if (kind === "model") {
        setModels((items) => [...items, created]);
        setModelName("");
      } else {
        setPlatforms((items) => [...items, created]);
        setPlatformName("");
      }
      setError(null);
    } catch {
      setError(text.library.taxonomy.presetSaveFailed);
    }
  }

  async function rename(kind: MetadataPresetKind, preset: MetadataPreset) {
    const name = window
      .prompt(text.library.taxonomy.renamePresetPrompt, preset.name)
      ?.trim();
    if (!name || name === preset.name) return;
    try {
      await service.updateMetadataPreset(kind, preset.id, name);
      const update = (items: readonly MetadataPreset[]) =>
        items.map((item) => (item.id === preset.id ? { ...item, name } : item));
      if (kind === "model") setModels(update);
      else setPlatforms(update);
      setError(null);
    } catch {
      setError(text.library.taxonomy.presetSaveFailed);
    }
  }

  async function remove(kind: MetadataPresetKind, preset: MetadataPreset) {
    if (
      !window.confirm(
        text.library.taxonomy.deletePresetConfirm(
          preset.name,
          preset.assetCount,
        ),
      )
    ) {
      return;
    }
    try {
      await service.deleteMetadataPreset(kind, preset.id);
      if (kind === "model") {
        setModels((items) => items.filter((item) => item.id !== preset.id));
      } else {
        setPlatforms((items) => items.filter((item) => item.id !== preset.id));
      }
      setError(null);
    } catch {
      setError(text.library.taxonomy.presetDeleteFailed);
    }
  }

  function group(
    kind: MetadataPresetKind,
    title: string,
    items: readonly MetadataPreset[],
    name: string,
    setName: (value: string) => void,
  ) {
    return (
      <section className="preset-group">
        <h3>{title}</h3>
        <form
          className="preset-create-form"
          onSubmit={(event) => {
            event.preventDefault();
            void create(kind, name);
          }}
        >
          <input
            aria-label={text.library.taxonomy.presetName(title)}
            value={name}
            onChange={(event) => setName(event.target.value)}
          />
          <button className="secondary-button" type="submit">
            {text.library.taxonomy.addPreset}
          </button>
        </form>
        <div className="preset-list">
          {items.map((preset) => (
            <div className="preset-row" key={preset.id}>
              <span>
                <strong>{preset.name}</strong>
                <small>{text.library.taxonomy.usedBy(preset.assetCount)}</small>
              </span>
              <div className="taxonomy-item-actions">
                <button type="button" onClick={() => void rename(kind, preset)}>
                  {text.library.taxonomy.rename}
                </button>
                <button
                  className="danger-text-button"
                  type="button"
                  onClick={() => void remove(kind, preset)}
                >
                  {text.library.taxonomy.deletePreset}
                </button>
              </div>
            </div>
          ))}
        </div>
      </section>
    );
  }

  return (
    <div className="metadata-preset-layout">
      {group(
        "model",
        text.library.taxonomy.models,
        models,
        modelName,
        setModelName,
      )}
      {group(
        "platform",
        text.library.taxonomy.platforms,
        platforms,
        platformName,
        setPlatformName,
      )}
      {error ? (
        <p className="form-error" role="alert">
          {error}
        </p>
      ) : null}
    </div>
  );
}
