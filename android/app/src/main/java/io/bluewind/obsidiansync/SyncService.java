package io.bluewind.obsidiansync;

import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.app.Service;
import android.content.Context;
import android.content.Intent;
import android.content.pm.ServiceInfo;
import android.os.Build;
import android.os.Handler;
import android.os.IBinder;
import android.os.Looper;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import org.json.JSONObject;

/**
 * 실시간 동기화 포그라운드 서비스.
 * 공통 Studio 백엔드(Native.studioInit / Native.studioApi)와 연동해 알림 상태를 유지하고,
 * 백그라운드·재부팅 후에도 중단 없이 동기화를 실행합니다.
 */
public final class SyncService extends Service {
    static final String STOP = "stop";
    static final String ATTACH_ONLY = "attach_only";
    private static final int ID = 1;
    private final Handler handler = new Handler(Looper.getMainLooper());
    private static final ExecutorService ENGINE = Executors.newSingleThreadExecutor();
    private String lastText = "";
    private volatile boolean destroyed;

    static void start(Context c) {
        ConfigStore.setEnabled(c, true);
        c.startForegroundService(new Intent(c, SyncService.class));
    }

    static void attachForeground(Context c) {
        ConfigStore.setEnabled(c, true);
        c.startForegroundService(new Intent(c, SyncService.class).setAction(ATTACH_ONLY));
    }

    static void stop(Context c) {
        ConfigStore.setEnabled(c, false);
        c.startService(new Intent(c, SyncService.class).setAction(STOP));
    }

    @Override public void onCreate() {
        super.onCreate();
        getSystemService(NotificationManager.class).createNotificationChannel(
                new NotificationChannel("sync", "동기화 상태", NotificationManager.IMPORTANCE_LOW));
    }

    private Notification notification(String text) {
        PendingIntent open = PendingIntent.getActivity(
                this, 0, new Intent(this, MainActivity.class), PendingIntent.FLAG_IMMUTABLE);
        PendingIntent stop = PendingIntent.getService(
                this, 1, new Intent(this, SyncService.class).setAction(STOP), PendingIntent.FLAG_IMMUTABLE);
        return new Notification.Builder(this, "sync")
                .setSmallIcon(R.drawable.ic_stat_sync)
                .setContentTitle("Onyx Studio 실시간 동기화")
                .setContentText(text)
                .setContentIntent(open)
                .setOngoing(true)
                .addAction(new Notification.Action.Builder(null, "중지", stop).build())
                .build();
    }

    @Override public int onStartCommand(Intent intent, int flags, int startId) {
        if (intent != null && STOP.equals(intent.getAction())) {
            ConfigStore.setEnabled(this, false);
            handler.removeCallbacksAndMessages(null);
            ENGINE.execute(() -> {
                Native.studioApi("/api/sync/stop", "{}");
                Native.stop();
            });
            stopForeground(STOP_FOREGROUND_REMOVE);
            stopSelf();
            return START_NOT_STICKY;
        }
        Notification n = notification("시작하는 중…");
        if (Build.VERSION.SDK_INT >= 34) startForeground(ID, n, ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE);
        else startForeground(ID, n);

        boolean attachOnly = intent != null && ATTACH_ONLY.equals(intent.getAction());
        String config = ConfigStore.load(this);
        String dataDir = ConfigStore.dataDir(this);
        handler.removeCallbacksAndMessages(null);
        ENGINE.execute(() -> {
            String initRaw = Native.studioInit(dataDir, config);
            String result = initRaw;
            try {
                if (new JSONObject(initRaw).optBoolean("ok") && !attachOnly) {
                    JSONObject st = new JSONObject(Native.studioSyncNotificationStatus());
                    if (!st.optBoolean("running")) {
                        result = Native.studioApi("/api/sync/start", "{}");
                    }
                }
            } catch (Exception ignored) { }
            final String finalRes = result;
            handler.post(() -> started(finalRes));
        });
        return START_STICKY;
    }

    private void started(String result) {
        if (destroyed) return;
        try {
            JSONObject r = new JSONObject(result);
            if (!r.optBoolean("ok")) {
                update("시작 실패: " + r.optString("error"));
                getSharedPreferences("state", 0).edit().putString("start_error", r.optString("error")).apply();
                return;
            }
        } catch (Exception ignored) { }
        getSharedPreferences("state", 0).edit().remove("start_error").apply();
        handler.removeCallbacksAndMessages(null);
        handler.post(this::refresh);
    }

    private void refresh() {
        try {
            JSONObject st = new JSONObject(Native.studioSyncNotificationStatus());
            String text = st.optString("text", "대기 중");
            update(text);
        } catch (Exception ignored) { }
        handler.postDelayed(this::refresh, 3500);
    }

    private void update(String text) {
        if (text.equals(lastText)) return;
        lastText = text;
        getSystemService(NotificationManager.class).notify(ID, notification(text));
    }

    @Override public void onDestroy() {
        destroyed = true;
        handler.removeCallbacksAndMessages(null);
        ENGINE.execute(() -> {
            Native.studioApi("/api/sync/stop", "{}");
            Native.stop();
        });
        super.onDestroy();
    }

    @Override public IBinder onBind(Intent intent) { return null; }
}
