package io.github.njk639.fitledger

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
 * - [RestChimeService] for the length of the rest: it buzzes on each of the last 3 seconds and
 *   plays a chime on the media stream at 0, even with the screen off or in another app; an exact
 *   alarm 3 s before the end restarts it if it was killed.
 */
object RestNotifications {
    const val CHANNEL_STATUS = "workout_status"
    // v4: silent; RestChimeService plays the chime (media volume) and the countdown vibration.
    // Channels can't be changed once created, hence the new id.
    const val CHANNEL_ALERT = "rest_over_v4"
    private val OLD_CHANNELS = listOf("rest_over", "rest_over_v2", "rest_over_v3")
    const val ID_STATUS = 1001
    const val ID_ALERT = 1002
    const val ACTION_REST_OVER = "io.github.njk639.fitledger.REST_OVER"
    private const val PREFS = "rest_timer"

    fun ensureChannels(ctx: Context) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        val nm = ctx.getSystemService(NotificationManager::class.java)
        OLD_CHANNELS.forEach { nm.deleteNotificationChannel(it) }
        nm.createNotificationChannel(
            NotificationChannel(CHANNEL_STATUS, "Workout in progress", NotificationManager.IMPORTANCE_LOW).apply {
                description = "Rest countdown and workout clock while you train"
                setShowBadge(false)
            },
        )
        nm.createNotificationChannel(
            NotificationChannel(CHANNEL_ALERT, "Rest over", NotificationManager.IMPORTANCE_HIGH).apply {
                description = "Shows when it's time for your next set (the chime follows media volume)"
                enableVibration(false)
                setSound(null, null)
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
        NotificationManagerCompat.from(ctx).notify(ID_STATUS, buildStatus(ctx, title, text, whenMs, countdown))
    }

    fun buildStatus(ctx: Context, title: String, text: String, whenMs: Long, countdown: Boolean): android.app.Notification =
        NotificationCompat.Builder(ctx, CHANNEL_STATUS)
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

    fun alertRestOver(ctx: Context, text: String) {
        ensureChannels(ctx)
        if (!canPost(ctx)) return
        val n = NotificationCompat.Builder(ctx, CHANNEL_ALERT)
            .setSmallIcon(ctx.applicationInfo.icon)
            .setContentTitle("Rest over")
            .setContentText(text)
            .setPriority(NotificationCompat.PRIORITY_HIGH)
            .setSilent(true)
            .setCategory(NotificationCompat.CATEGORY_ALARM)
            .setVisibility(NotificationCompat.VISIBILITY_PUBLIC)
            .setAutoCancel(true)
            .setTimeoutAfter(5 * 60 * 1000L)
            .setContentIntent(openApp(ctx))
            .build()
        NotificationManagerCompat.from(ctx).notify(ID_ALERT, n)
    }

    private fun alarmIntent(ctx: Context, endsAt: Long, text: String, workoutStart: Long, title: String): PendingIntent {
        val i = Intent(ctx, RestOverReceiver::class.java).setAction(ACTION_REST_OVER)
            .putExtra("endsAt", endsAt).putExtra("text", text).putExtra("workoutStart", workoutStart).putExtra("title", title)
        return PendingIntent.getBroadcast(ctx, 0, i, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
    }

    /** Starts the countdown service: from the alarm (allowed in the background) or from the app. */
    fun startChime(ctx: Context, endsAt: Long, text: String, workoutStart: Long, title: String) {
        val i = Intent(ctx, RestChimeService::class.java)
            .putExtra("endsAt", endsAt).putExtra("text", text).putExtra("workoutStart", workoutStart).putExtra("title", title)
        ContextCompat.startForegroundService(ctx, i)
    }

    fun scheduleRestOver(ctx: Context, atMs: Long, text: String, workoutStart: Long, title: String) {
        cancelRestOver(ctx, stopChime = false) // the new start replaces the running countdown
        // Started now, while the app is in front, the service may play audio in the background later;
        // one started by the alarm would be muted. The alarm is the backup if the service gets killed.
        startChime(ctx, atMs, text, workoutStart, title)
        val wakeAt = atMs - RestChimeService.LEAD_MS
        if (wakeAt <= System.currentTimeMillis()) return
        val am = ctx.getSystemService(AlarmManager::class.java)
        val pi = alarmIntent(ctx, atMs, text, workoutStart, title)
        if (Build.VERSION.SDK_INT < 31 || am.canScheduleExactAlarms()) {
            am.setExactAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, wakeAt, pi)
        } else {
            am.setAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, wakeAt, pi)
        }
        ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit().putLong("restEndsAt", atMs).apply()
    }

    fun cancelRestOver(ctx: Context, stopChime: Boolean = true, removeStatus: Boolean = false) {
        val am = ctx.getSystemService(AlarmManager::class.java)
        am.cancel(alarmIntent(ctx, 0, "", 0, ""))
        // Not stopService: that would take the status notification (the service's) with it.
        if (stopChime && RestChimeService.running) {
            ctx.startService(
                Intent(ctx, RestChimeService::class.java).setAction(RestChimeService.ACTION_STOP).putExtra("remove", removeStatus),
            )
        }
        NotificationManagerCompat.from(ctx).cancel(ID_ALERT)
        ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit().remove("restEndsAt").apply()
    }

    fun clearAll(ctx: Context) {
        cancelRestOver(ctx, removeStatus = true)
        NotificationManagerCompat.from(ctx).cancel(ID_STATUS)
    }
}

/** Fires 3 s before the end of a rest and (re)starts the countdown service, even if the app process is gone. */
class RestOverReceiver : BroadcastReceiver() {
    override fun onReceive(ctx: Context, intent: Intent) {
        if (intent.action != RestNotifications.ACTION_REST_OVER) return
        RestNotifications.startChime(
            ctx,
            intent.getLongExtra("endsAt", System.currentTimeMillis()),
            intent.getStringExtra("text") ?: "Time for your next set",
            intent.getLongExtra("workoutStart", 0L),
            intent.getStringExtra("title") ?: "Workout in progress",
        )
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

@InvokeArg
class ReminderArgs {
    var days: IntArray = IntArray(0)
    var hour: Int = 9
    var minute: Int = 0
}

@InvokeArg
class PruneArgs {
    var prefix: String = ""
    var keep: Int = 4
}

@InvokeArg
class DateArgs {
    var date: String = ""
}

@TauriPlugin
class RestTimerPlugin(private val activity: Activity) : Plugin(activity) {
    override fun load(webView: android.webkit.WebView) {
        RestNotifications.ensureChannels(activity)
        WeighInReminder.schedule(activity)
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

    /** Weigh-in reminder days (0 = Sunday) and time; no days turns it off. */
    @Command
    fun weighInReminder(invoke: Invoke) {
        val a = invoke.parseArgs(ReminderArgs::class.java)
        WeighInReminder.configure(activity, a.days.toList(), a.hour, a.minute)
        invoke.resolve()
    }

    @Command
    fun weighedIn(invoke: Invoke) {
        WeighInReminder.weighedIn(activity, invoke.parseArgs(DateArgs::class.java).date)
        invoke.resolve()
    }

    /** Copies an image from the app's cache into the gallery (Pictures/FitLedger). */
    @Command
    fun saveImage(invoke: Invoke) {
        saveTo(invoke, android.provider.MediaStore.Images.Media.EXTERNAL_CONTENT_URI, "Pictures/FitLedger", "Saving to Photos")
    }

    /** Copies a backup or export from the app's cache into Downloads/FitLedger. */
    @Command
    fun saveDownload(invoke: Invoke) {
        saveTo(invoke, android.provider.MediaStore.Downloads.EXTERNAL_CONTENT_URI, "Download/FitLedger", "Saving to Downloads")
    }

    /** Keeps only the newest `keep` files in Downloads/FitLedger whose names start with `prefix`
     *  (names carry the date, so newest sorts last). Files the app no longer owns are skipped. */
    @Command
    fun pruneDownloads(invoke: Invoke) {
        val a = invoke.parseArgs(PruneArgs::class.java)
        if (Build.VERSION.SDK_INT < 29) {
            invoke.resolve()
            return
        }
        val collection = android.provider.MediaStore.Downloads.EXTERNAL_CONTENT_URI
        val resolver = activity.contentResolver
        val found = mutableListOf<Pair<Long, String>>()
        resolver.query(
            collection,
            arrayOf(android.provider.MediaStore.MediaColumns._ID, android.provider.MediaStore.MediaColumns.DISPLAY_NAME),
            "${android.provider.MediaStore.MediaColumns.RELATIVE_PATH} LIKE ? AND ${android.provider.MediaStore.MediaColumns.DISPLAY_NAME} LIKE ?",
            arrayOf("Download/FitLedger%", "${a.prefix}%"),
            null,
        )?.use { c ->
            while (c.moveToNext()) found.add(c.getLong(0) to c.getString(1))
        }
        found.sortedByDescending { it.second }.drop(a.keep).forEach { (id, _) ->
            try {
                resolver.delete(android.content.ContentUris.withAppendedId(collection, id), null, null)
            } catch (e: SecurityException) {
                // Saved by an earlier install of the app: Android won't let us delete it.
            }
        }
        invoke.resolve()
    }

    private fun saveTo(invoke: Invoke, collection: android.net.Uri, folder: String, what: String) {
        val a = invoke.parseArgs(ShareArgs::class.java)
        val file = java.io.File(a.path)
        if (Build.VERSION.SDK_INT < 29) {
            invoke.reject("$what needs Android 10 or newer; use Share instead")
            return
        }
        val values = android.content.ContentValues().apply {
            put(android.provider.MediaStore.MediaColumns.DISPLAY_NAME, file.name)
            put(android.provider.MediaStore.MediaColumns.MIME_TYPE, a.mime)
            put(android.provider.MediaStore.MediaColumns.RELATIVE_PATH, folder)
        }
        val resolver = activity.contentResolver
        val uri = resolver.insert(collection, values)
        if (uri == null) {
            invoke.reject("Couldn't save the file")
            return
        }
        resolver.openOutputStream(uri)!!.use { out -> file.inputStream().use { it.copyTo(out) } }
        invoke.resolve()
    }

    /** Workout finished or cancelled. */
    @Command
    fun clear(invoke: Invoke) {
        RestNotifications.clearAll(activity)
        invoke.resolve()
    }
}
