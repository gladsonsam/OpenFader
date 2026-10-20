// Small DOM-only boot view: visible before Svelte, styles and route chunks load.
// This reports local UI preparation, never desk/network/hydration readiness.
export function createStartupView(target: HTMLElement) {
  const root = document.createElement('main');
  root.className = 'bootstatic';
  root.setAttribute('aria-label', 'openmix startup');
  const brand = document.createElement('h1');
  brand.className = 'bootstatic-word';
  brand.textContent = 'openmix';
  const spinner = document.createElement('div');
  spinner.className = 'bootstatic-spin';
  spinner.setAttribute('aria-hidden', 'true');
  const status = document.createElement('div');
  status.className = 'bootstatic-msg';
  status.setAttribute('role', 'status');
  status.setAttribute('aria-live', 'polite');
  status.setAttribute('aria-atomic', 'true');
  const title = document.createElement('p');
  const detail = document.createElement('p');
  detail.className = 'bootstatic-detail';
  const actions = document.createElement('div');
  actions.className = 'bootstatic-actions';
  status.append(title, detail);
  root.append(brand, spinner, status, actions);

  return {
    loading() {
      target.dataset.startupState = 'preparing';
      spinner.hidden = false;
      title.textContent = 'Preparing your workspace';
      detail.textContent = 'Loading pages and controls for a smooth first visit.';
      actions.replaceChildren();
      target.replaceChildren(root);
    },
    progress(completed: number, total: number) {
      detail.textContent = `${completed} of ${total} pages ready`;
    },
    failed(error: unknown, retry: () => void) {
      target.dataset.startupState = 'error';
      spinner.hidden = true;
      title.textContent = 'Couldn’t finish preparing openmix';
      detail.textContent = error instanceof StartupTimeout
        ? 'Loading is taking longer than expected. Retry, or reload the app to start fresh.'
        : 'Some app files could not be loaded. Retry, or reload the app to fetch a fresh copy.';
      const retryButton = document.createElement('button');
      retryButton.type = 'button';
      retryButton.textContent = 'Try again';
      retryButton.onclick = retry;
      const reloadButton = document.createElement('button');
      reloadButton.type = 'button';
      reloadButton.textContent = 'Reload app';
      reloadButton.onclick = () => window.location.reload();
      actions.replaceChildren(retryButton, reloadButton);
      // Also restore the view if a synchronous component mount failed.
      target.replaceChildren(root);
    },
  };
}

export class StartupTimeout extends Error {
  constructor() {
    super('Local UI preparation exceeded 25 seconds');
    this.name = 'StartupTimeout';
  }
}

/** Give the branded view a paint opportunity before evaluating route modules.
 * Backgrounded WebViews may suspend rAF; the timer keeps preparation moving. */
export function startupPaint(): Promise<void> {
  return new Promise(resolve => {
    let frame: number;
    const timer = setTimeout(done, 250);
    function done() {
      clearTimeout(timer);
      cancelAnimationFrame(frame);
      resolve();
    }
    frame = requestAnimationFrame(() => { frame = requestAnimationFrame(done); });
  });
}
