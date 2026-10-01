import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { StrictMode } from "react";
import { describe, expect, it, vi } from "vitest";
import MediaIntegrityPanel from "../src/features/p1/MediaIntegrityPanel";
import type { MediaIntegrityService } from "../src/services/media-integrity-service";

function createService(
  overrides: Partial<MediaIntegrityService> = {},
): MediaIntegrityService {
  return {
    listDuplicateGroups: vi.fn().mockResolvedValue({
      items: [],
      nextCursor: null,
    }),
    getAssetCover: vi.fn().mockResolvedValue(null),
    selectFfmpegExecutable: vi.fn().mockResolvedValue({
      path: "D:\\private\\ffmpeg.exe",
      fileName: "ffmpeg.exe",
    }),
    selectVideoFile: vi.fn().mockResolvedValue({
      path: "D:\\private\\video.mp4",
      fileName: "video.mp4",
    }),
    selectCustomCover: vi.fn().mockResolvedValue(null),
    selectRepairCandidate: vi.fn().mockResolvedValue({
      path: "D:\\private\\relinked.mp4",
      fileName: "relinked.mp4",
    }),
    previewVideoKeyFrame: vi.fn().mockResolvedValue({
      assetId: 9,
      frameTimestampMs: 0,
      previewUrl: null,
      dispose: vi.fn(),
    }),
    setVideoKeyFrameCover: vi.fn().mockResolvedValue({
      assetId: 9,
      sourceType: "keyFrame",
      frameTimestampMs: 0,
      previewUrl: null,
      dispose: vi.fn(),
    }),
    setCustomVideoCover: vi.fn(),
    removeVideoCover: vi.fn().mockResolvedValue(undefined),
    previewAssetPathRepair: vi.fn().mockResolvedValue({
      assetId: 9,
      fileName: "relinked.mp4",
      pathKind: "external",
      fileSize: 1024,
      verification: "unverified",
    }),
    executeAssetPathRepair: vi.fn().mockResolvedValue({
      assetId: 9,
      fileName: "relinked.mp4",
      pathKind: "external",
      fileSize: 1024,
      verification: "unverified",
    }),
    ...overrides,
  };
}

async function openPanel(service: MediaIntegrityService) {
  render(<MediaIntegrityPanel service={service} />);
  fireEvent.click(screen.getByRole("button", { name: "打开媒体完整性工具" }));
  await waitFor(() => expect(service.listDuplicateGroups).toHaveBeenCalled());
  fireEvent.change(screen.getByLabelText("媒体完整性作品编号"), {
    target: { value: "9" },
  });
}

describe("媒体完整性面板", () => {
  it("从视频详情进入时自动展开并带入作品编号", () => {
    render(
      <MediaIntegrityPanel service={createService()} initialAssetId="44" />,
    );

    expect(
      screen.getByRole("button", { name: "收起媒体完整性工具" }),
    ).toBeInTheDocument();
    expect(screen.getByLabelText("媒体完整性作品编号")).toHaveValue("44");
  });

  it("设置关键帧封面前明确确认，且页面不显示绝对路径", async () => {
    const service = createService();
    vi.spyOn(window, "confirm")
      .mockReturnValueOnce(false)
      .mockReturnValue(true);
    await openPanel(service);
    fireEvent.click(screen.getByRole("button", { name: "选择 FFmpeg" }));
    fireEvent.click(screen.getByRole("button", { name: "选择视频文件" }));
    await screen.findByText("ffmpeg.exe");
    await screen.findByText("video.mp4");

    fireEvent.click(screen.getByRole("button", { name: "确认设置关键帧封面" }));
    expect(service.setVideoKeyFrameCover).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "确认设置关键帧封面" }));
    await waitFor(() =>
      expect(service.setVideoKeyFrameCover).toHaveBeenCalledWith({
        assetId: 9,
        frameTimestampMs: 0,
        ffmpegPath: "D:\\private\\ffmpeg.exe",
        videoPath: "D:\\private\\video.mp4",
      }),
    );
    expect(document.body).not.toHaveTextContent("D:\\private");
  });

  it("未验证路径必须经过二次确认", async () => {
    const service = createService();
    vi.spyOn(window, "confirm")
      .mockReturnValueOnce(true)
      .mockReturnValueOnce(false)
      .mockReturnValueOnce(true)
      .mockReturnValueOnce(true);
    await openPanel(service);
    fireEvent.click(screen.getByRole("button", { name: "选择文件并预览修复" }));
    expect(await screen.findByText(/未验证/)).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "确认执行路径修复" }));
    expect(service.executeAssetPathRepair).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "确认执行路径修复" }));
    await waitFor(() =>
      expect(service.executeAssetPathRepair).toHaveBeenCalledWith({
        assetId: 9,
        candidatePath: "D:\\private\\relinked.mp4",
        allowUnverified: true,
      }),
    );
    expect(window.confirm).toHaveBeenCalledTimes(4);
  });

  it("卸载期间返回的封面预览会立即释放对象 URL", async () => {
    let resolveCover!: (
      value: Awaited<ReturnType<MediaIntegrityService["getAssetCover"]>>,
    ) => void;
    const dispose = vi.fn();
    const service = createService({
      getAssetCover: vi.fn().mockReturnValue(
        new Promise((resolve) => {
          resolveCover = resolve;
        }),
      ),
    });
    const view = render(<MediaIntegrityPanel service={service} />);
    fireEvent.click(screen.getByRole("button", { name: "打开媒体完整性工具" }));
    fireEvent.change(await screen.findByLabelText("媒体完整性作品编号"), {
      target: { value: "9" },
    });
    fireEvent.click(screen.getByRole("button", { name: "刷新封面状态" }));
    await waitFor(() => expect(service.getAssetCover).toHaveBeenCalledWith(9));
    view.unmount();
    await act(async () => {
      resolveCover({
        assetId: 9,
        sourceType: "custom",
        frameTimestampMs: null,
        previewUrl: "blob:late-cover",
        dispose,
      });
      await Promise.resolve();
    });
    expect(dispose).toHaveBeenCalledOnce();
  });

  it("StrictMode 重建 effect 后仍会展示异步返回的封面", async () => {
    const dispose = vi.fn();
    const service = createService({
      getAssetCover: vi.fn().mockResolvedValue({
        assetId: 9,
        sourceType: "custom",
        frameTimestampMs: null,
        previewUrl: "blob:strict-cover",
        dispose,
      }),
    });
    render(
      <StrictMode>
        <MediaIntegrityPanel service={service} />
      </StrictMode>,
    );
    fireEvent.click(screen.getByRole("button", { name: "打开媒体完整性工具" }));
    fireEvent.change(await screen.findByLabelText("媒体完整性作品编号"), {
      target: { value: "9" },
    });
    fireEvent.click(screen.getByRole("button", { name: "刷新封面状态" }));

    expect(await screen.findByText(/当前封面：自定义图片/)).toBeInTheDocument();
    expect(dispose).not.toHaveBeenCalled();
  });
});
