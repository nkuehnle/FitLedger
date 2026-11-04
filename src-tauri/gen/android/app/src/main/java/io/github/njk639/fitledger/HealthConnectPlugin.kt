package io.github.njk639.fitledger

import android.app.Activity
import androidx.activity.result.ActivityResult
import androidx.health.connect.client.HealthConnectClient
import androidx.health.connect.client.HealthConnectFeatures
import androidx.health.connect.client.PermissionController
import androidx.health.connect.client.permission.HealthPermission
import androidx.health.connect.client.records.ActiveCaloriesBurnedRecord
import androidx.health.connect.client.records.DistanceRecord
import androidx.health.connect.client.records.ExerciseSessionRecord
import androidx.health.connect.client.records.HeartRateRecord
import androidx.health.connect.client.records.HeartRateVariabilityRmssdRecord
import androidx.health.connect.client.records.Record
import androidx.health.connect.client.records.RestingHeartRateRecord
import androidx.health.connect.client.records.SleepSessionRecord
import androidx.health.connect.client.records.StepsRecord
import androidx.health.connect.client.records.WeightRecord
import androidx.health.connect.client.records.metadata.DataOrigin
import androidx.health.connect.client.request.AggregateGroupByPeriodRequest
import androidx.health.connect.client.request.AggregateRequest
import androidx.health.connect.client.request.ReadRecordsRequest
import androidx.health.connect.client.time.TimeRangeFilter
import app.tauri.annotation.ActivityCallback
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSArray
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.time.Instant
import java.time.LocalDate
import java.time.LocalDateTime
import java.time.Period
import java.time.ZoneId
import java.time.ZoneOffset
import java.time.format.DateTimeFormatter
import kotlin.reflect.KClass
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch

@InvokeArg
class HrWindowArg {
    var workoutId: Long = 0
    var start: String = ""
    var end: String = ""
    var utcOffsetMin: Int? = null
}

@InvokeArg
class ReadArgs {
    /** Local wall-clock time "YYYY-MM-DD HH:MM:SS". */
    var since: String = ""
    var windows: Array<HrWindowArg> = arrayOf()
}

/**
 * Reads what watches and health apps (Garmin Connect, Fitbit, Oura, …) wrote to Health Connect:
 * weigh-ins, exercise sessions, heart rate over FitLedger workouts, sleep, resting HR, HRV and steps.
 * Read-only; returns JSON shaped like fitledger_core::wearables::WearableImport.
 */
@TauriPlugin
class HealthConnectPlugin(private val activity: Activity) : Plugin(activity) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    private val readTypes: List<KClass<out Record>> = listOf(
        WeightRecord::class, ExerciseSessionRecord::class, HeartRateRecord::class, DistanceRecord::class,
        ActiveCaloriesBurnedRecord::class, SleepSessionRecord::class, RestingHeartRateRecord::class,
        HeartRateVariabilityRmssdRecord::class, StepsRecord::class,
    )

    private fun client() = HealthConnectClient.getOrCreate(activity)

    private fun wanted(c: HealthConnectClient): Set<String> {
        val p = readTypes.map { HealthPermission.getReadPermission(it) }.toMutableSet()
        if (c.features.getFeatureStatus(HealthConnectFeatures.FEATURE_READ_HEALTH_DATA_HISTORY) ==
            HealthConnectFeatures.FEATURE_STATUS_AVAILABLE
        ) {
            p += HealthPermission.PERMISSION_READ_HEALTH_DATA_HISTORY
        }
        return p
    }

    /** { available: "yes" | "update" | "no", granted: number of data types allowed, total }. */
    @Command
    fun status(invoke: Invoke) {
        val sdk = HealthConnectClient.getSdkStatus(activity)
        val out = JSObject()
        if (sdk != HealthConnectClient.SDK_AVAILABLE) {
            out.put("available", if (sdk == HealthConnectClient.SDK_UNAVAILABLE_PROVIDER_UPDATE_REQUIRED) "update" else "no")
            out.put("granted", 0)
            out.put("total", readTypes.size)
            invoke.resolve(out)
            return
        }
        scope.launch {
            try {
                val granted = client().permissionController.getGrantedPermissions()
                out.put("available", "yes")
                out.put("granted", readTypes.count { HealthPermission.getReadPermission(it) in granted })
                out.put("total", readTypes.size)
                invoke.resolve(out)
            } catch (e: Exception) {
                invoke.reject(e.message ?: e.toString())
            }
        }
    }

    /** Opens Health Connect's permission screen; resolves with status() once the user is back. */
    @Command
    fun connect(invoke: Invoke) {
        if (HealthConnectClient.getSdkStatus(activity) != HealthConnectClient.SDK_AVAILABLE) {
            invoke.reject("Health Connect isn't available on this phone")
            return
        }
        val intent = PermissionController.createRequestPermissionResultContract().createIntent(activity, wanted(client()))
        startActivityForResult(invoke, intent, "connectResult")
    }

    @ActivityCallback
    private fun connectResult(invoke: Invoke, @Suppress("UNUSED_PARAMETER") result: ActivityResult) {
        status(invoke)
    }

    /** Opens Health Connect itself, where sources and priorities are managed. */
    @Command
    fun openSettings(invoke: Invoke) {
        try {
            activity.startActivity(android.content.Intent(HealthConnectClient.ACTION_HEALTH_CONNECT_SETTINGS))
            invoke.resolve()
        } catch (e: Exception) {
            invoke.reject(e.message ?: e.toString())
        }
    }

    @Command
    fun read(invoke: Invoke) {
        val a = invoke.parseArgs(ReadArgs::class.java)
        scope.launch {
            try {
                invoke.resolve(readAll(client(), a))
            } catch (e: Exception) {
                invoke.reject(e.message ?: e.toString())
            }
        }
    }

    private suspend fun readAll(c: HealthConnectClient, a: ReadArgs): JSObject {
        val granted = c.permissionController.getGrantedPermissions()
        fun can(t: KClass<out Record>) = HealthPermission.getReadPermission(t) in granted
        val zone = ZoneId.systemDefault()
        val since = parseLocal(a.since).atZone(zone).toInstant()
        val after = TimeRangeFilter.after(since)
        val out = JSObject()

        val weights = JSArray()
        if (can(WeightRecord::class)) {
            for (r in readRecords(c, WeightRecord::class, after)) {
                weights.put(JSObject().apply {
                    put("at", local(r.time, r.zoneOffset))
                    put("weight_kg", r.weight.inKilograms)
                    put("source", sourceOf(r.metadata.dataOrigin))
                })
            }
        }
        out.put("weights", weights)

        val activities = JSArray()
        if (can(ExerciseSessionRecord::class)) {
            for (r in readRecords(c, ExerciseSessionRecord::class, after)) {
                val range = TimeRangeFilter.between(r.startTime, r.endTime)
                val origin = setOf(r.metadata.dataOrigin)
                val hr = if (can(HeartRateRecord::class)) {
                    c.aggregate(AggregateRequest(setOf(HeartRateRecord.BPM_AVG, HeartRateRecord.BPM_MAX), range))
                } else null
                val dist = if (can(DistanceRecord::class)) {
                    c.aggregate(AggregateRequest(setOf(DistanceRecord.DISTANCE_TOTAL), range, origin))[DistanceRecord.DISTANCE_TOTAL]
                } else null
                val kcal = if (can(ActiveCaloriesBurnedRecord::class)) {
                    c.aggregate(AggregateRequest(setOf(ActiveCaloriesBurnedRecord.ACTIVE_CALORIES_TOTAL), range, origin))[ActiveCaloriesBurnedRecord.ACTIVE_CALORIES_TOTAL]
                } else null
                activities.put(JSObject().apply {
                    put("key", "hc:" + r.metadata.id)
                    put("source", sourceOf(r.metadata.dataOrigin))
                    put("slug", slugOf(r.exerciseType))
                    put("start", local(r.startTime, r.startZoneOffset))
                    put("duration_s", (r.endTime.toEpochMilli() - r.startTime.toEpochMilli()) / 1000.0)
                    put("distance_m", dist?.inMeters)
                    put("avg_hr", hr?.get(HeartRateRecord.BPM_AVG))
                    put("max_hr", hr?.get(HeartRateRecord.BPM_MAX))
                    put("kcal", kcal?.inKilocalories)
                })
            }
        }
        out.put("activities", activities)

        val workoutHr = JSArray()
        if (can(HeartRateRecord::class)) {
            for (w in a.windows) {
                val off = w.utcOffsetMin?.let { ZoneOffset.ofTotalSeconds(it * 60) }
                fun inst(s: String) = if (off != null) parseLocal(s).toInstant(off) else parseLocal(s).atZone(zone).toInstant()
                val hr = c.aggregate(
                    AggregateRequest(setOf(HeartRateRecord.BPM_AVG, HeartRateRecord.BPM_MAX), TimeRangeFilter.between(inst(w.start), inst(w.end))),
                )
                val avg = hr[HeartRateRecord.BPM_AVG] ?: continue
                workoutHr.put(JSObject().apply {
                    put("workout_id", w.workoutId)
                    put("avg_hr", avg)
                    put("max_hr", hr[HeartRateRecord.BPM_MAX])
                })
            }
        }
        out.put("workout_hr", workoutHr)

        // Per local day. Aggregates already merge sources by the user's Health Connect priority.
        val days = sortedMapOf<String, JSObject>()
        fun day(d: LocalDate) = days.getOrPut(d.toString()) { JSObject().apply { put("date", d.toString()); put("source", "health_connect") } }
        val firstDay = since.atZone(zone).toLocalDate()
        if (can(StepsRecord::class)) {
            val groups = c.aggregateGroupByPeriod(
                AggregateGroupByPeriodRequest(
                    setOf(StepsRecord.COUNT_TOTAL),
                    TimeRangeFilter.between(firstDay.atStartOfDay(), LocalDate.now(zone).plusDays(1).atStartOfDay()),
                    Period.ofDays(1),
                ),
            )
            for (g in groups) g.result[StepsRecord.COUNT_TOTAL]?.let { day(g.startTime.toLocalDate()).put("steps", it) }
        }
        if (can(SleepSessionRecord::class)) {
            // The longest night ending on each morning (several apps may each record it).
            val best = mutableMapOf<LocalDate, Long>()
            for (r in readRecords(c, SleepSessionRecord::class, after)) {
                val awake = r.stages
                    .filter { it.stage == SleepSessionRecord.STAGE_TYPE_AWAKE || it.stage == SleepSessionRecord.STAGE_TYPE_OUT_OF_BED }
                    .sumOf { it.endTime.epochSecond - it.startTime.epochSecond }
                val asleep = r.endTime.epochSecond - r.startTime.epochSecond - awake
                val d = r.endTime.atOffset(r.endZoneOffset ?: zone.rules.getOffset(r.endTime)).toLocalDate()
                if (asleep > (best[d] ?: 0)) best[d] = asleep
            }
            for ((d, s) in best) day(d).put("sleep_s", s)
        }
        if (can(RestingHeartRateRecord::class)) {
            val low = mutableMapOf<LocalDate, Long>()
            for (r in readRecords(c, RestingHeartRateRecord::class, after)) {
                val d = r.time.atOffset(r.zoneOffset ?: zone.rules.getOffset(r.time)).toLocalDate()
                low[d] = minOf(low[d] ?: Long.MAX_VALUE, r.beatsPerMinute)
            }
            for ((d, v) in low) day(d).put("resting_hr", v)
        }
        if (can(HeartRateVariabilityRmssdRecord::class)) {
            val all = mutableMapOf<LocalDate, MutableList<Double>>()
            for (r in readRecords(c, HeartRateVariabilityRmssdRecord::class, after)) {
                val d = r.time.atOffset(r.zoneOffset ?: zone.rules.getOffset(r.time)).toLocalDate()
                all.getOrPut(d) { mutableListOf() } += r.heartRateVariabilityMillis
            }
            for ((d, v) in all) day(d).put("hrv_ms", v.average())
        }
        out.put("days", JSArray().apply { days.values.forEach { put(it) } })
        return out
    }

    private suspend fun <T : Record> readRecords(c: HealthConnectClient, type: KClass<T>, range: TimeRangeFilter): List<T> {
        val all = mutableListOf<T>()
        var token: String? = null
        do {
            val res = c.readRecords(ReadRecordsRequest(type, range, pageSize = 1000, pageToken = token))
            all += res.records
            token = res.pageToken
        } while (token != null)
        return all
    }

    companion object {
        private val FMT: DateTimeFormatter = DateTimeFormatter.ofPattern("yyyy-MM-dd HH:mm:ss")

        fun parseLocal(s: String): LocalDateTime = LocalDateTime.parse(s, FMT)

        fun local(t: Instant, off: ZoneOffset?): String =
            LocalDateTime.ofInstant(t, off ?: ZoneId.systemDefault()).format(FMT)

        fun sourceOf(o: DataOrigin): String = when (o.packageName) {
            "com.garmin.android.apps.connectmobile" -> "garmin"
            "com.fitbit.FitbitMobile" -> "fitbit"
            "com.ouraring.oura" -> "oura"
            "com.sec.android.app.shealth" -> "samsung"
            "com.google.android.apps.fitness" -> "google_fit"
            "com.withings.wiscale2" -> "withings"
            else -> o.packageName
        }

        /** Library slug for a cardio session type; null for others (they only lend heart rate). */
        fun slugOf(t: Int): String? = when (t) {
            ExerciseSessionRecord.EXERCISE_TYPE_RUNNING -> "running"
            ExerciseSessionRecord.EXERCISE_TYPE_RUNNING_TREADMILL -> "running-treadmill"
            ExerciseSessionRecord.EXERCISE_TYPE_WALKING -> "walking"
            ExerciseSessionRecord.EXERCISE_TYPE_HIKING -> "hiking"
            ExerciseSessionRecord.EXERCISE_TYPE_BIKING -> "cycling"
            ExerciseSessionRecord.EXERCISE_TYPE_BIKING_STATIONARY -> "cycling-indoor"
            ExerciseSessionRecord.EXERCISE_TYPE_ROWING_MACHINE -> "rowing-machine"
            ExerciseSessionRecord.EXERCISE_TYPE_SWIMMING_POOL,
            ExerciseSessionRecord.EXERCISE_TYPE_SWIMMING_OPEN_WATER -> "swimming"
            ExerciseSessionRecord.EXERCISE_TYPE_ELLIPTICAL -> "elliptical"
            ExerciseSessionRecord.EXERCISE_TYPE_STAIR_CLIMBING,
            ExerciseSessionRecord.EXERCISE_TYPE_STAIR_CLIMBING_MACHINE -> "stair-climber"
            else -> null
        }
    }
}
