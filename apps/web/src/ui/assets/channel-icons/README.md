# Channel SVG registry

`glyphs.ts` contains the exact 50 owner-approved glyph bodies from the 2026-10-20
preview: 45 IconOven free Stroke icons and five original OpenMix drawings.
`ui/icons.ts` supplies the shared 24 × 24 viewBox, currentColor, no fill, 1.5px
stroke and round caps/joins. No runtime icon service, package or network is used.
All original 43 picker keys, their order/IDs, and category names are preserved;
seven approved additions are appended. Saved selections need no migration.

Sources and acquisition SVG SHA-256 hashes for every glyph are distributed in
`apps/web/public/licenses/channel-icons.json`. The registry extracts only the
outer `<svg>` wrapper; the approved path geometry is unchanged. No SVG has
external references, IDs, filters, masks or scripts.

IconOven source: https://iconoven.com/icons
Free Stroke collection: https://iconoven.com/docs/icons/iconify
Official MIT text: https://iconoven.com/licence/icons#free
Copyright (c) 2026 Appic Marketing LLC (IconOven), verified against the published
MIT notice on 2026-10-20. Individual download URLs are in the manifest. Iconify
was the acquisition transport; the license is IconOven's collection license.

The original handheld mic, kick drum, hi-hat, rack tom and orchestra conductor
were drawn for this project, not traced from another library, and approved by
the owner for inclusion under MIT. Bass uses `iconoven:bass-guitar`, acoustic
uses `iconoven:guitar`, and electric uses `iconoven:electric-guitar`. Speaker
uses `speaker-tower`; subwoofer uses the approved single-woofer `speaker`
glyph. Orchestra uses the original conductor rather than a software diagram.

Full IconOven MIT, original custom MIT and retained Lucide/Feather notices are
in `apps/web/public/licenses/`. Vite copies them to dist for redistribution and
Tauri embeds that frontend bundle. The existing meter `audio-lines` drawing
and gear/chevron chrome remain unchanged; see LICENSE-Lucide.txt. The app's
custom logo and Geist font/OFL notice remain untouched.
