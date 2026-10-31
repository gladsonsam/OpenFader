import 'native_library.dart';

abstract interface class NativeLibraryStore {
  NativeLibraryVm? get nativeLibrary;
  String? get nativeStatus;
  bool get nativeBusy;
  bool get nativeWritable;
  Object get nativeContext;
  Future<void> nativeAction(Map<String, dynamic> action, Object captured);
  void cancelNativeAction();
  bool get hasNativeFile;
  Future<void> importNativeFile();
  Future<void> exportNativeFile();
}
