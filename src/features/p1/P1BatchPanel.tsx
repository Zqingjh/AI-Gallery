import { useEffect, useState } from "react";
import { FloatingMultiSelect } from "../../components/FloatingSelect";
import type { AiProvider, AiService } from "../../services/ai-service";
import type {
  LibraryService,
  MetadataPresets,
} from "../../services/library-service";
import type {
  P1BulkAssetEditInput,
  P1LibraryService,
} from "../../services/p1-library-service";

function ids(value: string): number[] {
  const parsed = new Set<number>();
  for (const token of value.split(/[\s,，;；]+/).filter(Boolean)) {
    const range = /^(\d+)\s*-\s*(\d+)$/.exec(token);
    if (range) {
      const start = Number(range[1]);
      const end = Number(range[2]);
      if (
        Number.isSafeInteger(start) &&
        Number.isSafeInteger(end) &&
        start > 0 &&
        end >= start &&
        end - start < 100
      ) {
        for (let id = start; id <= end; id += 1) parsed.add(id);
      }
      continue;
    }
    const id = Number(token);
    if (Number.isSafeInteger(id) && id > 0) parsed.add(id);
  }
  return [...parsed];
}

function aiFailureSummary(failureKinds: readonly string[]): string {
  const labels: Record<string, string> = {
    input_unavailable: "提示词不可用",
    request_failed: "服务请求失败",
    response_invalid: "服务回复格式错误",
    persist_failed: "建议保存失败",
  };
  const counts = new Map<string, number>();
  for (const kind of failureKinds)
    counts.set(kind, (counts.get(kind) ?? 0) + 1);
  return [...counts]
    .map(([kind, count]) => `${labels[kind] ?? "未知错误"} ${count} 件`)
    .join("；");
}

export function P1BatchPanel({
  service,
  taxonomyService,
  aiService,
  readOnly,
}: {
  readonly service: P1LibraryService;
  readonly taxonomyService: Pick<
    LibraryService,
    "listTaxonomy" | "listMetadataPresets"
  >;
  readonly aiService?: AiService;
  readonly readOnly: boolean;
}) {
  const [assetIds, setAssetIds] = useState("");
  const [aiAssetIds, setAiAssetIds] = useState("");
  const [rating, setRating] = useState("");
  const [model, setModel] = useState("");
  const [platform, setPlatform] = useState("");
  const [displayStates, setDisplayStates] = useState<readonly string[]>([]);
  const [taxonomy, setTaxonomy] = useState<
    Awaited<ReturnType<LibraryService["listTaxonomy"]>>
  >({ dimensions: [], tags: [] });
  const [metadataPresets, setMetadataPresets] = useState<MetadataPresets>({
    models: [],
    platforms: [],
  });
  const [addCategories, setAddCategories] = useState<readonly number[]>([]);
  const [removeCategories, setRemoveCategories] = useState<readonly number[]>(
    [],
  );
  const [addTags, setAddTags] = useState<readonly number[]>([]);
  const [removeTags, setRemoveTags] = useState<readonly number[]>([]);
  const [impact, setImpact] = useState<string | null>(null);
  const [operationError, setOperationError] = useState<string | null>(null);
  const [providers, setProviders] = useState<readonly AiProvider[]>([]);
  const [providerId, setProviderId] = useState("");
  const [aiImpact, setAiImpact] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    void taxonomyService
      .listTaxonomy()
      .then((nextTaxonomy) => {
        if (active) setTaxonomy(nextTaxonomy);
      })
      .catch(() => {
        if (active) setOperationError("无法读取分类与标签预设。");
      });
    return () => {
      active = false;
    };
  }, [taxonomyService]);

  useEffect(() => {
    let active = true;
    void taxonomyService
      .listMetadataPresets()
      .then((value) => {
        if (active) setMetadataPresets(value);
      })
      .catch(() => {
        // 预设只是快捷入口；加载失败时仍允许输入新值。
      });
    return () => {
      active = false;
    };
  }, [taxonomyService]);

  useEffect(() => {
    let active = true;
    if (!aiService)
      return () => {
        active = false;
      };
    void aiService
      .listProviders()
      .then((value) => {
        if (active) setProviders(value);
      })
      .catch(() => {
        if (active)
          setOperationError("无法读取已配置的 AI 服务，请到设置页检查服务。 ");
      });
    return () => {
      active = false;
    };
  }, [aiService]);

  const input = (): P1BulkAssetEditInput => ({
    assetIds: ids(assetIds),
    ...(rating ? { rating: Number(rating) } : {}),
    ...(displayStates.includes("favorite") ? { isFavorite: true } : {}),
    ...(displayStates.includes("public") ? { isPublic: true } : {}),
    model: model.trim()
      ? { action: "set", value: model.trim() }
      : { action: "keep" },
    platform: platform.trim()
      ? { action: "set", value: platform.trim() }
      : { action: "keep" },
    addCategoryIds: addCategories,
    removeCategoryIds: removeCategories,
    addTagIds: addTags,
    removeTagIds: removeTags,
  });

  function bulkFieldSummary(): string {
    const fields: string[] = [];
    if (rating) fields.push(`评分 ${rating}`);
    if (displayStates.includes("public")) fields.push("公开");
    if (displayStates.includes("favorite")) fields.push("收藏");
    if (model.trim()) fields.push(`模型“${model.trim()}”`);
    if (platform.trim()) fields.push(`平台“${platform.trim()}”`);
    if (addCategories.length || removeCategories.length)
      fields.push("分类关系");
    if (addTags.length || removeTags.length) fields.push("标签关系");
    return fields.length ? fields.join("、") : "未设置任何变更字段";
  }

  async function previewBulk() {
    try {
      const value = await service.previewBulkAssetEdit(input());
      setImpact(
        `将影响 ${value.targetCount} 件作品；设置：${bulkFieldSummary()}；分类 +${value.categoryRelationsToAdd}/-${value.categoryRelationsToRemove}；标签 +${value.tagRelationsToAdd}/-${value.tagRelationsToRemove}`,
      );
      setOperationError(null);
    } catch {
      setOperationError("无法预览批量修改，请核对编号与输入值。");
    }
  }
  async function applyBulk() {
    try {
      const value = await service.previewBulkAssetEdit(input());
      setImpact(
        `将影响 ${value.targetCount} 件作品；设置：${bulkFieldSummary()}；分类 +${value.categoryRelationsToAdd}/-${value.categoryRelationsToRemove}；标签 +${value.tagRelationsToAdd}/-${value.tagRelationsToRemove}`,
      );
      setOperationError(null);
      if (
        !window.confirm(
          `确认批量修改 ${value.targetCount} 件作品？\n将设置：${bulkFieldSummary()}`,
        )
      )
        return;
      await service.bulkEditAssets(input());
      setImpact("批量修改已完成。");
    } catch {
      setOperationError("批量修改失败，未保存任何变更。");
    }
  }

  async function previewAiClassification() {
    if (!providerId || !ids(aiAssetIds).length) {
      setOperationError("请选择 AI 服务，并输入至少一个作品编号。");
      return;
    }
    try {
      const preview = await service.previewBatchAi({
        providerId: Number(providerId),
        assetIds: ids(aiAssetIds),
        inputScope: {
          title: false,
          promptZh: true,
          promptEn: true,
          negativePrompt: true,
        },
      });
      setAiImpact(
        `将检查 ${preview.targetCount} 件作品的提示词，并按 ${preview.taxonomyDimensionCount} 个预设分类维度生成待审核建议；没有提示词的作品会跳过。`,
      );
      setOperationError(null);
    } catch {
      setOperationError("无法预览 AI 分类，请检查作品编号、服务与分类预设。");
    }
  }

  async function createAiClassification() {
    if (!providerId || !ids(aiAssetIds).length) {
      setOperationError("请选择 AI 服务，并输入至少一个作品编号。");
      return;
    }
    try {
      const selectedAssetIds = ids(aiAssetIds);
      if (
        !window.confirm(
          `确认让 AI 检查 ${selectedAssetIds.length} 件作品的提示词并生成分类建议？`,
        )
      )
        return;
      const result = await service.createBatchAi({
        providerId: Number(providerId),
        assetIds: selectedAssetIds,
        inputScope: {
          title: false,
          promptZh: true,
          promptEn: true,
          negativePrompt: true,
        },
      });
      const failedReason = aiFailureSummary(result.failureKinds);
      setAiImpact(
        `已为 ${result.created} 件作品建立待审核建议；${result.failed} 件跳过${failedReason ? `（${failedReason}）` : ""}。新分类建议需在“待审核”中采纳后才会加入预设库。`,
      );
      setOperationError(null);
    } catch {
      setOperationError("AI 批量分类未完成，请检查服务配置后重试。");
    }
  }

  return (
    <>
      {!readOnly ? (
        <section className="management-panel" aria-labelledby="bulk-title">
          <h2 id="bulk-title">批量编辑与分类</h2>
          <p>先预览影响，再明确确认；单次最多 100 件作品。</p>
          <div className="library-form two-column-form">
            <label className="form-field form-wide">
              <span>作品编号（逗号、分号或 1-20）</span>
              <input
                value={assetIds}
                onChange={(event) => setAssetIds(event.target.value)}
              />
            </label>
            <label className="form-field">
              <span>评分（留空保持）</span>
              <input
                type="number"
                min="0"
                max="5"
                value={rating}
                onChange={(event) => setRating(event.target.value)}
              />
            </label>
            <label className="form-field">
              <span>添加模型（留空保持）</span>
              <input
                value={model}
                list="p1-batch-model-presets"
                maxLength={200}
                onChange={(event) => setModel(event.target.value)}
              />
              <datalist id="p1-batch-model-presets">
                {metadataPresets.models.map((preset) => (
                  <option key={preset.id} value={preset.name} />
                ))}
              </datalist>
            </label>
            <label className="form-field">
              <span>添加平台（留空保持）</span>
              <input
                value={platform}
                list="p1-batch-platform-presets"
                maxLength={200}
                onChange={(event) => setPlatform(event.target.value)}
              />
              <datalist id="p1-batch-platform-presets">
                {metadataPresets.platforms.map((preset) => (
                  <option key={preset.id} value={preset.name} />
                ))}
              </datalist>
            </label>
            <FloatingMultiSelect
              label="展示情况（留空保持）"
              value={displayStates}
              options={[
                { value: "public", label: "公开" },
                { value: "favorite", label: "收藏" },
              ]}
              onChange={setDisplayStates}
            />
            <FloatingMultiSelect
              label="添加分类"
              value={addCategories.map(String)}
              options={taxonomy.dimensions.flatMap((dimension) =>
                dimension.categories.map((category) => ({
                  value: category.id,
                  label: category.name,
                  group: dimension.name,
                })),
              )}
              onChange={(value) => setAddCategories(value.map(Number))}
            />
            <FloatingMultiSelect
              label="移除分类"
              value={removeCategories.map(String)}
              options={taxonomy.dimensions.flatMap((dimension) =>
                dimension.categories.map((category) => ({
                  value: category.id,
                  label: category.name,
                  group: dimension.name,
                })),
              )}
              onChange={(value) => setRemoveCategories(value.map(Number))}
            />
            <FloatingMultiSelect
              label="添加标签"
              value={addTags.map(String)}
              options={taxonomy.tags.map((tag) => ({
                value: tag.id,
                label: tag.name,
              }))}
              onChange={(value) => setAddTags(value.map(Number))}
            />
            <FloatingMultiSelect
              label="移除标签"
              value={removeTags.map(String)}
              options={taxonomy.tags.map((tag) => ({
                value: tag.id,
                label: tag.name,
              }))}
              onChange={(value) => setRemoveTags(value.map(Number))}
            />
            <div className="dialog-actions form-wide">
              <button
                type="button"
                className="secondary-button"
                onClick={() => void previewBulk()}
              >
                预览影响
              </button>
              <button
                type="button"
                className="primary-button"
                onClick={() => void applyBulk()}
              >
                确认批量修改
              </button>
            </div>
          </div>
          {impact ? <p role="status">{impact}</p> : null}
        </section>
      ) : null}

      {!readOnly ? (
        <section className="management-panel" aria-labelledby="ai-batch-title">
          <h2 id="ai-batch-title">AI 批量分类</h2>
          <p>
            仅发送作品提示词，不发送媒体、路径或备注。AI
            只生成待审核建议，不会直接改写正式分类。
          </p>
          <div className="library-form two-column-form">
            <label className="form-field form-wide">
              <span>AI 分类作品编号（逗号、分号或 1-20）</span>
              <input
                value={aiAssetIds}
                onChange={(event) => setAiAssetIds(event.target.value)}
              />
            </label>
            <label className="form-field">
              <span>使用已配置的服务</span>
              <select
                value={providerId}
                onChange={(event) => setProviderId(event.target.value)}
              >
                <option value="">请选择服务</option>
                {providers
                  .filter(
                    (provider) =>
                      provider.isEnabled &&
                      provider.capabilities.textClassification,
                  )
                  .map((provider) => (
                    <option key={provider.id} value={provider.id}>
                      {provider.displayName} · {provider.model}
                    </option>
                  ))}
              </select>
            </label>
            <div className="dialog-actions form-wide">
              <button
                className="secondary-button"
                type="button"
                onClick={() => void previewAiClassification()}
              >
                预览 AI 分类范围
              </button>
              <button
                className="primary-button"
                type="button"
                onClick={() => void createAiClassification()}
              >
                生成待审核建议
              </button>
            </div>
          </div>
          {aiImpact ? <p role="status">{aiImpact}</p> : null}
        </section>
      ) : null}

      {operationError ? (
        <p className="form-error" role="alert">
          {operationError}
        </p>
      ) : null}
    </>
  );
}
