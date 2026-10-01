import { useEffect, useRef, useState } from "react";
import type {
  AssetCover,
  AssetPathRepairPreview,
  MediaIntegrityService,
  SelectedLocalFile,
  VideoKeyFramePreview,
} from "../../services/media-integrity-service";
import type { DuplicateAssetGroup } from "../../services/library-service";

export interface MediaIntegrityPanelProps {
  readonly service: MediaIntegrityService;
  readonly readOnly?: boolean;
  readonly initialAssetId?: string | null;
}

function parseAssetId(value: string): number | null {
  const parsed = Number(value);
  return Number.isSafeInteger(parsed) && parsed > 0 ? parsed : null;
}

function parseTimestamp(value: string): number | null {
  const parsed = Number(value);
  return Number.isSafeInteger(parsed) && parsed >= 0 ? parsed : null;
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : "操作失败，请稍后重试。";
}

function fileSizeLabel(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

export function MediaIntegrityPanel({
  service,
  readOnly = false,
  initialAssetId,
}: MediaIntegrityPanelProps) {
  const [open, setOpen] = useState(Boolean(initialAssetId));
  const [duplicates, setDuplicates] = useState<readonly DuplicateAssetGroup[]>(
    [],
  );
  const [duplicateCursor, setDuplicateCursor] = useState<string | null>(null);
  const [assetIdText, setAssetIdText] = useState(initialAssetId ?? "");
  const [timestampText, setTimestampText] = useState("0");
  const [cover, setCover] = useState<AssetCover | null>(null);
  const [keyFramePreview, setKeyFramePreview] =
    useState<VideoKeyFramePreview | null>(null);
  const [ffmpeg, setFfmpeg] = useState<SelectedLocalFile | null>(null);
  const [video, setVideo] = useState<SelectedLocalFile | null>(null);
  const [customCover, setCustomCover] = useState<SelectedLocalFile | null>(
    null,
  );
  const [repairCandidate, setRepairCandidate] =
    useState<SelectedLocalFile | null>(null);
  const [repairPreview, setRepairPreview] =
    useState<AssetPathRepairPreview | null>(null);
  const [status, setStatus] = useState("");
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);
  const mounted = useRef(true);
  const coverRequest = useRef(0);
  const keyFrameRequest = useRef(0);

  useEffect(() => () => cover?.dispose(), [cover]);
  useEffect(() => () => keyFramePreview?.dispose(), [keyFramePreview]);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      coverRequest.current += 1;
      keyFrameRequest.current += 1;
    };
  }, []);
  useEffect(() => {
    if (!initialAssetId) return;
    setOpen(true);
    setAssetIdText(initialAssetId);
  }, [initialAssetId]);

  const run = async (operation: () => Promise<void>) => {
    setError("");
    setStatus("");
    setLoading(true);
    try {
      await operation();
    } catch (caught) {
      setError(errorMessage(caught));
    } finally {
      setLoading(false);
    }
  };

  const requireAssetId = (): number | null => {
    const assetId = parseAssetId(assetIdText);
    if (assetId === null) setError("请输入有效的作品编号。");
    return assetId;
  };

  const loadDuplicateGroups = async (append = false) => {
    await run(async () => {
      const page = await service.listDuplicateGroups({
        limit: 24,
        ...(append && duplicateCursor ? { cursor: duplicateCursor } : {}),
      });
      setDuplicates((current) =>
        append ? [...current, ...page.items] : page.items,
      );
      setDuplicateCursor(page.nextCursor);
    });
  };

  const toggleOpen = () => {
    const next = !open;
    setOpen(next);
    if (!next) {
      coverRequest.current += 1;
      keyFrameRequest.current += 1;
      setCover(null);
      setKeyFramePreview(null);
    }
    if (next && duplicates.length === 0) void loadDuplicateGroups();
  };

  const loadCover = async () => {
    const assetId = requireAssetId();
    if (assetId === null) return;
    await run(async () => {
      const requestId = ++coverRequest.current;
      const nextCover = await service.getAssetCover(assetId);
      if (!mounted.current || requestId !== coverRequest.current) {
        nextCover?.dispose();
        return;
      }
      setCover(nextCover);
      setStatus("封面状态已刷新。");
    });
  };

  const chooseFfmpeg = async () => {
    await run(async () => {
      const selected = await service.selectFfmpegExecutable();
      if (selected) setFfmpeg(selected);
    });
  };

  const chooseVideo = async () => {
    await run(async () => {
      const selected = await service.selectVideoFile();
      if (selected) setVideo(selected);
    });
  };

  const previewKeyFrame = async () => {
    const assetId = requireAssetId();
    const frameTimestampMs = parseTimestamp(timestampText);
    if (assetId === null) return;
    if (frameTimestampMs === null) {
      setError("关键帧时间必须是非负整数毫秒。");
      return;
    }
    if (!ffmpeg || !video) {
      setError("请先选择 FFmpeg 和视频文件。");
      return;
    }
    await run(async () => {
      const requestId = ++keyFrameRequest.current;
      const preview = await service.previewVideoKeyFrame({
        assetId,
        frameTimestampMs,
        ffmpegPath: ffmpeg.path,
        videoPath: video.path,
      });
      if (!mounted.current || requestId !== keyFrameRequest.current) {
        preview.dispose();
        return;
      }
      setKeyFramePreview(preview);
      setStatus("关键帧预览已生成，尚未修改封面。");
    });
  };

  const setKeyFrameCover = async () => {
    const assetId = requireAssetId();
    const frameTimestampMs = parseTimestamp(timestampText);
    if (assetId === null || frameTimestampMs === null || !ffmpeg || !video) {
      setError("请先完成关键帧预览所需的输入。");
      return;
    }
    if (!window.confirm("确认用当前关键帧设置或替换视频封面？")) return;
    await run(async () => {
      const requestId = ++coverRequest.current;
      const nextCover = await service.setVideoKeyFrameCover({
        assetId,
        frameTimestampMs,
        ffmpegPath: ffmpeg.path,
        videoPath: video.path,
      });
      if (!mounted.current || requestId !== coverRequest.current) {
        nextCover.dispose();
        return;
      }
      setCover(nextCover);
      setStatus("视频封面已更新。");
    });
  };

  const chooseCustomCover = async () => {
    await run(async () => {
      const selected = await service.selectCustomCover();
      if (selected) setCustomCover(selected);
    });
  };

  const applyCustomCover = async () => {
    const assetId = requireAssetId();
    if (assetId === null) return;
    if (!customCover) {
      setError("请先选择自定义封面。");
      return;
    }
    if (!window.confirm("确认设置或替换为所选自定义封面？")) return;
    await run(async () => {
      const requestId = ++coverRequest.current;
      const nextCover = await service.setCustomVideoCover({
        assetId,
        coverPath: customCover.path,
      });
      if (!mounted.current || requestId !== coverRequest.current) {
        nextCover.dispose();
        return;
      }
      setCover(nextCover);
      setStatus("自定义封面已更新。");
    });
  };

  const removeCover = async () => {
    const assetId = requireAssetId();
    if (assetId === null) return;
    if (!window.confirm("确认移除当前视频封面？原始视频不会被删除。")) return;
    await run(async () => {
      await service.removeVideoCover(assetId);
      setCover(null);
      setStatus("视频封面已移除，原始视频未受影响。");
    });
  };

  const chooseRepairCandidate = async () => {
    const assetId = requireAssetId();
    if (assetId === null) return;
    await run(async () => {
      const selected = await service.selectRepairCandidate();
      if (!selected) return;
      setRepairCandidate(selected);
      setRepairPreview(
        await service.previewAssetPathRepair({
          assetId,
          candidatePath: selected.path,
        }),
      );
      setStatus("修复影响已预览，尚未写入。");
    });
  };

  const executeRepair = async () => {
    if (!repairPreview || !repairCandidate) {
      setError("请先选择文件并预览修复影响。");
      return;
    }
    if (
      !window.confirm("确认将作品记录重新连接到所选文件？不会移动或删除文件。")
    ) {
      return;
    }
    const allowUnverified = repairPreview.verification === "unverified";
    if (
      allowUnverified &&
      !window.confirm(
        "该作品没有可核对的历史哈希，文件身份未验证。仍要写入新路径并保存当前哈希吗？",
      )
    ) {
      return;
    }
    await run(async () => {
      const result = await service.executeAssetPathRepair({
        assetId: repairPreview.assetId,
        candidatePath: repairCandidate.path,
        allowUnverified,
      });
      setRepairPreview(result);
      setStatus("作品路径已修复，文件未被移动或删除。");
    });
  };

  return (
    <section className="management-panel" aria-labelledby="integrity-title">
      <header>
        <div>
          <h2 id="integrity-title">媒体完整性</h2>
          <p>按需检查重复内容、视频封面和失效路径，不会自动删除或合并媒体。</p>
        </div>
        <button
          className="secondary-button"
          type="button"
          aria-expanded={open}
          onClick={toggleOpen}
        >
          {open ? "收起媒体完整性工具" : "打开媒体完整性工具"}
        </button>
      </header>

      {open ? (
        <>
          <section aria-labelledby="duplicate-groups-title">
            <h3 id="duplicate-groups-title">完全重复内容</h3>
            {duplicates.length === 0 ? (
              <p className="management-empty">未发现完全重复组。</p>
            ) : (
              <ul>
                {duplicates.map((group) => (
                  <li key={group.contentHash}>
                    <strong>{group.representativeFileName}</strong> · 共{" "}
                    {group.assetCount} 个完全相同文件
                  </li>
                ))}
              </ul>
            )}
            {duplicateCursor ? (
              <button
                className="secondary-button"
                type="button"
                onClick={() => void loadDuplicateGroups(true)}
              >
                加载更多重复组
              </button>
            ) : null}
          </section>

          <section aria-labelledby="cover-management-title">
            <h3 id="cover-management-title">视频封面</h3>
            <label className="form-field">
              <span>媒体完整性作品编号</span>
              <input
                inputMode="numeric"
                value={assetIdText}
                onChange={(event) => {
                  coverRequest.current += 1;
                  keyFrameRequest.current += 1;
                  setAssetIdText(event.target.value);
                  setCover(null);
                  setKeyFramePreview(null);
                  setRepairPreview(null);
                  setRepairCandidate(null);
                }}
              />
            </label>
            <button
              className="secondary-button"
              type="button"
              onClick={() => void loadCover()}
            >
              刷新封面状态
            </button>
            {cover ? (
              <div>
                <p>
                  当前封面：
                  {cover.sourceType === "keyFrame"
                    ? "视频关键帧"
                    : "自定义图片"}
                </p>
                {cover.previewUrl ? (
                  <img src={cover.previewUrl} alt="当前视频封面预览" />
                ) : null}
              </div>
            ) : null}

            {!readOnly ? (
              <div className="workspace-management-grid">
                <label className="form-field">
                  <span>关键帧时间（毫秒）</span>
                  <input
                    type="number"
                    min="0"
                    step="1"
                    value={timestampText}
                    onChange={(event) => setTimestampText(event.target.value)}
                  />
                </label>
                <div className="dialog-actions">
                  <button
                    type="button"
                    className="secondary-button"
                    onClick={() => void chooseFfmpeg()}
                  >
                    选择 FFmpeg
                  </button>
                  <span>{ffmpeg?.fileName ?? "尚未选择"}</span>
                  <button
                    type="button"
                    className="secondary-button"
                    onClick={() => void chooseVideo()}
                  >
                    选择视频文件
                  </button>
                  <span>{video?.fileName ?? "尚未选择"}</span>
                </div>
                <div className="dialog-actions">
                  <button
                    type="button"
                    className="secondary-button"
                    onClick={() => void previewKeyFrame()}
                  >
                    预览关键帧
                  </button>
                  <button
                    type="button"
                    className="primary-button"
                    onClick={() => void setKeyFrameCover()}
                  >
                    确认设置关键帧封面
                  </button>
                </div>
                {keyFramePreview?.previewUrl ? (
                  <img src={keyFramePreview.previewUrl} alt="关键帧封面预览" />
                ) : null}
                <div className="dialog-actions">
                  <button
                    type="button"
                    className="secondary-button"
                    onClick={() => void chooseCustomCover()}
                  >
                    选择自定义封面
                  </button>
                  <span>{customCover?.fileName ?? "尚未选择"}</span>
                  <button
                    type="button"
                    className="primary-button"
                    onClick={() => void applyCustomCover()}
                  >
                    确认设置自定义封面
                  </button>
                  <button
                    type="button"
                    className="danger-button"
                    onClick={() => void removeCover()}
                  >
                    移除视频封面
                  </button>
                </div>
              </div>
            ) : null}
          </section>

          {!readOnly ? (
            <section aria-labelledby="path-repair-title">
              <h3 id="path-repair-title">失效路径修复</h3>
              <p>
                只连接用户明确选择的单个文件，不扫描目录，也不移动或删除媒体。
              </p>
              <button
                className="secondary-button"
                type="button"
                onClick={() => void chooseRepairCandidate()}
              >
                选择文件并预览修复
              </button>
              {repairPreview ? (
                <div role="status">
                  <p>候选文件：{repairPreview.fileName}</p>
                  <p>文件大小：{fileSizeLabel(repairPreview.fileSize)}</p>
                  <p>
                    校验状态：
                    {repairPreview.verification === "matched"
                      ? "哈希一致"
                      : "未验证（执行时需要再次确认）"}
                  </p>
                  <button
                    className="primary-button"
                    type="button"
                    onClick={() => void executeRepair()}
                  >
                    确认执行路径修复
                  </button>
                </div>
              ) : null}
            </section>
          ) : null}
        </>
      ) : null}

      {loading ? <p role="status">正在处理…</p> : null}
      {status ? <p role="status">{status}</p> : null}
      {error ? (
        <p className="form-error" role="alert">
          {error}
        </p>
      ) : null}
    </section>
  );
}

export default MediaIntegrityPanel;
