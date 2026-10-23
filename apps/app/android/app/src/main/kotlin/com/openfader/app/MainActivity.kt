package com.openfader.app

import android.content.Context
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.net.NetworkRequest
import android.net.wifi.WifiManager
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel

class MainActivity : FlutterActivity() {
    private var netCallback: ConnectivityManager.NetworkCallback? = null
    private var multicastLock: WifiManager.MulticastLock? = null

    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        MethodChannel(
            flutterEngine.dartExecutor.binaryMessenger,
            "openfader/wifi_net"
        ).setMethodCallHandler { call, result ->
            when (call.method) {
                "bindWifi" -> result.success(bindWifi())
                "unbindWifi" -> result.success(unbindWifi())
                "scanStart" -> result.success(scanStart())
                "scanStop" -> result.success(scanStop())
                else -> result.notImplemented()
            }
        }
    }

    private fun connectivity(): ConnectivityManager? =
        getSystemService(Context.CONNECTIVITY_SERVICE) as? ConnectivityManager

    // Pin process traffic to a Wi-Fi network before probe/connect so the
    // desk unicast does not route out the mobile interface.
    private fun bindWifi(): Boolean {
        return try {
            val cm = connectivity() ?: return false
            if (netCallback != null) return true
            val req = NetworkRequest.Builder()
                .addTransportType(NetworkCapabilities.TRANSPORT_WIFI)
                .build()
            val cb = object : ConnectivityManager.NetworkCallback() {
                override fun onAvailable(network: Network) {
                    try {
                        cm.bindProcessToNetwork(network)
                    } catch (_: Exception) {
                    }
                }
            }
            netCallback = cb
            cm.requestNetwork(req, cb)
            true
        } catch (_: Exception) {
            false
        }
    }

    private fun unbindWifi(): Boolean {
        return try {
            val cm = connectivity()
            netCallback?.let { cb ->
                try {
                    cm?.unregisterNetworkCallback(cb)
                } catch (_: Exception) {
                }
            }
            netCallback = null
            try {
                cm?.bindProcessToNetwork(null)
            } catch (_: Exception) {
            }
            true
        } catch (_: Exception) {
            false
        }
    }

    private fun scanStart(): Boolean {
        return try {
            val wm = applicationContext.getSystemService(Context.WIFI_SERVICE) as? WifiManager
                ?: return false
            val lock = multicastLock ?: wm.createMulticastLock("openfader_scan").also {
                it.setReferenceCounted(true)
                multicastLock = it
            }
            if (!lock.isHeld) lock.acquire()
            true
        } catch (_: Exception) {
            false
        }
    }

    private fun scanStop(): Boolean {
        return try {
            val lock = multicastLock
            if (lock != null && lock.isHeld) lock.release()
            true
        } catch (_: Exception) {
            false
        }
    }

    override fun onPause() {
        // Never hold the multicast lock while backgrounded.
        try {
            val lock = multicastLock
            if (lock != null && lock.isHeld) lock.release()
        } catch (_: Exception) {
        }
        super.onPause()
    }
}
