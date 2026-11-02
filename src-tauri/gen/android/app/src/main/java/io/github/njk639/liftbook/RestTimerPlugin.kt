package io.github.njk639.liftbook

import android.Manifest
import android.app.Activity
import android.app.AlarmManager
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.media.AudioAttributes
import android.media.RingtoneManager
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.Plugin

/**
 * Rest timer and workout status outside the app:
 * - an ongoing notification in the shade (rest countdown, or the workout clock between sets),
 *   tapping it opens the app;
 * - an exact alarm at the end of the rest that vibrates and alerts even with the screen off,
 *   in another app, or after the app was swiped away.
 */
object RestNotifications {
    const val CHANNEL_STATUS = "workout_status"
    const val CHANNEL_ALERT = "rest_over"
    const val ID_STATUS = 1001
    const val ID_ALERT = 1002
    const val ACTION_REST_OVER = "io.github.njk639.liftbook.REST_OVER"
    private const val PREFS = "rest_timer"

    fun ensureChannels(ctx: Context) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        val nm = ctx.getSystemService(NotificationManager::class.java)
        nm.createNotificationChannel(
            NotificationChannel(CHANNEL_STATUS, "Workout in progress", NotificationManager.IMPORTANCE_LOW).apply {
                description = "Rest countdown and workout clock while you train"
                setShowBadge(false)
            },
        )
        nm.createNotificationChannel(
            NotificationChannel(CHANNEL_ALERT, "Rest over", NotificationManager.IMPORTANCE_HIGH).apply {
                description = "Alerts you when it's time for your next set"
                enableVibration(true)
                vibrationPattern = longArrayOf(0, 200, 120, 200)
                setSound(
                    RingtoneManager.getDefaultUri(RingtoneManager.TYPE_NOTIFICATION),
                    AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_ALARM).build(),
                )
            },
        )
    }

    private fun openApp(ctx: Context): PendingIntent {
        val intent = ctx.packageManager.getLaunchIntentForPackage(ctx.packageName)!!
            .addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP)
        return PendingIntent.getActivity(ctx, 0, intent, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
    }

    private fun canPost(ctx: Context): Boolean =
        Build.VERSION.SDK_INT < 33 ||
            ContextCompat.checkSelfPermission(ctx, Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED

    /** Ongoing status: counts down to [whenMs] when [countdown], else counts up from it. */
    fun showStatus(ctx: Context, title: String, text: String, whenMs: Long, countdown: Boolean) {
        ensureChannels(ctx)
        if (!canPost(ctx)) return
        val n = NotificationCompat.Builder(ctx, CHANNEL_STATUS)
            .setSmallIcon(ctx.applicationInfo.icon)
            .setContentTitle(title)
            .setContentText(text)
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .setSilent(true)
            .setCategory(NotificationCompat.CATEGORY_STOPWATCH)
            .setVisibility(NotificationCompat.VISIBILITY_PUBLIC)
            .setUsesChronometer(true)
            .setChronometerCountDown(countdown)
            .setWhen(whenMs)
            .setShowWhen(true)
            .setContentIntent(openApp(ctx))
            .build()
        NotificationManagerCompat.from(ctx).notify(ID_STATUS, n)
    }

    // The "Rest over" channel carries the vibration (200 ms, pause, 200 ms) and sound; a direct
    // Vibrator call would be ignored when the app is in the background.
    fun alertRestOver(ctx: Context, text: String) {
        ensureChannels(ctx)
        if (!canPost(ctx)) return
        val n = NotificationCompat.Builder(ctx, CHANNEL_ALERT)
            .setSmallIcon(ctx.applicationInfo.icon)
            .setContentTitle("Rest over")
            .setContentText(text)
            .setPriority(NotificationCompat.PRIORITY_HIGH)
            .setVibrate(longArrayOf(0, 200, 120, 200))
            .setCategory(NotificationCompat.CATEGORY_ALARM)
            .setVisibility(NotificationCompat.VISIBILITY_PUBLIC)
            .setAutoCancel(true)
            .setTimeoutAfter(5 * 60 * 1000L)
            .setContentIntent(openApp(ctx))
            .build()
        NotificationManagerCompat.from(ctx).notify(ID_ALERT, n)
    }

    private fun alarmIntent(ctx: Context, text: String, workoutStart: Long, title: String): PendingIntent {
        val i = Intent(ctx, RestOverReceiver::class.java).setAction(ACTION_REST_OVER)
            .putExtra("text", text).putExtra("workoutStart", workoutStart).putExtra("title", title)
        return PendingIntent.getBroadcast(ctx, 0, i, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
    }

    fun scheduleRestOver(ctx: Context, atMs: Long, text: String, workoutStart: Long, title: String) {
        val am = ctx.getSystemService(AlarmManager::class.java)
        val pi = alarmIntent(ctx, text, workoutStart, title)
        if (Build.VERSION.SDK_INT < 31 || am.canScheduleExactAlarms()) {
            am.setExactAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, atMs, pi)
        } else {
            am.setAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, atMs, pi)
        }
        ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit().putLong("restEndsAt", atMs).apply()
    }

    fun cancelRestOver(ctx: Context) {
        val am = ctx.getSystemService(AlarmManager::class.java)
        am.cancel(alarmIntent(ctx, "", 0, ""))
        NotificationManagerCompat.from(ctx).cancel(ID_ALERT)
        ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit().remove("restEndsAt").apply()
    }

    fun clearAll(ctx: Context) {
        cancelRestOver(ctx)
        NotificationManagerCompat.from(ctx).cancel(ID_STATUS)
    }
}

/** Fires at the end of a rest, even if the app process is gone. */
class RestOverReceiver : BroadcastReceiver() {
    override fun onReceive(ctx: Context, intent: Intent) {
        if (intent.action != RestNotifications.ACTION_REST_OVER) return
        val text = intent.getStringExtra("text") ?: "Time for your next set"
        RestNotifications.alertRestOver(ctx, text)
        val start = intent.getLongExtra("workoutStart", 0L)
        val title = intent.getStringExtra("title") ?: "Workout in progress"
        if (start > 0) RestNotifications.showStatus(ctx, title, "Ready for your next set", start, false)
    }
}

@InvokeArg
class StatusArgs {
    var title: String = "Workout in progress"
    var text: String = ""
    var whenMs: Long = 0
    var countdown: Boolean = false
}

@InvokeArg
class ShareArgs {
    var path: String = ""
    var mime: String = "application/octet-stream"
    var title: String = "Share"
}

@InvokeArg
class RestArgs {
    var endsAt: Long = 0
    var workoutStart: Long = 0
    var title: String = "Workout in progress"
    var next: String = "Time for your next set"
}

@TauriPlugin
class RestTimerPlugin(private val activity: Activity) : Plugin(activity) {
    override fun load(webView: android.webkit.WebView) {
        RestNotifications.ensureChannels(activity)
        if (Build.VERSION.SDK_INT >= 33 &&
            ContextCompat.checkSelfPermission(activity, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        ) {
            activity.requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS), 4711)
        }
    }

    /** Workout clock between sets (counts up from the workout start). */
    @Command
    fun status(invoke: Invoke) {
        val a = invoke.parseArgs(StatusArgs::class.java)
        RestNotifications.cancelRestOver(activity)
        RestNotifications.showStatus(activity, a.title, a.text, a.whenMs, a.countdown)
        invoke.resolve()
    }

    /** Starts (or moves) a rest: countdown in the shade plus an alarm at the end. */
    @Command
    fun rest(invoke: Invoke) {
        val a = invoke.parseArgs(RestArgs::class.java)
        RestNotifications.showStatus(activity, "Rest · ${a.title}", a.next, a.endsAt, true)
        RestNotifications.scheduleRestOver(activity, a.endsAt, a.next, a.workoutStart, a.title)
        invoke.resolve()
    }

    /** Opens the Android share sheet for a file in the app's cache folder. */
    @Command
    fun shareFile(invoke: Invoke) {
        val a = invoke.parseArgs(ShareArgs::class.java)
        val file = java.io.File(a.path)
        val uri = androidx.core.content.FileProvider.getUriForFile(activity, "${activity.packageName}.fileprovider", file)
        val send = Intent(Intent.ACTION_SEND)
            .setType(a.mime)
            .putExtra(Intent.EXTRA_STREAM, uri)
            .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        activity.startActivity(Intent.createChooser(send, a.title))
        invoke.resolve()
    }

    /** Workout finished or cancelled. */
    @Command
    fun clear(invoke: Invoke) {
        RestNotifications.clearAll(activity)
        invoke.resolve()
    }
}
