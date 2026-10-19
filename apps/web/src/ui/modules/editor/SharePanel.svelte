<!-- Visual-editor share panel: profile export/import plus banks/pages
     library editing. Export is `exportProfile` JSON in a textarea (copyable
     text = the "QR/text" path: paste it anywhere); file download/upload
     buttons move the same document. Import runs `importProfile` (migrate +
     validate, never throws) and reports errors inline. Banks/pages edits go
     through the store setters (locked profiles reject them). -->
<script lang="ts">
  import { exportProfile, importProfile, type BankDef, type PageDef } from '../profile/io.ts';

  let {
    profile,
    locked = false,
    canEditLibrary = true,
    onImport,
    onBanks,
    onPages,
  }: {
    profile: { id: string; ops: unknown[]; banks: BankDef[]; pages: PageDef[] };
    locked?: boolean;
    canEditLibrary?: boolean;
    onImport: (json: string) => string | null;
    onBanks: (banks: BankDef[]) => void;
    onPages: (pages: PageDef[]) => void;
  } = $props();

  const full = $derived(
    // Rehydrate the full document for export from the live profile fields.
    // The parent passes the real UserProfile; this view only needs id/ops.
    profile as unknown as Parameters<typeof exportProfile>[0],
  );
  let text = $state('');
  let error = $state('');
  let lastId = $state('');

  $effect(() => {
    if (full.id === lastId) return;
    lastId = full.id;
    text = exportProfile(full);
    error = '';
  });

  function refresh(): void {
    text = exportProfile(full);
    error = '';
  }

  function doImport(): void {
    const err = onImport(text);
    error = err ?? '';
  }

  function download(): void {
    const blob = new Blob([exportProfile(full)], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `${full.id}.openmix-profile.json`;
    a.click();
    URL.revokeObjectURL(url);
  }

  function upload(e: Event): void {
    const input = e.currentTarget as HTMLInputElement;
    const f = input.files?.[0];
    if (!f) return;
    const r = new FileReader();
    r.onload = () => {
      text = String(r.result ?? '');
      doImport();
    };
    r.readAsText(f);
    input.value = '';
  }

  function copyText(): void {
    void navigator.clipboard?.writeText(text).catch(() => {});
  }

  function checkImportPreview(): void {
    const r = importProfile(text);
    error = r.ok
      ? `Valid: ${r.profile.ops.length} ops, ${r.profile.banks.length} banks, ${r.profile.pages.length} pages.`
      : r.errors.join('; ');
  }

  function editBankItems(b: BankDef, raw: string): void {
    const items = raw
      .split(',')
      .map(s => s.trim())
      .filter(s => s.length > 0);
    onBanks(profile.banks.map(x => (x.id === b.id ? { ...x, items } : x)));
  }

  function addBank(): void {
    const id = `bank-${profile.banks.length + 1}`;
    onBanks([...profile.banks, { id, name: `Bank ${profile.banks.length + 1}`, items: [] }]);
  }

  function addPage(): void {
    const id = `page-${profile.pages.length + 1}`;
    onPages([...profile.pages, { id, title: `Page ${profile.pages.length + 1}`, layout: 'mixer.default', nav: 'tab' }]);
  }
</script>

<div aria-label="Share profile" style="display:flex;flex-direction:column;gap:6px;min-width:230px">
  <div style="display:flex;flex-wrap:wrap;gap:4px">
    <button onclick={refresh} style="font-size:12px">Export</button>
    <button onclick={doImport} style="font-size:12px">Import</button>
    <button onclick={download} style="font-size:12px">Download file</button>
    <button onclick={copyText} style="font-size:12px">Copy</button>
    <button onclick={checkImportPreview} style="font-size:12px">Validate</button>
    <label style="font-size:12px;border:1px solid var(--u4-line-ctl);border-radius:4px;padding:2px 6px;cursor:pointer">
      Upload file
      <input type="file" accept=".json,application/json" onchange={upload} style="display:none" />
    </label>
  </div>
  {#if error}<div role={error.startsWith('Valid') ? 'status' : 'alert'} style="font-size:12px;color:{error.startsWith('Valid') ? 'var(--u4-text-4)' : '#e0605f'}">{error}</div>{/if}
  <textarea
    aria-label="Profile JSON"
    rows="6"
    cols="40"
    bind:value={text}
    style="font-family:monospace;font-size:11px"
  ></textarea>

  {#if canEditLibrary}
    <details open>
      <summary style="font-size:12px;font-weight:600">Banks ({profile.banks.length})</summary>
      <div style="display:flex;flex-direction:column;gap:4px;margin-top:4px">
        {#each profile.banks as b (b.id)}
          <div style="display:flex;flex-direction:column;gap:2px;border:1px solid var(--u4-line-faint);border-radius:4px;padding:4px">
            <div style="display:flex;gap:4px;align-items:center">
              <input
                aria-label={`Bank name ${b.id}`}
                type="text"
                value={b.name}
                disabled={locked}
                onchange={(e) => onBanks(profile.banks.map(x => (x.id === b.id ? { ...x, name: (e.currentTarget as HTMLInputElement).value } : x)))}
                style="flex:1;font-size:12px;padding:2px 4px"
              />
              <button
                aria-label={`Delete bank ${b.id}`}
                disabled={locked}
                onclick={() => onBanks(profile.banks.filter(x => x.id !== b.id))}
                style="font-size:11px">✕</button
              >
            </div>
            <input
              aria-label={`Bank items ${b.id} (comma separated strip keys)`}
              type="text"
              value={b.items.join(', ')}
              disabled={locked}
              onchange={(e) => editBankItems(b, (e.currentTarget as HTMLInputElement).value)}
              placeholder="ch/01, ch/02, main"
              style="font-size:11px;padding:2px 4px"
            />
          </div>
        {/each}
        <button onclick={addBank} disabled={locked} style="font-size:12px">Add bank</button>
      </div>
    </details>
    <details>
      <summary style="font-size:12px;font-weight:600">Pages ({profile.pages.length})</summary>
      <div style="display:flex;flex-direction:column;gap:4px;margin-top:4px">
        {#each profile.pages as p (p.id)}
          <div style="display:flex;flex-wrap:wrap;gap:4px;border:1px solid var(--u4-line-faint);border-radius:4px;padding:4px;align-items:center">
            <input
              aria-label={`Page title ${p.id}`}
              type="text"
              value={p.title}
              disabled={locked}
              onchange={(e) => onPages(profile.pages.map(x => (x.id === p.id ? { ...x, title: (e.currentTarget as HTMLInputElement).value } : x)))}
              style="flex:1;min-width:90px;font-size:12px;padding:2px 4px"
            />
            <input
              aria-label={`Page layout ${p.id}`}
              type="text"
              value={p.layout}
              disabled={locked}
              onchange={(e) => onPages(profile.pages.map(x => (x.id === p.id ? { ...x, layout: (e.currentTarget as HTMLInputElement).value } : x)))}
              style="flex:1;min-width:90px;font-size:11px;padding:2px 4px"
            />
            <select
              aria-label={`Page nav ${p.id}`}
              value={p.nav}
              disabled={locked}
              onchange={(e) => onPages(profile.pages.map(x => (x.id === p.id ? { ...x, nav: (e.currentTarget as HTMLSelectElement).value as PageDef['nav'] } : x)))}
              style="font-size:11px"
            >
              <option value="header">header</option>
              <option value="rail">rail</option>
              <option value="tab">tab</option>
              <option value="hidden">hidden</option>
            </select>
            <button
              aria-label={`Delete page ${p.id}`}
              disabled={locked}
              onclick={() => onPages(profile.pages.filter(x => x.id !== p.id))}
              style="font-size:11px">✕</button
            >
          </div>
        {/each}
        <button onclick={addPage} disabled={locked} style="font-size:12px">Add page</button>
      </div>
    </details>
  {/if}
</div>
