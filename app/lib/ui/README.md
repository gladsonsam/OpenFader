# OpenFader UI library

Import `ui/console.dart` from a feature to use shared controls and design tokens.
Implementation files import their specific dependencies to keep dependencies
acyclic and avoid accidentally coupling primitives to the whole library.

- `controls/`: buttons, icon targets, selectors and switches.
- `layout/`: panel surfaces and preference rows.
- `overlays/`: anchored option menus and reusable decision/name dialogs.
- `icons/`: the shared console icon vocabulary.
- `../design.dart`: immutable spacing, type, shape, target and timing tokens.
- `../palette.dart`: context-resolved theme colors.

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
