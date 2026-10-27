import 'dart:convert';

/// Installation-local convenience lock, not mixer authentication or secure
/// storage. Guide V5.1 pp.190, 192-194 establishes device scope, All On/Off,
/// four-character entry/confirmation and uninstall recovery. The credential stays in this device's preferences, never exports.
class AccessPolicy {
  AccessPolicy({
    Set<String> blocked = const {},
    this.passcode = '',
    this.locked = false,
  }) : blocked = Set.unmodifiable(blocked);

  final Set<String> blocked;
  final String passcode;
  final bool locked;
  bool get hasPasscode => passcode.isNotEmpty;

  AccessPolicy copyWith({
    Set<String>? blocked,
    String? passcode,
    bool? locked,
  }) =>
      AccessPolicy(
        blocked: blocked ?? this.blocked,
        passcode: passcode ?? this.passcode,
        locked: locked ?? this.locked,
      );

  static void validatePasscode(String value, String confirmation) {
    if (value.runes.length != 4) {
      throw ArgumentError('Enter exactly four characters.');
    }
    if (value != confirmation) {
      throw ArgumentError('Passcodes do not match.');
    }
  }

  void requirePasscode(String value) {
    if (!hasPasscode || value != passcode) {
      throw StateError('Incorrect passcode.');
    }
  }

  String encode() => jsonEncode({
        'blocked': blocked.toList()..sort(),
        'passcode': passcode,
        'locked': locked,
      });

  static AccessPolicy decode(String? raw, String? legacy) {
    if (raw != null) {
      var blocked = _legacy(legacy);
      try {
        final value = jsonDecode(raw) as Map;
        blocked = (value['blocked'] as List).cast<String>().toSet();
        final passcode = value['passcode'] as String;
        final locked = value['locked'] as bool;
        if (passcode.isNotEmpty && passcode.runes.length != 4 ||
            locked && passcode.isEmpty) {
          throw const FormatException();
        }
        return AccessPolicy(
          blocked: blocked,
          passcode: passcode,
          locked: locked,
        );
      } catch (_) {
        // Never silently unlock a damaged persisted lock. OS reset is recovery.
        return AccessPolicy(blocked: blocked, locked: true);
      }
    }
    return AccessPolicy(blocked: _legacy(legacy));
  }

  static Set<String> _legacy(String? raw) {
    try {
      return (jsonDecode(raw ?? '[]') as List).cast<String>().toSet();
    } catch (_) {
      return const {};
    }
  }
}
