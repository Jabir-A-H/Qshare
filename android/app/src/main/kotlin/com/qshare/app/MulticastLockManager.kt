package com.qshare.app

import android.content.Context
import android.net.wifi.WifiManager
import android.util.Log

/**
 * Manages the Android Wi-Fi MulticastLock.
 *
 * Android Wi-Fi chipsets filter out incoming UDP multicast packets (e.g., mDNS on 224.0.0.251:5353)
 * to preserve battery when idle. This manager holds an active multicast lock during LAN discovery
 * to ensure incoming peer announcements are received reliably.
 */
class MulticastLockManager(context: Context) {
    private val wifiManager = context.applicationContext.getSystemService(Context.WIFI_SERVICE) as? WifiManager
    private var multicastLock: WifiManager.MulticastLock? = null

    companion object {
        private const val TAG = "MulticastLockManager"
    }

    fun acquire() {
        if (multicastLock == null) {
            multicastLock = wifiManager?.createMulticastLock("QshareMdnsLock")?.apply {
                setReferenceCounted(true)
            }
        }
        multicastLock?.let {
            if (!it.isHeld) {
                it.acquire()
                Log.i(TAG, "Acquired Wi-Fi MulticastLock for mDNS discovery")
            }
        }
    }

    fun release() {
        multicastLock?.let {
            if (it.isHeld) {
                it.release()
                Log.i(TAG, "Released Wi-Fi MulticastLock")
            }
        }
    }
}
