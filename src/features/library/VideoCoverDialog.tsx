import { useEffect, useRef, useState } from "react";
import type {
  AssetCover,
  MediaIntegrityService,
} from "../../services/media-integrity-service";
import { LibraryDialog } from "./LibraryDialog";

export function VideoCoverDialog({
  assetId,
  service,
  onClose,
  onSaved,
}: {
  readonly assetId: number;
  readonly service: MediaIntegrityService;
  readonly onClose: () => void;
  readonly onSaved: () => void;
}) {
  const [cover, setCover] = useState<AssetCover | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const coverRef = useRef<AssetCover | null>(null);

  useEffect(() => {
    let active = true;
    void service
      .getAssetCover(assetId)
      .then((value) => {
        if (!active) {
          value?.dispose();
          return;
        }
        coverRef.current?.dispose();
        coverRef.current = value;
        setCover(value);
      })
      .catch(() => {
        if (active) setError("无法读取当前视频封面。");
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
      coverRef.current?.dispose();
      coverRef.current = null;
    };
  }, [assetId, service]);

  async function apply(action: () => Promise<AssetCover>) {
    setSaving(true);
    setError(null);
    try {
      const next = await action();
      coverRef.current?.dispose();
      coverRef.current = next;
      setCover(next);
      onSaved();
    } catch {
      setError("设置视频封面失败，请确认视频文件和 FFmpeg 配置可用。");
    } finally {
      setSaving(false);
    }
  }

  async function uploadCover() {
    const selected = await service.selectCustomCover();
    if (!selected) return;
    await apply(() =>
      service.setCustomVideoCover({ assetId, coverPath: selected.path }),
    );
  }

  async function generateCover() {
    if (!service.generateDefaultVideoCover) {
      setError("当前应用版本未配置自动生成视频封面的能力。");
      return;
    }
    await apply(() => service.generateDefaultVideoCover!(assetId));
  }

  return (
    <LibraryDialog
      title="设置视频封面"
      description={
        loading
          ? "正在检测已有封面…"
          : cover
            ? "已检测到封面；可保留当前封面，或替换为新的封面。"
            : "未检测到封面。请选择自动生成首帧，或上传一张封面图片。"
      }
      onClose={onClose}
    >
      {cover?.previewUrl ? (
        <img
          className="video-cover-preview"
          src={cover.previewUrl}
          alt="当前视频封面"
        />
      ) : null}
      <div className="dialog-actions">
        <button
          className="secondary-button"
          type="button"
          disabled={loading || saving}
          onClick={() => void generateCover()}
        >
          调用已配置的 FFmpeg 自动生成
        </button>
        <button
          className="primary-button"
          type="button"
          disabled={loading || saving}
          onClick={() => void uploadCover()}
        >
          上传封面
        </button>
      </div>
      {error ? (
        <p className="form-error" role="alert">
          {error}
        </p>
      ) : null}
    </LibraryDialog>
  );
}
