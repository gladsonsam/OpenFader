// Persisted recent mixers (name + address), shared by the Connections
// start screen and the Settings forget flows. Stored as one JSON list.
library;

import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:shared_preferences/shared_preferences.dart';

class RecentMixerEntry {
  final String name;
  final String addr;
  const RecentMixerEntry({required this.name, required this.addr});
}

class RecentMixers extends ChangeNotifier {
  RecentMixers._();
  static final RecentMixers instance = RecentMixers._();
  static bool _loadStarted = false;

  static const key = 'openfader.recentMixers';

  List<RecentMixerEntry> _items = [];
  List<RecentMixerEntry> get items => List.unmodifiable(_items);

  static void ensureLoaded() {
    if (_loadStarted) return;
    _loadStarted = true;
    instance.load();
  }

  Future<void> load() async {
    final prefs = await SharedPreferences.getInstance();
    _items = _decode(prefs.getString(key));
    notifyListeners();
  }

  static List<RecentMixerEntry> _decode(String? raw) {
    if (raw == null || raw.isEmpty) return [];
    try {
      final list = jsonDecode(raw) as List;
      return [
        for (final e in list)
          if (e is Map<String, dynamic> && '${e['addr'] ?? ''}'.isNotEmpty)
            RecentMixerEntry(
                name: '${e['name'] ?? ''}', addr: '${e['addr'] ?? ''}'),
      ];
    } catch (_) {
      return [];
    }
  }

  Future<void> _save() async {
    final prefs = await SharedPreferences.getInstance();
    final saved = await prefs.setString(
        key,
        jsonEncode([
          for (final r in _items) {'name': r.name, 'addr': r.addr}
        ]));
    if (!saved) {
      await prefs.reload();
      _items = _decode(prefs.getString(key));
      notifyListeners();
      throw StateError('Saved mixers could not be updated.');
    }
    notifyListeners();
  }

  /// Add or rename (only when remembering is enabled by the caller).
  Future<void> remember(String name, String addr) async {
    if (addr.isEmpty) return;
    final i = _items.indexWhere((r) => r.addr == addr);
    if (i >= 0) {
      if (name.isNotEmpty && _items[i].name != name) {
        _items[i] = RecentMixerEntry(name: name, addr: addr);
      } else {
        return;
      }
    } else {
      _items.add(RecentMixerEntry(name: name, addr: addr));
    }
    await _save();
  }

  Future<void> forget(String addr) async {
    _items.removeWhere((r) => r.addr == addr);
    await _save();
  }

  Future<void> clear() async {
    if (_items.isEmpty) return;
    _items = [];
    await _save();
  }
}
