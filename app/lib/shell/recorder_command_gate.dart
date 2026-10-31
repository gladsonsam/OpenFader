/// Serializes recorder commands while allowing Stop to request cancellation of
/// an operation that is still waiting for its host response.
class RecorderCommandGate {
  RecorderCommandGate(this.onChanged);

  final void Function() onChanged;
  Future<void> _queue = Future.value();
  int _active = 0;
  bool _stopActive = false;

  bool get pending => _active > 0;
  bool get stopActive => _stopActive;

  Future<void> run(String action, Future<void> Function() operation) {
    if (action == 'stop') {
      if (_stopActive) return Future.value();
      _stopActive = true;
      return _execute(operation, stop: true);
    }
    // Do not enqueue a second command behind a long-running operation.
    if (pending) return Future.value();
    _active++;
    onChanged();
    final result = _queue.then((_) => operation());
    _queue = result.then<void>((_) {}, onError: (Object _, StackTrace __) {});
    return result.whenComplete(() {
      _active--;
      onChanged();
    });
  }

  Future<void> _execute(Future<void> Function() operation,
      {required bool stop}) async {
    _active++;
    onChanged();
    try {
      await operation();
    } finally {
      _active--;
      if (stop) _stopActive = false;
      onChanged();
    }
  }
}
