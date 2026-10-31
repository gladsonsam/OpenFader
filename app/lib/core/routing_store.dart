import 'package:flutter/foundation.dart';
import 'destination_controls.dart';

class RouteStrip {
  final int strip;
  final String id;
  final String name;
  final String src;

  const RouteStrip({
    required this.strip,
    required this.id,
    required this.name,
    required this.src,
  });
}

class RouteSrcTile {
  final String label;
  // Numbered sources reserve space for the index independently of the group.
  // Null keeps the complete label for unnumbered sources.
  final String? displayGroupLabel;
  final String group;
  final int index;
  final String usedBy;
  final bool current;
  final bool mode;
  final String? operationKey;

  const RouteSrcTile({
    required this.label,
    required this.group,
    required this.index,
    this.displayGroupLabel,
    this.usedBy = '',
    this.operationKey,
    this.current = false,
    this.mode = false,
  });
}

class RoutePatchTarget {
  final String key, label;
  final bool clearable;
  const RoutePatchTarget(this.key, this.label, this.clearable);
}

class RouteChange {
  final RouteStrip destination;
  final RouteSrcTile source;
  final String guard;
  const RouteChange(this.destination, this.source, this.guard);
}

// Implemented by the shell over the engine or a fixture sim.
abstract class RoutingStore extends ChangeNotifier {
  List<RouteStrip> get strips;
  List<RoutePatchTarget> get targets;
  String get target;
  bool get canClear;
  String? get operationKey;
  void selectTarget(String target);
  String groupLabel(String group);
  List<String> get groups;
  String get group;
  int? get selectedStrip;
  String get selectedName;
  String get selectedSrc;
  List<RouteSrcTile> get srcTiles;
  List<RouteSrcTile> get allSrcTiles => srcTiles;
  bool get routingSupported;
  bool get isModeSelector;
  bool get canPatch => routingSupported;
  int get controlAccessEpoch => 0;
  DestinationControlVm? get tapControl => null;
  bool tapIsCurrent(DestinationControlVm control) => false;
  Future<String?> setTapControl(
          DestinationControlVm control, double value) async =>
      'Routing tap unavailable.';

  void selectStrip(int strip);
  void selectGroup(String group);
  void patch(RouteSrcTile tile);
  void clearPatch(int strip, {String? operationKey});
  List<RouteChange>? planMappings(List<(int, RouteSrcTile)> mappings);
  bool planIsCurrent(List<RouteChange> plan);
  bool applyPlan(List<RouteChange> plan);
}
