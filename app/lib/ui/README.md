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

## Which component do I use?

- Action: `ConsoleButton`. Variants: `secondary` (default), `primary` (main
  action), `quiet` (low emphasis, e.g. Cancel in dense spots), `danger`
  (outlined risky action), `destructive` (filled irreversible confirm). Sizes:
  `compact` (default), `regular` (dialogs, forms), `large` (page actions).
  `selected` marks a toggle on state. Icon-only: `ConsoleIconButton`.
- Dialogs: `showConsoleConfirmation` (yes/no), `showConsoleChoice` (2-3
  outcomes, Cancel returns null), `showConsoleTextInput` / `showConsoleNameDialog`
  (text, with optional inline `validate`), `showConsoleDialog` + `ConsoleDialog`
  for custom content.
- Selection list: `showConsoleSelection` (pick one of several options, optional
  detail line and check on the selected one; Cancel returns null).
- Tappable row that opens a page or action: `ConsoleNavigationTile` (label,
  optional detail, leading, trailing chevron by default; `onTap: null` disables).
- Text: `ConsoleText` or `consoleTextStyle` with a `ConsoleTextRole` (size and
  weight; `sheetTitle` for full-screen sheets, `entryValue` for large entered
  values) and `ConsoleTextTone` (colour).
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
build the app, and check both phone and tablet simulators. Do not add frontend
test suites (see the repository's AGENTS.md). Rust workspace tests remain the
protocol regression gate.

`python3 scripts/ui_guard.py check` fails on new raw buttons, dialogs, numeric font sizes, color literals and breakpoints outside this library; see AGENTS.md.
