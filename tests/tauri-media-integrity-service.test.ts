import { describe, expect, it, vi } from "vitest";
import type { CommandClient } from "../src/services/command-client";
import { createTauriMediaIntegrityService } from "../src/services/tauri-media-integrity-service";

function setup() {
  const invoke = vi.fn();
  const client: CommandClient = { invoke };
  return {
    invoke,
    service: createTauriMediaIntegrityService(client, () => "D:\\Library"),
  };
}

describe("Tauri 媒体完整性服务", () => {
  it("复用现有重复组命令并映射游标", async () => {
    const { invoke, service } = setup();
    invoke.mockResolvedValue({
      items: [
        {
          contentHash: "abc:def",
          assetCount: 2,
          representativeAssetId: 7,
          representativeFileName: "cover.png",
          updatedAt: 1000,
        },
      ],
      nextCursor: { updatedAt: 1000, contentHash: "abc:def" },
    });

    const page = await service.listDuplicateGroups({ limit: 24 });

    expect(page.items[0]).toMatchObject({
      representativeAssetId: "7",
      assetCount: 2,
    });
    expect(page.nextCursor).toBe("1000:abc%3Adef");
    expect(invoke).toHaveBeenCalledWith("library_list_duplicate_groups", {
      request: { rootPath: "D:\\Library", cursor: undefined, limit: 24 },
    });
  });

  it("所有写操作携带确认标记，修复响应不暴露路径", async () => {
    const { invoke, service } = setup();
    invoke
      .mockResolvedValueOnce({
        assetId: 3,
        sourceType: "custom",
        frameTimestampMs: null,
      })
      .mockResolvedValueOnce({
        assetId: 3,
        fileName: "relinked.mp4",
        pathKind: "external",
        fileSize: 2048,
        verification: "unverified",
        storedPath: "D:\\private\\relinked.mp4",
      });

    await service.setCustomVideoCover({
      assetId: 3,
      coverPath: "D:\\private\\cover.png",
    });
    const repaired = await service.executeAssetPathRepair({
      assetId: 3,
      candidatePath: "D:\\private\\relinked.mp4",
      allowUnverified: true,
    });

    expect(invoke.mock.calls[0]).toEqual([
      "set_custom_video_cover",
      {
        request: {
          rootPath: "D:\\Library",
          assetId: 3,
          coverPath: "D:\\private\\cover.png",
          confirmed: true,
        },
      },
    ]);
    expect(invoke.mock.calls[1]?.[1]).toMatchObject({
      request: { confirmed: true, allowUnverified: true },
    });
    expect(repaired).toEqual({
      assetId: 3,
      fileName: "relinked.mp4",
      pathKind: "external",
      fileSize: 2048,
      verification: "unverified",
    });
    expect(repaired).not.toHaveProperty("storedPath");
  });

  it("拒绝越界预览字节", async () => {
    const { invoke, service } = setup();
    invoke.mockResolvedValue({
      assetId: 3,
      frameTimestampMs: 0,
      mimeType: "image/png",
      bytes: [0, 256],
    });
    await expect(
      service.previewVideoKeyFrame({
        assetId: 3,
        frameTimestampMs: 0,
        ffmpegPath: "ffmpeg.exe",
        videoPath: "video.mp4",
      }),
    ).rejects.toMatchObject({ code: "INVALID_RESPONSE" });
  });
});
