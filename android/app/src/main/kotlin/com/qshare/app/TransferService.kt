package com.qshare.app

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.IBinder
import androidx.core.app.NotificationCompat

/**
 * Android Foreground Service for P2P File Transfers.
 *
 * Ensures multi-gigabyte transfers are not terminated when the user leaves the application
 * or locks their device.
 */
class TransferService : Service() {

    companion object {
        const val CHANNEL_ID = "qshare_transfer_channel"
        const val NOTIFICATION_ID = 1001
        const val ACTION_START_TRANSFER = "com.qshare.app.START_TRANSFER"
        const val ACTION_STOP_TRANSFER = "com.qshare.app.STOP_TRANSFER"
        const val EXTRA_FILENAME = "extra_filename"

        fun start(context: Context, filename: String) {
            val intent = Intent(context, TransferService::class.java).apply {
                action = ACTION_START_TRANSFER
                putExtra(EXTRA_FILENAME, filename)
            }
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                context.startForegroundService(intent)
            } else {
                context.startService(intent)
            }
        }

        fun stop(context: Context) {
            val intent = Intent(context, TransferService::class.java).apply {
                action = ACTION_STOP_TRANSFER
            }
            context.startService(intent)
        }
    }

    override fun onCreate() {
        super.onCreate()
        createNotificationChannel()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        when (intent?.action) {
            ACTION_START_TRANSFER -> {
                val filename = intent.getStringExtra(EXTRA_FILENAME) ?: "File"
                startForeground(NOTIFICATION_ID, buildNotification(filename, 0, 100))
            }
            ACTION_STOP_TRANSFER -> {
                stopForeground(STOP_FOREGROUND_REMOVE)
                stopSelf()
            }
        }
        return START_NOT_STICKY
    }

    override fun onBind(intent: Intent?): IBinder? = null

    private fun buildNotification(filename: String, progress: Int, max: Int): Notification {
        val launchIntent = Intent(this, MainActivity::class.java)
        val pendingIntent = PendingIntent.getActivity(
            this,
            0,
            launchIntent,
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
        )

        return NotificationCompat.Builder(this, CHANNEL_ID)
            .setContentTitle("Qshare Transfer Active")
            .setContentText("Transferring $filename...")
            .setSmallIcon(android.R.drawable.stat_sys_download)
            .setProgress(max, progress, false)
            .setOngoing(true)
            .setContentIntent(pendingIntent)
            .build()
    }

    private fun createNotificationChannel() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val channel = NotificationChannel(
                CHANNEL_ID,
                "Qshare File Transfers",
                NotificationManager.IMPORTANCE_LOW
            ).apply {
                description = "Shows live progress for P2P file transfers"
            }
            val manager = getSystemService(NotificationManager::class.java)
            manager.createNotificationChannel(channel)
        }
    }
}
