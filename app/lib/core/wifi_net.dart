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
    if (defaultTargetPlatform != TargetPlatform.android) return true;
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

  /// Android 13+ separates nearby Wi-Fi access from physical location.
  /// Keep the existing legacy grant on older Android versions; requesting
  /// Location on a modern device must not gate LAN discovery.
  static Future<Permission?> _scanPermission() async {
    final sdk = await _ch.invokeMethod<int>('sdkInt');
    if (sdk == null) return null;
    return sdk >= 33 ? Permission.nearbyWifiDevices : Permission.location;
  }

  static bool _granted(PermissionStatus status) =>
      status.isGranted || status.isLimited || status.isProvisional;

  /// Request only the runtime grant appropriate to this Android version.
  /// Never throws; other platforms have their own network permission flow.
  static Future<bool> ensureScanPermissions() async {
    if (defaultTargetPlatform != TargetPlatform.android) return true;
    try {
      final permission = await _scanPermission();
      return permission != null && _granted(await permission.request());
    } catch (_) {
      return false;
    }
  }

  /// Discovery guidance; directed connections do not require Location.
  static Future<String?> permissionHint() async {
    if (defaultTargetPlatform != TargetPlatform.android) return null;
    try {
      final permission = await _scanPermission();
      if (permission == null || _granted(await permission.status)) return null;
      final name = permission == Permission.nearbyWifiDevices
          ? 'Nearby Wi-Fi devices'
          : 'Location';
      return 'Allow $name to discover mixers '
          '(Android Settings → Apps → OpenFader → Permissions).';
    } catch (_) {
      return null;
    }
  }
}
