# OpenFader UI library

Import `ui/console.dart` from a feature to use shared controls and design tokens.
Implementation files import their specific dependencies to keep dependencies
acyclic and avoid accidentally coupling primitives to the whole library.

- `controls/`: buttons, icon targets, selectors and switches.
- `layout/`: panel surfaces and preference rows.
- `overlays/`: anchored option menus and reusable decision/name dialogs.
- `text/`: named text roles and tones.
- `icons/`: the shared console icon vocabulary.
- `../design.dart`: immutable spacing, type, shape, target and timing tokens.
- `../palette.dart`: context-resolved theme colors.
- `density.dart`: `ConsoleDensity` theme extension (large touch targets);
  read with `ConsoleDensity.of(context)` and pass values into painters.

## Which component do I use?

- Action: `ConsoleButton`. Variants: `secondary` (default), `primary` (main
  action), `quiet` (low emphasis, e.g. Cancel in dense spots), `danger`
  (outlined risky action), `destructive` (filled irreversible confirm). Sizes
  set label size and painted density: `dense` (11, 28-high body), `compact`
  (13, 30, default), `regular` (16, 34, dialogs/forms), `large` (17, 38, page
  actions) and `hero` (24, 48, front-door actions). Every size keeps a target
  of at least 44 points around the smaller painted body. `selected` marks a
  toggle on state. `padding: ConsoleButtonPadding.tight/none` for fixed
  slots, `leading`/`trailing` widgets (glyph, drop-down arrow), `alignment`,
  `subtitle` (muted second line), `foreground` (on-colour when filled) and
  `accent` (also the text colour of `quiet`). Icon-only: `ConsoleIconButton`.
- Underline tabs: `ConsoleTabStrip<T>`.
- Dialogs: `showConsoleConfirmation` (yes/no), `showConsoleChoice` (2-3
  outcomes, Cancel returns null), `showConsoleTextInput` / `showConsoleNameDialog`
  (text, with optional inline `validate`), `showConsoleDialog` + `ConsoleDialog`
  for custom content.
- Selection list: `showConsoleSelection` (pick one of several options, optional
  detail line and check on the selected one; Cancel returns null).
- Tappable row that opens a page or action: `ConsoleNavigationTile` (label,
  optional detail, leading, trailing chevron by default; `outlined` for
  standalone panel tiles; `onTap: null` disables).
- Text: `ConsoleText` or `consoleTextStyle` with a `ConsoleTextRole` (size and
  weight; `sheetTitle` for full-screen sheets, `entryValue` for large entered
  values, `micro` 10) and `ConsoleTextTone` (colour). `AppDesign.tickLabel` (8)
  is for painter tick labels only.
- Breakpoints: shared ones come from `AppDesign` (`narrowBreakpoint` for narrow
  content panes, `phoneBreakpoint`, ...). A feature-specific threshold must be a
  named const in its file. Painter/graph geometry stays in its module.

Screens must not use raw `TextButton`, `FilledButton`, `ElevatedButton`,
`OutlinedButton`, `IconButton`, `AlertDialog` or numeric font sizes.

Keep this library independent of screens, stores, engine/protocol objects and
application state. Pass values and callbacks through constructors. Dialogs only
return decisions or text; callers own validation, captured targets and actions.
Use a small visible icon inside a full touch target, and keep option menus
anchored to their trigger. Use theme colors rather than mutable global colors.

Audio-specific controls belong in `widgets/channel/`, exported through
`widgets/channel_widgets.dart`; mixer strip composition stays in
`mixer/components/`. Keep graph math, gesture behavior and fader laws with their
owning audio modules. Feature-specific widgets stay within their feature until
there is an actual shared responsibility to extract.

When changing a shared component, inspect every consumer, run `flutter analyze`,
and visually verify the running app at the viewport sizes relevant to the
change. The Linux desktop app qualifies on Linux. Follow the repository's
`AGENTS.md` for the remaining gates and the prohibition on frontend test suites.
Its Visual standards section covers visual bounds versus hit areas, strip
insets, shared spacing tokens, label budgets and semantics; use
`scripts/visual_qa.py` for repeatable Linux captures.

`python3 scripts/ui_guard.py check` fails on new raw buttons, dialogs, numeric
font sizes, color literals and breakpoints outside this library, and on literal
action labels over 5 words or 36 characters; see AGENTS.md.
