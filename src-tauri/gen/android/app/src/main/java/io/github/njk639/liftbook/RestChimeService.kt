package io.github.njk639.liftbook

import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.media.AudioAttributes
import android.media.AudioFocusRequest
import android.media.AudioManager
import android.media.MediaPlayer
import android.os.Build
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import android.os.PowerManager
import android.os.VibrationAttributes
import android.os.VibrationEffect
import android.os.Vibrator
import android.os.VibratorManager

/**
 * The last seconds of a rest: a short buzz at 3, 2 and 1 s left, then a chime and a longer buzz at 0.
 * The chime plays on the media stream (media volume, ducking any music) from a foreground service
 * that runs for the whole rest: started while the app is in front, Android lets it play audio later
 * from the background. It holds a wake lock until the rest ends so the countdown stays on time.
 */
class RestChimeService : Service() {
    companion object {
        const val LEAD_MS = 3_000L
        const val ACTION_STOP = "io.github.njk639.liftbook.STOP_CHIME"
        @Volatile var running = false
    }

    private val handler = Handler(Looper.getMainLooper())
    private var player: MediaPlayer? = null
    private var focus: AudioFocusRequest? = null
    private var wake: PowerManager.WakeLock? = null
    private var lastStartId = 0

    override fun onCreate() {
        super.onCreate()
        running = true
    }

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        lastStartId = startId
        if (intent?.action == ACTION_STOP) {
            // Rest skipped: leave the notification to the app, which updates it. Workout over: remove it.
            reset()
            stopForeground(if (intent.getBooleanExtra("remove", false)) STOP_FOREGROUND_REMOVE else STOP_FOREGROUND_DETACH)
            stopSelf(startId)
            return START_NOT_STICKY
        }
        val endsAt = intent?.getLongExtra("endsAt", 0L) ?: 0L
        val text = intent?.getStringExtra("text") ?: "Time for your next set"
        val workoutStart = intent?.getLongExtra("workoutStart", 0L) ?: 0L
        val title = intent?.getStringExtra("title") ?: "Workout in progress"
        RestNotifications.ensureChannels(this)
        val n = RestNotifications.buildStatus(this, "Rest · $title", text, endsAt, true)
        if (Build.VERSION.SDK_INT >= 29) {
            startForeground(RestNotifications.ID_STATUS, n, ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK)
        } else {
            startForeground(RestNotifications.ID_STATUS, n)
        }
        if (intent == null || endsAt == 0L) {
            finish(0L, "", "")
            return START_NOT_STICKY
        }
        reset()
        wake = getSystemService(PowerManager::class.java)
            .newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "liftbook:rest")
            .apply { acquire((endsAt - System.currentTimeMillis()).coerceAtLeast(0L) + 10_000L) }

        val now = System.currentTimeMillis()
        for (k in 3 downTo 1) {
            val at = endsAt - k * 1000L
            if (at > now) handler.postDelayed({ buzz(120) }, at - now)
        }
        handler.postDelayed({
            buzz(500)
            chime { finish(workoutStart, title, text) }
            RestNotifications.alertRestOver(this, text)
        }, (endsAt - now).coerceAtLeast(0L))
        return START_NOT_STICKY
    }

    private fun buzz(ms: Long) {
        val v: Vibrator = if (Build.VERSION.SDK_INT >= 31) {
            getSystemService(VibratorManager::class.java).defaultVibrator
        } else {
            @Suppress("DEPRECATION")
            getSystemService(Context.VIBRATOR_SERVICE) as Vibrator
        }
        if (Build.VERSION.SDK_INT < 26) {
            @Suppress("DEPRECATION")
            v.vibrate(ms)
            return
        }
        val effect = VibrationEffect.createOneShot(ms, VibrationEffect.DEFAULT_AMPLITUDE)
        // Alarm usage, so it still buzzes with the ringer on silent or vibrate.
        if (Build.VERSION.SDK_INT >= 33) {
            v.vibrate(effect, VibrationAttributes.createForUsage(VibrationAttributes.USAGE_ALARM))
        } else {
            @Suppress("DEPRECATION")
            v.vibrate(effect, AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_ALARM).build())
        }
    }

    private fun chime(done: () -> Unit) {
        val attrs = AudioAttributes.Builder()
            .setUsage(AudioAttributes.USAGE_MEDIA)
            .setContentType(AudioAttributes.CONTENT_TYPE_SONIFICATION)
            .build()
        val am = getSystemService(AudioManager::class.java)
        if (Build.VERSION.SDK_INT >= 26) focus = AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN_TRANSIENT_MAY_DUCK)
            .setAudioAttributes(attrs)
            .build()
            .also { am.requestAudioFocus(it) }
        val p = MediaPlayer()
        player = p
        try {
            p.setAudioAttributes(attrs)
            resources.openRawResourceFd(R.raw.rest_chime).use { p.setDataSource(it.fileDescriptor, it.startOffset, it.length) }
            p.setOnCompletionListener { done() }
            p.setOnErrorListener { _, _, _ -> done(); true }
            p.prepare()
            p.start()
        } catch (e: Exception) {
            done()
        }
    }

    private fun finish(workoutStart: Long, title: String, text: String) {
        reset()
        stopForeground(STOP_FOREGROUND_REMOVE)
        if (workoutStart > 0) RestNotifications.showStatus(this, title, "Ready for your next set", workoutStart, false)
        stopSelf(lastStartId)
    }

    private fun reset() {
        handler.removeCallbacksAndMessages(null)
        player?.release()
        player = null
        if (Build.VERSION.SDK_INT >= 26) focus?.let { getSystemService(AudioManager::class.java).abandonAudioFocusRequest(it) }
        focus = null
        wake?.let { if (it.isHeld) it.release() }
        wake = null
    }

    override fun onDestroy() {
        running = false
        reset()
        super.onDestroy()
    }
}
