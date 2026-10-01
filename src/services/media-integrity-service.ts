import type {
  DuplicateAssetGroup,
  DuplicateGroupPageRequest,
  PageResult,
} from "./library-service";

export interface SelectedLocalFile {
  /** 仅供 service 作为 IPC 入参，UI 不得展示或记录。 */
  readonly path: string;
  readonly fileName: string;
}

export interface DisposableImagePreview {
  readonly previewUrl: string | null;
  dispose(): void;
}

export interface AssetCover extends DisposableImagePreview {
  readonly assetId: number;
  readonly sourceType: "keyFrame" | "custom";
  readonly frameTimestampMs: number | null;
}

export interface VideoKeyFramePreview extends DisposableImagePreview {
  readonly assetId: number;
  readonly frameTimestampMs: number;
}

export interface AssetPathRepairPreview {
  readonly assetId: number;
  readonly fileName: string;
  readonly pathKind: "managed" | "external";
  readonly fileSize: number;
  readonly verification: "matched" | "unverified";
}

export interface MediaIntegrityService {
  listDuplicateGroups(
    request: DuplicateGroupPageRequest,
  ): Promise<PageResult<DuplicateAssetGroup>>;
  getAssetCover(assetId: number): Promise<AssetCover | null>;

  selectFfmpegExecutable(): Promise<SelectedLocalFile | null>;
  selectVideoFile(): Promise<SelectedLocalFile | null>;
  selectCustomCover(): Promise<SelectedLocalFile | null>;
  selectRepairCandidate(): Promise<SelectedLocalFile | null>;

  previewVideoKeyFrame(input: {
    readonly assetId: number;
    readonly frameTimestampMs: number;
    readonly ffmpegPath: string;
    readonly videoPath: string;
  }): Promise<VideoKeyFramePreview>;
  setVideoKeyFrameCover(input: {
    readonly assetId: number;
    readonly frameTimestampMs: number;
    readonly ffmpegPath: string;
    readonly videoPath: string;
  }): Promise<AssetCover>;
  generateDefaultVideoCover?(assetId: number): Promise<AssetCover>;
  setCustomVideoCover(input: {
    readonly assetId: number;
    readonly coverPath: string;
  }): Promise<AssetCover>;
  removeVideoCover(assetId: number): Promise<void>;

  previewAssetPathRepair(input: {
    readonly assetId: number;
    readonly candidatePath: string;
  }): Promise<AssetPathRepairPreview>;
  executeAssetPathRepair(input: {
    readonly assetId: number;
    readonly candidatePath: string;
    readonly allowUnverified: boolean;
  }): Promise<AssetPathRepairPreview>;
}
