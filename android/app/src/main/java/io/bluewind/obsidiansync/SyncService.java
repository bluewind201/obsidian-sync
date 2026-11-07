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
import org.json.JSONObject;

/**
 * 실시간 동기화 서비스. Rust 엔진이 볼트 폴더를 감시하고 주기적으로 원격을 확인합니다.
 * 알림을 띄운 채 계속 실행되며, 알림의 '중지' 버튼으로 끌 수 있습니다.
 */
public final class SyncService extends Service {
    static final String STOP = "stop";
    private static final int ID = 1;
    private final Handler handler = new Handler(Looper.getMainLooper());
    private String lastText = "";

    static void start(Context c) {
        ConfigStore.setEnabled(c, true);
        c.startForegroundService(new Intent(c, SyncService.class));
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
        PendingIntent open = PendingIntent.getActivity(this, 0, new Intent(this, MainActivity.class), PendingIntent.FLAG_IMMUTABLE);
        PendingIntent stop = PendingIntent.getService(this, 1, new Intent(this, SyncService.class).setAction(STOP), PendingIntent.FLAG_IMMUTABLE);
        return new Notification.Builder(this, "sync")
                .setSmallIcon(R.drawable.ic_stat_sync)
                .setContentTitle("Obsidian 실시간 동기화")
                .setContentText(text)
                .setContentIntent(open)
                .setOngoing(true)
                .addAction(new Notification.Action.Builder(null, "중지", stop).build())
                .build();
    }

    @Override public int onStartCommand(Intent intent, int flags, int startId) {
        if (intent != null && STOP.equals(intent.getAction())) {
            ConfigStore.setEnabled(this, false);
            Native.stop();
            stopForeground(STOP_FOREGROUND_REMOVE);
            stopSelf();
            return START_NOT_STICKY;
        }
        Notification n = notification("시작하는 중…");
        if (Build.VERSION.SDK_INT >= 34) startForeground(ID, n, ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE);
        else startForeground(ID, n);

        String config = ConfigStore.load(this);
        String result = config.isEmpty() ? "{\"ok\":false,\"error\":\"설정이 없습니다\"}" : Native.start(config, ConfigStore.dataDir(this));
        try {
            JSONObject r = new JSONObject(result);
            if (!r.optBoolean("ok")) {
                update("시작 실패: " + r.optString("error"));
                getSharedPreferences("state", 0).edit().putString("start_error", r.optString("error")).apply();
                return START_NOT_STICKY;
            }
        } catch (Exception ignored) { }
        getSharedPreferences("state", 0).edit().remove("start_error").apply();
        handler.removeCallbacksAndMessages(null);
        handler.post(this::refresh);
        // 시스템이 종료시켜도 다시 시작
        return START_STICKY;
    }

    private void refresh() {
        try {
            JSONObject s = new JSONObject(Native.status());
            String text;
            if (s.optBoolean("syncing")) text = "동기화 중…";
            else if (!s.isNull("last_error") && !s.optString("last_error").isEmpty()) text = "확인 필요: " + s.optString("last_error");
            else if (!s.isNull("last_sync")) text = "마지막 동기화 " + s.optString("last_sync").substring(11) + " · " + s.optString("last_summary");
            else text = "대기 중";
            update(text);
        } catch (Exception ignored) { }
        handler.postDelayed(this::refresh, 5000);
    }

    private void update(String text) {
        if (text.equals(lastText)) return;
        lastText = text;
        getSystemService(NotificationManager.class).notify(ID, notification(text));
    }

    @Override public void onDestroy() {
        handler.removeCallbacksAndMessages(null);
        Native.stop();
        super.onDestroy();
    }

    @Override public IBinder onBind(Intent intent) { return null; }
}
