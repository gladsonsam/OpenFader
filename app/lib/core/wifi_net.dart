// Android Wi-Fi seam for desk discovery (UDP broadcast/mDNS + unicast).
//
// The desk lives on Wi-Fi (e.g. 192.168.1.120) while the phone may also
// have mobile data. Without binding, Android can route the probe/scan out
// the mobile interface and the desk silently never replies. These helpers
// choose the route before probe/connect ([prepareConnection]), hold a
// multicast lock around scan ([scanStart]/[scanStop]) so discovery replies
// are delivered, and request the runtime grants discovery needs
// ([ensureScanPermissions]).
//
// All calls are safe on non-Android / missing platform channel: they catch
// and return false so the sim loopback path keeps working.
library;

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:permission_handler/permission_handler.dart';

class WifiNet {
  static const _ch = MethodChannel('openfader/wifi_net');

  /// Choose the route for a resolved numeric host before opening sockets.
  /// Unicast identification needs INTERNET, not location/nearby scan grants.
  static Future<bool> prepareConnection(String host) async {
    if (defaultTargetPlatform != TargetPlatform.android) return true;
    try {
      return await _ch
              .invokeMethod<bool>('prepareConnection', {'host': host}) ??
          false;
    } catch (_) {
      return false;
    }
  }

  /// Pin process traffic to Wi-Fi before probe/connect. No-op off Android.
  static Future<bool> bindWifi() async {
    try {
      final v = await _ch.invokeMethod<bool>('bindWifi');
      return v ?? false;
    } catch (_) {
      return false;
    }
  }

  static Future<bool> unbindWifi() async {
    try {
      final v = await _ch.invokeMethod<bool>('unbindWifi');
      return v ?? false;
    } catch (_) {
      return false;
    }
  }

  static Future<bool> scanStart() async {
    try {
      final v = await _ch.invokeMethod<bool>('scanStart');
      return v ?? false;
    } catch (_) {
      return false;
    }
  }

  static Future<bool> scanStop() async {
    try {
      final v = await _ch.invokeMethod<bool>('scanStop');
      return v ?? false;
    } catch (_) {
      return false;
    }
  }

  /// Runtime grants needed for LAN discovery on Android:
  /// location (scan) + nearby Wi-Fi devices (API 33+). Returns true when
  /// scanning may proceed. Never throws; desktop/test returns true.
  static Future<bool> ensureScanPermissions() async {
    if (defaultTargetPlatform != TargetPlatform.android) return true;
    try {
      // Nearby-devices first where it exists; requesting an unavailable
      // permission is a no-op grant on older APIs.
      final nearby = await Permission.nearbyWifiDevices.request();
      final loc = await Permission.location.request();
      final okNearby =
          nearby.isGranted || nearby.isLimited || nearby.isProvisional;
      final okLoc = loc.isGranted || loc.isLimited || loc.isProvisional;
      // Pre-Android-13 devices have no nearby-devices permission: location
      // alone gates the scan there.
      if (!await Permission.nearbyWifiDevices.shouldShowRequestRationale) {
        // Cannot reliably detect absence; accept location-only as OK when
        // the nearby status is restricted (never asked / not applicable).
        if (nearby.isRestricted && okLoc) return true;
      }
      return okNearby && okLoc;
    } catch (_) {
      return false;
    }
  }

  /// Human-readable denial hint for the Link tab / boot screen.
  static Future<String?> permissionHint() async {
    if (defaultTargetPlatform != TargetPlatform.android) return null;
    try {
      final nearby = await Permission.nearbyWifiDevices.status;
      final loc = await Permission.location.status;
      final missing = <String>[];
      if (!nearby.isGranted && !nearby.isLimited && !nearby.isRestricted) {
        missing.add('Nearby Wi-Fi devices');
      }
      if (!loc.isGranted && !loc.isLimited) missing.add('Location');
      if (missing.isEmpty) return null;
      return 'Grant ${missing.join(' + ')} to scan/probe the desk '
          '(Android Settings → Apps → OpenFader → Permissions).';
    } catch (_) {
      return null;
    }
  }
}
