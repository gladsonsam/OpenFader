part of 'app_store.dart';

// Selected destination processing is independent of input channel indices.
abstract interface class _AppStoreOutputsContract {
  set _outputProcessingTarget(String? value);
  String get masterProcessingTarget;
  String get _primaryMainTarget;
  int get _outputContextRevision;
  void _observeOutputProcessingContext();
}

mixin _AppStoreOutputs on _AppStoreContext {
  int _outputContextRevisionBase = 0;
  @override
  int get _outputContextRevision =>
      _outputContextRevisionBase + engine.nativeProofRevision;
  bool? _outputLastLocked;

  // Observe every engine frame, including lock/unlock within one poll batch.
  @override
  void _observeOutputProcessingContext() {
    if (_outputLastLocked != locked) {
      _outputLastLocked = locked;
      _outputContextRevisionBase++;
    }
  }

  String? _selectedOutputTarget;
  String? get _outputProcessingTarget => _selectedOutputTarget;
  @override
  set _outputProcessingTarget(String? value) {
    if (_selectedOutputTarget == value) return;
    _outputContextRevisionBase++;
    _selectedOutputTarget = value;
  }

  @override
  Map<String, String> get outputTargets => Map.unmodifiable({
        for (final control in outputProcessingControls)
          control.target: control.destinationName,
      });

  @override
  String? get outputProcessingTarget => _outputProcessingTarget;
  @override
  List<DestinationControlVm> get outputProcessingControls => List.unmodifiable(
        engine.destinationControls.where(
          (control) => control.surface.startsWith('output_'),
        ),
      );

  @override
  String get masterProcessingTarget => _processingTargetForBus(_busId);

  @override
  String get _primaryMainTarget {
    for (final family in engine.topology) {
      if (family['kind'] != 'main' ||
          family['first'] is! num ||
          family['count'] is! num ||
          (family['count'] as num) < 1) {
        continue;
      }
      final first = (family['first'] as num).toInt();
      final strip = engine.strips.where((s) => s.ch == first).firstOrNull;
      if (strip != null) return strip.key;
      return '';
    }
    // Some providers expose a separate main master without a strip family.
    // Only use its legacy target once a real master value has arrived.
    if (engine.connected &&
        (engine.mainFader != null || engine.mainMuted != null)) {
      return 'main';
    }
    return '';
  }

  String _processingTargetForBus(String id) => id.startsWith('solo:')
      ? id.split(':')[1]
      : id == 'main'
          ? _primaryMainTarget
          : id.startsWith('subgroup')
              ? 'subgroup/${((int.tryParse(id.substring('subgroup'.length)) ?? 0) + 1).toString().padLeft(2, '0')}'
              : 'bus/${((int.tryParse(id.replaceFirst('bus', '')) ?? 0) + 1).toString().padLeft(2, '0')}';

  @override
  void openOutputProcessing(String target) {
    if (!accessTargetAllowed(target) ||
        !accessFamilyAllowed(AccessFamily.masterDsp) ||
        !outputTargets.containsKey(target)) {
      return;
    }
    if (_outputProcessingTarget == target) return;
    _outputProcessingTarget = target;
    for (final row in busRows) {
      if (_processingTargetForBus(row.id) == target) {
        selectBus(row.id);
        break;
      }
    }
    notifyListeners();
  }
}
