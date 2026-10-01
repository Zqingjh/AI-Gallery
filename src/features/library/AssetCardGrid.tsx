import { useEffect, useState } from "react";
import { text } from "../../app/texts";
import type {
  AssetSummary,
  LibraryService,
} from "../../services/library-service";

function createThumbnailQueue() {
  let active = 0;
  const disposedServices = new WeakSet<LibraryService>();
  const pending: Array<{
    service: LibraryService;
    start: () => void;
    cancel: () => void;
  }> = [];
  const maxPending = 32;
  const run = async <T,>(
    service: LibraryService,
    work: () => Promise<T>,
  ): Promise<T | null> => {
    if (disposedServices.has(service)) return null;
    let admitted = false;
    await new Promise<void>((resolve) => {
      const start = () => {
        if (disposedServices.has(service)) {
          resolve();
          return;
        }
        admitted = true;
        active += 1;
        resolve();
      };
      if (active < 2) start();
      else if (pending.length < maxPending) {
        pending.push({ service, start, cancel: resolve });
      } else resolve();
    });
    if (!admitted) return null;
    try {
      return await work();
    } finally {
      active -= 1;
      pending.shift()?.start();
    }
  };
  return {
    run,
    dispose(service: LibraryService) {
      disposedServices.add(service);
      for (let index = pending.length - 1; index >= 0; index -= 1) {
        if (pending[index].service !== service) continue;
        pending.splice(index, 1)[0].cancel();
      }
    },
  };
}

const thumbnailQueue = createThumbnailQueue();

/** 切换列表或详情页时复用缩略图，避免重复等待 IPC 与 Blob 创建。 */
function createThumbnailPreviewCache() {
  const entries = new Map<string, { revision: string; url: string }>();
  const capacity = 120;

  function release(url: string) {
    if (url.startsWith("blob:")) URL.revokeObjectURL(url);
  }

  return {
    get(assetId: string, revision: string): string | null {
      const value = entries.get(assetId);
      if (!value) return null;
      if (value.revision !== revision) {
        entries.delete(assetId);
        release(value.url);
        return null;
      }
      entries.delete(assetId);
      entries.set(assetId, value);
      return value.url;
    },
    set(assetId: string, revision: string, url: string) {
      const previous = entries.get(assetId);
      if (previous && previous.url !== url) release(previous.url);
      entries.delete(assetId);
      entries.set(assetId, { revision, url });
      while (entries.size > capacity) {
        const oldest = entries.entries().next().value as
          [string, { revision: string; url: string }] | undefined;
        if (!oldest) break;
        entries.delete(oldest[0]);
        release(oldest[1].url);
      }
    },
    invalidate(assetIds: ReadonlySet<string>) {
      for (const assetId of assetIds) {
        const value = entries.get(assetId);
        if (!value) continue;
        entries.delete(assetId);
        release(value.url);
      }
    },
    dispose() {
      entries.forEach(({ url }) => release(url));
      entries.clear();
    },
  };
}

const thumbnailPreviewCaches = new WeakMap<
  LibraryService,
  ReturnType<typeof createThumbnailPreviewCache>
>();

function thumbnailPreviewCacheFor(service: LibraryService) {
  let cache = thumbnailPreviewCaches.get(service);
  if (!cache) {
    cache = createThumbnailPreviewCache();
    thumbnailPreviewCaches.set(service, cache);
  }
  return cache;
}

export function disposeAssetPreviewSession(service: LibraryService) {
  thumbnailQueue.dispose(service);
  thumbnailPreviewCaches.get(service)?.dispose();
  thumbnailPreviewCaches.delete(service);
}

/** 作品进入回收站后立即释放缓存，避免编号或数据库 ID 复用旧预览。 */
export function invalidateAssetPreviewCache(
  service: LibraryService,
  assetIds: readonly string[],
) {
  thumbnailPreviewCaches.get(service)?.invalidate(new Set(assetIds));
}

export function AssetCardGrid({
  assets,
  service,
  onOpen,
  selectedIds = new Set<string>(),
  onToggleSelection,
}: {
  readonly assets: readonly AssetSummary[];
  readonly service: LibraryService;
  readonly onOpen: (asset: AssetSummary) => void;
  readonly selectedIds?: ReadonlySet<string>;
  readonly onToggleSelection?: (assetId: string) => void;
}) {
  return (
    <section className="asset-grid" aria-label={text.library.tabs.assets}>
      {assets.map((asset) => (
        <AssetCard
          asset={asset}
          service={service}
          onOpen={onOpen}
          selected={selectedIds.has(asset.id)}
          onToggleSelection={onToggleSelection}
          key={asset.id}
        />
      ))}
    </section>
  );
}

function AssetCard({
  asset,
  service,
  onOpen,
  selected,
  onToggleSelection,
}: {
  readonly asset: AssetSummary;
  readonly service: LibraryService;
  readonly onOpen: (asset: AssetSummary) => void;
  readonly selected: boolean;
  readonly onToggleSelection?: (assetId: string) => void;
}) {
  return (
    <article className="asset-card">
      {onToggleSelection ? (
        <label className="asset-card-select">
          <input
            type="checkbox"
            checked={selected}
            onChange={() => onToggleSelection(asset.id)}
            aria-label={`选择作品：${asset.fileName}`}
          />
        </label>
      ) : null}
      <button
        type="button"
        className="asset-card-open"
        onClick={() => onOpen(asset)}
        aria-label={`${text.library.openDetail}：${asset.fileName}`}
      >
        <AssetThumbnailPreview asset={asset} service={service} />
        <div className="asset-card-copy">
          <h2>
            {asset.displayOrder ? `#${asset.displayOrder} · ` : ""}
            {asset.fileName || text.library.untitled}
          </h2>
          <p>
            {asset.modelName || text.library.unknownModel} ·{" "}
            {asset.platformName || text.library.unknownPlatform}
          </p>
          <small>
            {asset.width && asset.height
              ? `${asset.width} × ${asset.height}`
              : "—"}
            {asset.isFavorite ? " · ★" : ""}
          </small>
        </div>
      </button>
    </article>
  );
}

/** 供画布成员卡片复用的按需缩略图，共享有界队列与缓存。 */
export function AssetThumbnailPreview({
  asset,
  service,
}: {
  readonly asset: AssetSummary;
  readonly service: LibraryService;
}) {
  const initialThumbnail = asset.thumbnailUrl ?? asset.coverUrl ?? null;
  const thumbnailRevision = `${asset.updatedAt}\u0000${asset.fileName}\u0000${asset.mediaType}`;
  const [thumbnail, setThumbnail] = useState<string | null>(initialThumbnail);
  const [thumbnailFailed, setThumbnailFailed] = useState(false);

  useEffect(() => {
    let active = true;
    let retryTimer: number | null = null;
    let attempts = 0;
    const thumbnailPreviewCache = thumbnailPreviewCacheFor(service);
    const cachedThumbnail = thumbnailPreviewCache.get(
      asset.id,
      thumbnailRevision,
    );
    const visibleThumbnail = initialThumbnail ?? cachedThumbnail;
    setThumbnail(visibleThumbnail);
    setThumbnailFailed(false);
    const requestThumbnail = () => {
      void thumbnailQueue
        .run(service, () => service.getAssetThumbnail(asset.id))
        .then((url) => {
          if (!active) {
            if (url?.startsWith("blob:")) URL.revokeObjectURL(url);
            return;
          }
          if (url) thumbnailPreviewCache.set(asset.id, thumbnailRevision, url);
          setThumbnail(url);
          if (!url && attempts < 12) {
            attempts += 1;
            retryTimer = window.setTimeout(requestThumbnail, 250);
          }
        })
        .catch(() => {
          if (active) {
            setThumbnail(null);
            setThumbnailFailed(true);
          }
        });
    };
    if (!visibleThumbnail) requestThumbnail();
    return () => {
      active = false;
      if (retryTimer !== null) window.clearTimeout(retryTimer);
    };
  }, [
    asset.id,
    asset.mediaType,
    asset.updatedAt,
    asset.fileName,
    initialThumbnail,
    service,
    thumbnailRevision,
  ]);

  return (
    <div className="asset-thumbnail">
      {thumbnail ? (
        <img src={thumbnail} alt="" />
      ) : (
        <span>
          {thumbnailFailed
            ? text.library.thumbnailUnavailable
            : asset.mediaType === "video"
              ? text.library.noVideoCover
              : text.library.noThumbnail}
        </span>
      )}
      <b>
        {asset.mediaType === "image" ? text.library.image : text.library.video}
      </b>
    </div>
  );
}
