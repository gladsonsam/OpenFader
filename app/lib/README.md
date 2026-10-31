# Application structure

OpenFader separates presentation, screen contracts, shell adapters and state.
Follow the existing responsibility before adding a new abstraction or folder.

| Folder | Responsibility |
| --- | --- |
| `ui/` | Generic UI library: controls, surfaces, overlays and icons; import `ui/console.dart`. |
| `widgets/` | Reusable audio/product widgets; channel controls are grouped in `widgets/channel/`. |
| `screens/` | Page composition and local interaction state. Related settings sections live in `screens/settings/`. |
| `core/` | Shared view models, contracts and domain helpers. Routing, scenes and native-library store contracts live here. |
| `mixer/` | Mixer layout/controller and strip-specific components. |
| `shell/` | Navigation, dependency composition and adapters implementing screen contracts. |
| `state/` | The application store and responsibility-specific parts. |
| `boot/` | Staged initialization and native/offline engine selection. |
| `settings/` | Persisted preferences and local mix/connection services. |

`design.dart` owns the shared geometry and type tokens. `palette.dart` owns
immutable theme colors, resolved from context. `theme.dart` maps those choices
onto Flutter's Material controls. See `ui/README.md` for component conventions.

Keep UI primitives independent of stores and engine data. Inject screen stores
through constructors; screens never import `state/app_store.dart`. The shell
adapts the application state to those contracts. New contracts shared by an
adapter and a screen belong in `core/`, rather than in a widget implementation.
Existing page-local contracts can move when that feature is being refactored.

A component owns its presentation and immediate interaction, not application
transactions. Capture and validate targets/access in the feature or store around
an awaited dialog; shared dialogs return only a decision or entered text. Keep
provider-specific labels and capabilities in the adapters/domain models.

Keep specialized paint geometry and gesture handling together. Share audio
controls through `widgets/channel_widgets.dart` without exposing their private
painters. Do not merge superficially similar meters or faders whose scales or
protocol values differ. The shell's 80 ms poll loop remains the only meter
clock, and hot-path strip changes update per-strip notifiers in place.

Verification and repository rules are in the root `AGENTS.md`: clean analyzer,
iOS build and both simulator sizes, plus the locked Rust workspace tests. No
frontend test suites are currently permitted.
