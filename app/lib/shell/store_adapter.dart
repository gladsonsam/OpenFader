// Shell adapter: base responsibility. The shell owns polling.
import 'package:flutter/foundation.dart';
import '../engine.dart';
import '../state/app_store.dart';

abstract class ShellStoreAdapter extends ChangeNotifier {
  ShellStoreAdapter(this.app) {
    app.addListener(forwardChange);
  }

  @protected
  final AppStore app;
  bool _isDisposed = false;

  @protected
  bool get isDisposed => _isDisposed;

  @protected
  EngineState get engine => app.engine;
  @protected
  void send(String json) => app.sendRaw(json);

  @protected
  void forwardChange() {
    if (!isDisposed) notifyListeners();
  }

  @override
  void dispose() {
    _isDisposed = true;
    app.removeListener(forwardChange);
    super.dispose();
  }
}
