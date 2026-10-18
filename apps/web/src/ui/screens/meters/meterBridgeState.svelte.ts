// Meter-bridge overlay state (module rune store, no UiState/nav changes).
//
// The topbar button toggles `open` from anywhere; the Shell renders the
// MeterBridge dialog while open. `?meters=1` defaults it open (headless
// screenshot + deep-link affordance; the button remains the runtime path).

class MeterBridgeUi {
  open = $state(false);

  constructor() {
    try {
      const q = new URLSearchParams(window.location.search);
      if (q.get('meters') === '1') this.open = true;
    } catch {
      /* non-browser: closed */
    }
  }

  toggle(): void {
    this.open = !this.open;
  }

  close(): void {
    this.open = false;
  }
}

export const meterBridge = new MeterBridgeUi();
