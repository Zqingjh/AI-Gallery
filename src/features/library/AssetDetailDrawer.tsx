import { useEffect, useRef, useState } from "react";
import { text } from "../../app/texts";
import type {
  AssetDetail,
  LibraryService,
} from "../../services/library-service";

export function AssetDetailDrawer({
  asset,
  service,
  onClose,
  onEdit,
  onTrash,
  onNotice,
  onManageVideoCover,
  onUpdateOrder,
  readOnly,
}: {
  readonly asset: AssetDetail;
  readonly service: LibraryService;
  readonly onClose: () => void;
  readonly onEdit: () => void;
  readonly onTrash: () => void;
  readonly onNotice: (message: string) => void;
  readonly onManageVideoCover?: () => void;
  readonly onUpdateOrder?: (
    targetPosition: number,
    mode: "swap" | "shiftFollowing",
  ) => Promise<void>;
  readonly readOnly: boolean;
}) {
  const videoRef = useRef<HTMLVideoElement>(null);
  const [promptLanguage, setPromptLanguage] = useState<"zh" | "en">("zh");
  const [editingOrder, setEditingOrder] = useState(false);
  const [targetOrder, setTargetOrder] = useState(
    String(asset.displayOrder ?? ""),
  );
  const [orderMode, setOrderMode] = useState<"swap" | "shiftFollowing">("swap");
  const [savingOrder, setSavingOrder] = useState(false);
  useEffect(() => {
    return () => {
      const video = videoRef.current;
      if (video) {
        video.pause();
        video.removeAttribute("src");
        video.load();
      }
    };
  }, []);
  async function copy(value: string) {
    await service.copyText(value);
    onNotice(text.library.detail.copySuccess);
  }
  async function saveOrder() {
    const parsed = Number(targetOrder);
    if (!onUpdateOrder || !Number.isSafeInteger(parsed) || parsed < 1) return;
    setSavingOrder(true);
    try {
      await onUpdateOrder(parsed, orderMode);
      setEditingOrder(false);
    } finally {
      setSavingOrder(false);
    }
  }
  const jsonSummary = JSON.stringify(
    {
      title: asset.title,
      mediaType: asset.mediaType,
      model: asset.modelName,
      platform: asset.platformName,
      promptZh: asset.promptZh,
      promptEn: asset.promptEn,
      negativePrompt: asset.negativePrompt,
      generationParams: JSON.parse(asset.generationParamsJson || "{}"),
    },
    null,
    2,
  );
  const markdownSummary = [
    `# ${asset.title || asset.fileName}`,
    `- ${text.library.detail.model}: ${asset.modelName || "—"}`,
    `- ${text.library.detail.platform}: ${asset.platformName || "—"}`,
    "",
    asset.promptZh || asset.promptEn || "—",
  ].join("\n");
  return (
    <aside
      className="detail-drawer"
      role="dialog"
      aria-modal="true"
      aria-labelledby="asset-detail-title"
    >
      <header className="detail-header">
        <div>
          <p className="eyebrow">{text.library.detail.title}</p>
          <h2 id="asset-detail-title">{asset.title || asset.fileName}</h2>
        </div>
        <button
          className="icon-button"
          type="button"
          aria-label={text.library.form.close}
          onClick={onClose}
        >
          ×
        </button>
      </header>
      <div className="detail-preview">
        {asset.previewUrl ? (
          asset.mediaType === "image" ? (
            <img src={asset.previewUrl} alt={asset.title} />
          ) : (
            <video
              ref={videoRef}
              src={asset.previewUrl}
              controls
              preload="metadata"
            />
          )
        ) : (
          <span>
            {asset.previewError ?? text.library.detail.previewUnavailable}
          </span>
        )}
      </div>
      <div className="detail-body">
        <section>
          <h3>{text.library.detail.metadata}</h3>
          <dl>
            <div>
              <dt>{text.library.detail.assetId}</dt>
              <dd>{asset.displayOrder ?? asset.id}</dd>
            </div>
            <div>
              <dt>最后修改日期</dt>
              <dd>
                {new Intl.DateTimeFormat("zh-CN", {
                  dateStyle: "medium",
                  timeStyle: "short",
                }).format(new Date(asset.updatedAt))}
              </dd>
            </div>
            <div>
              <dt>{text.library.detail.model}</dt>
              <dd>{asset.modelName || "—"}</dd>
            </div>
            <div>
              <dt>{text.library.detail.platform}</dt>
              <dd>{asset.platformName || "—"}</dd>
            </div>
            <div>
              <dt>{text.library.detail.dimensions}</dt>
              <dd>
                {asset.width && asset.height
                  ? `${asset.width} × ${asset.height}`
                  : "—"}
              </dd>
            </div>
            {asset.mediaType === "video" ? (
              <div>
                <dt>{text.library.detail.duration}</dt>
                <dd>
                  {asset.durationMs
                    ? `${(asset.durationMs / 1000).toFixed(1)} s`
                    : "—"}
                </dd>
              </div>
            ) : null}
          </dl>
        </section>
        <section>
          <h3>{text.library.detail.prompts}</h3>
          <div
            className="prompt-language-toggle"
            role="group"
            aria-label={text.library.detail.prompts}
          >
            <button
              type="button"
              aria-pressed={promptLanguage === "zh"}
              onClick={() => setPromptLanguage("zh")}
            >
              中文
            </button>
            {asset.promptEn ? (
              <button
                type="button"
                aria-pressed={promptLanguage === "en"}
                onClick={() => setPromptLanguage("en")}
              >
                English
              </button>
            ) : null}
          </div>
          <p className="prompt-preview">
            {promptLanguage === "zh"
              ? asset.promptZh || "—"
              : asset.promptEn || "—"}
          </p>
          <div className="copy-actions">
            <button
              type="button"
              onClick={() => void copy(asset.promptZh)}
              disabled={!asset.promptZh}
            >
              {text.library.detail.copyZh}
            </button>
            <button
              type="button"
              onClick={() => void copy(asset.promptEn)}
              disabled={!asset.promptEn}
            >
              {text.library.detail.copyEn}
            </button>
            <button
              type="button"
              onClick={() => void copy(asset.negativePrompt)}
              disabled={!asset.negativePrompt}
            >
              {text.library.detail.copyNegative}
            </button>
            <button
              type="button"
              onClick={() => void copy(asset.generationParamsJson)}
              disabled={!asset.generationParamsJson}
            >
              {text.library.detail.copyParams}
            </button>
            {!readOnly ? (
              <button type="button" onClick={() => void copy(asset.storedPath)}>
                {text.library.detail.copyPath}
              </button>
            ) : null}
            <button type="button" onClick={() => void copy(jsonSummary)}>
              {text.library.detail.copyJson}
            </button>
            <button type="button" onClick={() => void copy(markdownSummary)}>
              {text.library.detail.copyMarkdown}
            </button>
          </div>
        </section>
        <section>
          <h3>{text.library.detail.notes}</h3>
          <p className="prompt-preview">{asset.notes || "—"}</p>
        </section>
        {!readOnly && onUpdateOrder ? (
          <section>
            <h3>{text.library.detail.assetId}</h3>
            {editingOrder ? (
              <div className="asset-order-editor">
                <label className="form-field">
                  <span>{text.library.detail.targetOrder}</span>
                  <input
                    type="number"
                    min="1"
                    value={targetOrder}
                    onChange={(event) => setTargetOrder(event.target.value)}
                  />
                </label>
                <label>
                  <input
                    type="radio"
                    checked={orderMode === "swap"}
                    onChange={() => setOrderMode("swap")}
                  />
                  {text.library.detail.orderSwap}
                </label>
                <label>
                  <input
                    type="radio"
                    checked={orderMode === "shiftFollowing"}
                    onChange={() => setOrderMode("shiftFollowing")}
                  />
                  {text.library.detail.orderShift}
                </label>
                <div className="dialog-actions">
                  <button
                    type="button"
                    className="secondary-button"
                    onClick={() => setEditingOrder(false)}
                  >
                    {text.library.form.cancel}
                  </button>
                  <button
                    type="button"
                    className="primary-button"
                    disabled={savingOrder}
                    onClick={() => void saveOrder()}
                  >
                    {text.library.detail.orderSave}
                  </button>
                </div>
              </div>
            ) : (
              <button
                type="button"
                className="secondary-button"
                onClick={() => {
                  setTargetOrder(String(asset.displayOrder ?? ""));
                  setEditingOrder(true);
                }}
              >
                {text.library.detail.editOrder}
              </button>
            )}
          </section>
        ) : null}
      </div>
      {!readOnly ? (
        <footer className="detail-actions">
          <button
            className="danger-text-button"
            type="button"
            onClick={onTrash}
          >
            {text.library.detail.delete}
          </button>
          {onManageVideoCover ? (
            <button
              className="secondary-button"
              type="button"
              onClick={onManageVideoCover}
            >
              {text.library.detail.manageVideoCover}
            </button>
          ) : null}
          <button className="primary-button" type="button" onClick={onEdit}>
            {text.library.detail.edit}
          </button>
        </footer>
      ) : null}
    </aside>
  );
}
