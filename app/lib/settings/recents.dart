// Persisted recent mixers (name, address, driver), shared by the Connections
// screen and the Settings forget flows. One JSON list.
library;

import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:shared_preferences/shared_preferences.dart';

class RecentMixerEntry {
  final String name;
  final String addr;
  final String driver;
  const RecentMixerEntry(
      {required this.name, required this.addr, this.driver = 'auto'});
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
                name: '${e['name'] ?? ''}',
                addr: '${e['addr'] ?? ''}',
                driver: '${e['driver'] ?? 'auto'}'),
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
          for (final r in _items)
            {'name': r.name, 'addr': r.addr, 'driver': r.driver}
        ]));
    if (!saved) {
      await prefs.reload();
      _items = _decode(prefs.getString(key));
      notifyListeners();
      throw StateError('Saved mixers could not be updated.');
    }
    notifyListeners();
  }

  // Separate from recents: reconnect-last keeps its type even when the
  // user has disabled the recent-mixers list.
  static const _lastConnectionKey = 'openfader.lastConnectionType';

  static Future<void> rememberLastConnection(String addr, String driver) async {
    final prefs = await SharedPreferences.getInstance();
    if (!await prefs.setString(
        _lastConnectionKey, jsonEncode({'addr': addr, 'driver': driver}))) {
      throw StateError('The last mixer type could not be saved.');
    }
  }

  static Future<String> savedDriverFor(String addr) async {
    final prefs = await SharedPreferences.getInstance();
    final host = addr.trim().toLowerCase();
    try {
      final last = jsonDecode(prefs.getString(_lastConnectionKey) ?? '{}');
      if (last is Map && '${last['addr']}'.trim().toLowerCase() == host) {
        return '${last['driver'] ?? 'auto'}';
      }
    } catch (_) {
      // Older installs have no separate last-connection record.
    }
    for (final entry in _decode(prefs.getString(key))) {
      if (entry.addr.trim().toLowerCase() == host) return entry.driver;
    }
    return 'auto';
  }

  /// Record a successful connection, most recent first. The caller enables remembering.
  Future<void> remember(String name, String addr, {String? driver}) async {
    if (addr.isEmpty) return;
    final i = _items.indexWhere((r) => r.addr == addr);
    final previous = i < 0 ? null : _items[i];
    final entry = RecentMixerEntry(
        name: name.isEmpty ? previous?.name ?? '' : name,
        addr: addr,
        driver: driver ?? previous?.driver ?? 'auto');
    if (previous != null) {
      if (i == 0 &&
          previous.name == entry.name &&
          previous.driver == entry.driver) {
        return;
      }
      _items.removeAt(i);
    }
    _items.insert(0, entry);
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
