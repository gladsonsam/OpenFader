part of 'app_store.dart';

// Selected destination processing is independent of input channel indices.
abstract interface class _AppStoreOutputsContract {
  set _outputProcessingTarget(String? value);
  String get masterProcessingTarget;
  int get _outputContextRevision;
  void _observeOutputProcessingContext();
}

mixin _AppStoreOutputs on _AppStoreContext {
  @override
  int _outputContextRevision = 0;
  bool? _outputLastLocked;

  // Observe every engine frame, including lock/unlock within one poll batch.
  @override
  void _observeOutputProcessingContext() {
    if (_outputLastLocked != locked) {
      _outputLastLocked = locked;
      _outputContextRevision++;
    }
  }

  String? _selectedOutputTarget;
  String? get _outputProcessingTarget => _selectedOutputTarget;
  @override
  set _outputProcessingTarget(String? value) {
    if (_selectedOutputTarget == value) return;
    _outputContextRevision++;
    _selectedOutputTarget = value;
  }

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

  String _processingTargetForBus(String id) => id == 'main'
      ? 'main'
      : id.startsWith('subgroup')
          ? 'subgroup/${((int.tryParse(id.substring('subgroup'.length)) ?? 0) + 1).toString().padLeft(2, '0')}'
          : 'bus/${((int.tryParse(id.replaceFirst('bus', '')) ?? 0) + 1).toString().padLeft(2, '0')}';

  @override
  void openOutputProcessing(String target) {
    if (!outputProcessingControls.any((control) => control.target == target)) {
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
