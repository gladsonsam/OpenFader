import 'package:flutter/foundation.dart';
import '../engine.dart' show NativeSnapshotsVm;
import 'native_library.dart';
import 'native_library_store.dart';

class SceneEntryVm {
  final int number;
  final String name;
  final bool current;

  const SceneEntryVm({
    required this.number,
    required this.name,
    this.current = false,
  });
}

// Implemented by the shell over the engine or a fixture sim.
abstract class ScenesStore extends ChangeNotifier
    implements NativeLibraryStore {
  @override
  NativeLibraryVm? get nativeLibrary => null;
  @override
  String? get nativeStatus => null;
  @override
  bool get nativeBusy => false;
  @override
  bool get nativeLastConfirmed => false;
  @override
  bool get nativeWritable => false;
  @override
  Object get nativeContext => actionContext;
  @override
  Future<void> nativeAction(
      Map<String, dynamic> action, Object captured) async {}
  @override
  void cancelNativeAction() {}
  List<SceneEntryVm> get entries;
  List<String> get shows;
  List<String> get showScenes;
  bool get busy;
  String? get status;
  String get libraryDescription =>
      'Local library durability is not yet proven. '
      'It is not a native mixer backup.';
  String? get show; // active show for capture/recall, null = numbered only
  bool get online;

  /// Optional passive surface. Stores without native support remain honest.
  NativeSnapshotsVm get nativeSnapshots =>
      const NativeSnapshotsVm.unsupported();

  // Validation rejects stale targets without sending.
  Object get actionContext;
  bool validateActionContext(Object captured);
  bool captureWillOverwrite(String name);

  void save(String name);
  void recall(int number);
  void rename(int number, String name);
  void remove(int number);
  void clearAll();
  void selectShow(String show);
  void beginShow();
  void captureShow(String name);
  void recallShow(String name);
  void deleteShow(String name);
}
