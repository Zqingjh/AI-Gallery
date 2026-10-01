import type { MediaIntegrityService } from "../../services/media-integrity-service";
import type { AiService } from "../../services/ai-service";
import type { LibraryService } from "../../services/library-service";
import type {
  P1LibraryService,
  P1SavedAssetFilter,
} from "../../services/p1-library-service";
import { P1BatchPanel } from "./P1BatchPanel";

export type { P1LibraryService } from "../../services/p1-library-service";

export function P1LibraryPage({
  service,
  taxonomyService,
  aiService,
  accessMode = "readWrite",
  onBack,
}: {
  readonly service: P1LibraryService;
  readonly taxonomyService: Pick<
    LibraryService,
    "listTaxonomy" | "listMetadataPresets"
  >;
  readonly aiService?: AiService;
  readonly mediaIntegrityService?: MediaIntegrityService;
  readonly accessMode?: "readWrite" | "readOnly";
  readonly initialMediaAssetId?: string | null;
  readonly onBack?: () => void;
  readonly onApplyFilter?: (filter: P1SavedAssetFilter) => void;
}) {
  const readOnly = accessMode === "readOnly";
  return (
    <main className="library-main" aria-labelledby="p1-library-title">
      <header className="library-heading">
        <div>
          <p className="eyebrow">P1 / EFFICIENCY</p>
          <h1 id="p1-library-title">创作效率</h1>
          <p className="readonly-notice">
            按作品展示编号批量修改评分、状态，并从现有分类与标签中选择关系。
          </p>
        </div>
        <div className="dialog-actions">
          {readOnly ? <span className="readonly-pill">只读展示</span> : null}
          {onBack ? (
            <button className="secondary-button" type="button" onClick={onBack}>
              返回作品库
            </button>
          ) : null}
        </div>
      </header>
      <P1BatchPanel
        service={service}
        taxonomyService={taxonomyService}
        aiService={aiService}
        readOnly={readOnly}
      />
    </main>
  );
}

export default P1LibraryPage;
