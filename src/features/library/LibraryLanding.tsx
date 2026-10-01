import { text } from "../../app/texts";
import { BrandMark } from "../../components/BrandMark";
import { ThemeToggle } from "../../components/ThemeToggle";
import type { LibraryService } from "../../services/library-service";
import type { MediaIntegrityService } from "../../services/media-integrity-service";
import type { P1SavedAssetFilter } from "../../services/p1-library-service";
import type { WorkspaceAccessMode } from "../../services/workspace-management-service";
import { LibraryPage } from "./LibraryPage";
import { Suspense } from "react";

interface LibraryLandingProps {
  onOpenSettings: () => void;
  onOpenAiReview: () => void;
  onOpenP1?: (assetId?: string) => void;
  onWorkspaceChanged: (rootPath: string | null) => void;
  accessMode: WorkspaceAccessMode;
  service?: LibraryService;
  mediaIntegrityService?: MediaIntegrityService;
  savedAssetFilter?: P1SavedAssetFilter | null;
  mode?: "standard" | "nsfw";
  onSecretActivate?: () => void;
  onExitPrivate?: () => void;
}

export function LibraryLanding({
  onOpenAiReview,
  onOpenP1,
  onOpenSettings,
  onWorkspaceChanged,
  accessMode,
  service,
  mediaIntegrityService,
  savedAssetFilter,
  mode = "standard",
  onSecretActivate,
  onExitPrivate,
}: LibraryLandingProps) {
  if (service) {
    return (
      <Suspense
        fallback={<p className="route-loading">{text.shell.loading}</p>}
      >
        <LibraryPage
          service={service}
          mediaIntegrityService={mediaIntegrityService}
          onOpenAiReview={onOpenAiReview}
          onOpenP1={onOpenP1}
          onOpenSettings={onOpenSettings}
          onWorkspaceChanged={onWorkspaceChanged}
          accessMode={accessMode}
          savedAssetFilter={savedAssetFilter}
          mode={mode}
          onSecretActivate={onSecretActivate}
          onExitPrivate={onExitPrivate}
        />
      </Suspense>
    );
  }

  return (
    <div className="app-shell">
      <header className="topbar">
        <a className="brand" href="/" aria-label={text.productName}>
          <BrandMark />
          <span>
            <strong>{text.productName}</strong>
            <small>{text.productTagline}</small>
          </span>
        </a>
        <nav className="main-nav" aria-label={text.mainNavigationLabel}>
          <a className="is-active" href="/">
            {text.nav.library}
          </a>
          <span aria-disabled="true">{text.nav.projects}</span>
          {onOpenP1 ? (
            <button
              className="text-button"
              type="button"
              onClick={() => onOpenP1()}
            >
              {text.nav.efficiency}
            </button>
          ) : null}
          <button
            className="text-button"
            type="button"
            onClick={onOpenAiReview}
          >
            {text.nav.review}
          </button>
        </nav>
        <div className="topbar-actions">
          <span className="privacy-pill">
            <i />
            {text.shell.privacy}
          </span>
          <ThemeToggle />
          <button
            className="text-button settings-button"
            type="button"
            onClick={onOpenSettings}
          >
            {text.nav.settings}
          </button>
        </div>
      </header>

      <main>
        <section className="hero" aria-labelledby="hero-title">
          <div className="hero-copy">
            <p className="eyebrow">{text.shell.eyebrow}</p>
            <h1 id="hero-title">{text.shell.title}</h1>
            <p className="hero-description">{text.shell.description}</p>
            <div className="hero-actions">
              <button
                className="primary-button"
                type="button"
                disabled
                aria-describedby="workspace-actions-status"
              >
                {text.shell.createWorkspace}
              </button>
              <button
                className="secondary-button"
                type="button"
                disabled
                aria-describedby="workspace-actions-status"
              >
                {text.shell.openWorkspace}
              </button>
            </div>
            <p
              id="workspace-actions-status"
              className="action-status"
              role="status"
            >
              {text.shell.workspaceActionsUnavailable}
            </p>
          </div>
          <div className="archive-card" aria-label={text.shell.emptyTitle}>
            <div className="archive-index">A—001</div>
            <div className="empty-frame">
              <BrandMark />
            </div>
            <div>
              <h2>{text.shell.emptyTitle}</h2>
              <p>{text.shell.emptyDescription}</p>
            </div>
          </div>
        </section>
      </main>
    </div>
  );
}
