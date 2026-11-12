// Engine-owned local library protocol. This state outlives desk/RTA sessions.
enum LocalLibraryMode { memory, device }

enum LocalLibraryHealth { ready, error, uncertain }

enum LocalLibraryResultStatus { committed, rejected, uncertain }

bool _unsigned(Object? value) => value is int && value >= 0;
bool _optionalString(Object? value) => value == null || value is String;
LocalLibraryMode? _mode(Object? value) => switch (value) {
      'memory' => LocalLibraryMode.memory,
      'device' => LocalLibraryMode.device,
      _ => null,
    };

class LocalLibraryStatus {
  final LocalLibraryMode mode;
  final LocalLibraryHealth status;
  final int revision;
  final String? error;
  final int? bytes;
  const LocalLibraryStatus._(
      this.mode, this.status, this.revision, this.error, this.bytes);

  static LocalLibraryStatus? fromJson(Map<String, dynamic> frame) {
    final mode = _mode(frame['mode']);
    final status = switch (frame['status']) {
      'ready' => LocalLibraryHealth.ready,
      'error' => LocalLibraryHealth.error,
      'uncertain' => LocalLibraryHealth.uncertain,
      _ => null,
    };
    if (frame['type'] != 'local_library_status' ||
        mode == null ||
        status == null ||
        !_unsigned(frame['revision']) ||
        !_optionalString(frame['error']) ||
        (frame['bytes'] != null && !_unsigned(frame['bytes'])) ||
        (mode == LocalLibraryMode.memory && frame['bytes'] != null)) {
      return null;
    }
    return LocalLibraryStatus._(mode, status, frame['revision'] as int,
        frame['error'] as String?, frame['bytes'] as int?);
  }

  @override
  bool operator ==(Object other) =>
      other is LocalLibraryStatus &&
      other.mode == mode &&
      other.status == status &&
      other.revision == revision &&
      other.error == error &&
      other.bytes == bytes;
  @override
  int get hashCode => Object.hash(mode, status, revision, error, bytes);
}

class LocalLibraryAction {
  final String operation;
  final int? number;
  final String? show;
  final String? name;
  const LocalLibraryAction._(this.operation,
      {this.number, this.show, this.name});
  factory LocalLibraryAction.save(int number, String name) =>
      LocalLibraryAction._numbered('save', number, name);
  factory LocalLibraryAction.rename(int number, String name) =>
      LocalLibraryAction._numbered('rename', number, name);
  factory LocalLibraryAction.delete(int number) =>
      LocalLibraryAction._numbered('delete', number, null);
  factory LocalLibraryAction._numbered(String op, int number, String? name) {
    if (number < 1 || number > 999) throw ArgumentError.value(number, 'number');
    if (name != null && name.trim().isEmpty) {
      throw ArgumentError.value(name, 'name');
    }
    return LocalLibraryAction._(op, number: number, name: name);
  }
  const LocalLibraryAction.clearNumbered() : this._('clear_numbered');
  factory LocalLibraryAction.capture(String show, String name) =>
      LocalLibraryAction._named('capture', show, name);
  factory LocalLibraryAction.deleteNamed(String show, String name) =>
      LocalLibraryAction._named('delete_named', show, name);
  factory LocalLibraryAction._named(String op, String show, String name) {
    if (show.trim().isEmpty || name.trim().isEmpty) {
      throw ArgumentError('Empty show or scene name');
    }
    return LocalLibraryAction._(op, show: show, name: name);
  }
  const LocalLibraryAction.reset() : this._('reset');
  static const operations = {
    'save',
    'rename',
    'delete',
    'clear_numbered',
    'capture',
    'delete_named',
    'reset'
  };
  Map<String, Object> toJson() => {
        'op': operation,
        'n': ?number,
        'show': ?show,
        'name': ?name,
      };
}

class LocalLibraryResult {
  final String requestId;
  final String operation;
  final int revision;
  final LocalLibraryResultStatus status;
  final LocalLibraryMode mode;
  final String? error;
  const LocalLibraryResult._(this.requestId, this.operation, this.revision,
      this.status, this.mode, this.error);

  static LocalLibraryResult? fromJson(Map<String, dynamic> frame) {
    final mode = _mode(frame['mode']);
    final status = switch (frame['status']) {
      'committed' => LocalLibraryResultStatus.committed,
      'rejected' => LocalLibraryResultStatus.rejected,
      'uncertain' => LocalLibraryResultStatus.uncertain,
      _ => null,
    };
    if (frame['type'] != 'local_library_result' ||
        mode == null ||
        status == null ||
        frame['request_id'] is! String ||
        (frame['request_id'] as String).isEmpty ||
        !LocalLibraryAction.operations.contains(frame['operation']) ||
        !_unsigned(frame['revision']) ||
        !_optionalString(frame['error'])) {
      return null;
    }
    return LocalLibraryResult._(
        frame['request_id'] as String,
        frame['operation'] as String,
        frame['revision'] as int,
        status,
        mode,
        frame['error'] as String?);
  }
}

/// Only [result] is engine authority; a transport/deadline cancellation cannot
/// turn submission or a refreshed list into proof that a write committed.
class LocalLibraryCompletion {
  final LocalLibraryResult? result;
  final String? interruption;
  final bool dispatched;
  const LocalLibraryCompletion.result(LocalLibraryResult value)
      : result = value,
        interruption = null,
        dispatched = true;
  const LocalLibraryCompletion.interrupted(String message)
      : result = null,
        interruption = message,
        dispatched = true;
  const LocalLibraryCompletion.notDispatched(String message)
      : result = null,
        interruption = message,
        dispatched = false;
  bool get committed => result?.status == LocalLibraryResultStatus.committed;
  String get message {
    final value = result;
    if (value == null) {
      return dispatched
          ? '$interruption The operation may have taken effect; its result is unconfirmed.'
          : interruption!;
    }
    final detail = value.error == null ? '' : ' ${value.error}';
    return switch (value.status) {
      LocalLibraryResultStatus.committed => value.mode ==
              LocalLibraryMode.device
          ? 'Change saved on this device.'
          : 'Change accepted in local memory only; it will not survive engine restart.',
      LocalLibraryResultStatus.rejected => 'Library change rejected.$detail',
      LocalLibraryResultStatus.uncertain =>
        'Library change uncertain; it may have taken effect.$detail',
    };
  }
}
