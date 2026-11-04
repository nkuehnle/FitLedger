package io.github.njk639.fitledger

import android.app.AlarmManager
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import java.text.SimpleDateFormat
import java.util.Calendar
import java.util.Locale

/**
 * Morning weigh-in reminder on chosen weekdays. The schedule lives in SharedPreferences so the
 * receiver can re-arm the next alarm (and after a reboot) without the app running. The reminder
 * is skipped when a weigh-in for today has already been logged.
 */
object WeighInReminder {
    private const val PREFS = "weigh_in_reminder"
    private const val CHANNEL = "weigh_in"
    private const val ID = 1003
    const val ACTION = "io.github.njk639.fitledger.WEIGH_IN"

    private fun prefs(ctx: Context) = ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
    private fun todayIso() = SimpleDateFormat("yyyy-MM-dd", Locale.US).format(Calendar.getInstance().time)

    /** [days] use JavaScript numbering: 0 = Sunday … 6 = Saturday. Empty turns the reminder off. */
    fun configure(ctx: Context, days: List<Int>, hour: Int, minute: Int) {
        prefs(ctx).edit().putString("days", days.joinToString(",")).putInt("hour", hour).putInt("minute", minute).apply()
        schedule(ctx)
    }

    fun weighedIn(ctx: Context, date: String) {
        prefs(ctx).edit().putString("last", date).apply()
        if (date == todayIso()) NotificationManagerCompat.from(ctx).cancel(ID)
    }

    private fun pending(ctx: Context): PendingIntent {
        val i = Intent(ctx, WeighInReceiver::class.java).setAction(ACTION)
        return PendingIntent.getBroadcast(ctx, 1, i, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
    }

    /** Arms the alarm for the next chosen day/time (strictly in the future). */
    fun schedule(ctx: Context) {
        val am = ctx.getSystemService(AlarmManager::class.java)
        am.cancel(pending(ctx))
        val p = prefs(ctx)
        val days = p.getString("days", "")!!.split(",").mapNotNull { it.trim().toIntOrNull() }.toSet()
        if (days.isEmpty()) return
        val c = Calendar.getInstance().apply {
            set(Calendar.HOUR_OF_DAY, p.getInt("hour", 9))
            set(Calendar.MINUTE, p.getInt("minute", 0))
            set(Calendar.SECOND, 0)
            set(Calendar.MILLISECOND, 0)
        }
        val now = System.currentTimeMillis()
        for (i in 0..7) {
            if (c.timeInMillis > now && (c.get(Calendar.DAY_OF_WEEK) - 1) in days) break
            c.add(Calendar.DAY_OF_MONTH, 1)
        }
        if (Build.VERSION.SDK_INT < 31 || am.canScheduleExactAlarms()) {
            am.setExactAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, c.timeInMillis, pending(ctx))
        } else {
            am.setAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, c.timeInMillis, pending(ctx))
        }
    }

    fun remind(ctx: Context) {
        if (prefs(ctx).getString("last", "") == todayIso()) return
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            ctx.getSystemService(NotificationManager::class.java).createNotificationChannel(
                NotificationChannel(CHANNEL, "Weigh-in reminder", NotificationManager.IMPORTANCE_DEFAULT).apply {
                    description = "Morning reminder on your weigh-in days"
                },
            )
        }
        val open = ctx.packageManager.getLaunchIntentForPackage(ctx.packageName)!!
            .addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP)
        val n = NotificationCompat.Builder(ctx, CHANNEL)
            .setSmallIcon(ctx.applicationInfo.icon)
            .setContentTitle("Weigh-in day")
            .setContentText("Step on the scale and log it in FitLedger")
            .setAutoCancel(true)
            .setContentIntent(PendingIntent.getActivity(ctx, 2, open, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT))
            .build()
        try {
            NotificationManagerCompat.from(ctx).notify(ID, n)
        } catch (e: SecurityException) {
            // Notifications not allowed.
        }
    }
}

class WeighInReceiver : BroadcastReceiver() {
    override fun onReceive(ctx: Context, intent: Intent) {
        when (intent.action) {
            WeighInReminder.ACTION -> {
                WeighInReminder.remind(ctx)
                WeighInReminder.schedule(ctx)
            }
            Intent.ACTION_BOOT_COMPLETED, Intent.ACTION_MY_PACKAGE_REPLACED, Intent.ACTION_TIMEZONE_CHANGED ->
                WeighInReminder.schedule(ctx)
        }
    }
}
