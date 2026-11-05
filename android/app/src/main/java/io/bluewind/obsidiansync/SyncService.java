package io.bluewind.obsidiansync;
import android.app.*;
import android.content.Intent;
import android.content.pm.ServiceInfo;
import android.os.IBinder;
import java.util.concurrent.*;

public final class SyncService extends Service {
    private ScheduledExecutorService executor;
    @Override public void onCreate() {
        super.onCreate();
        getSystemService(NotificationManager.class).createNotificationChannel(new NotificationChannel("sync", "Vault synchronization", NotificationManager.IMPORTANCE_LOW));
    }
    @Override public int onStartCommand(Intent intent, int flags, int startId) {
        Intent stopIntent = new Intent(this, SyncService.class).setAction("stop");
        if (intent != null && "stop".equals(intent.getAction())) { stopSelf(); return START_NOT_STICKY; }
        PendingIntent open = PendingIntent.getActivity(this, 0, new Intent(this, MainActivity.class), PendingIntent.FLAG_IMMUTABLE);
        PendingIntent stop = PendingIntent.getService(this, 1, stopIntent, PendingIntent.FLAG_IMMUTABLE);
        Notification notification = new Notification.Builder(this, "sync").setSmallIcon(android.R.drawable.stat_notify_sync).setContentTitle("Obsidian Sync").setContentText("실시간 동기화 중 · Android 실행 제한 적용").setContentIntent(open).setOngoing(true).addAction(new Notification.Action.Builder(null, "중지", stop).build()).build();
        startForeground(1, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC);
        if (executor == null) {
            executor = Executors.newSingleThreadScheduledExecutor();
            executor.scheduleWithFixedDelay(() -> ConfigStore.run(this), 0, 15, TimeUnit.SECONDS);
            // Stop before Android 15's aggregate six-hour dataSync limit; periodic work remains.
            new android.os.Handler(getMainLooper()).postDelayed(this::stopSelf, TimeUnit.HOURS.toMillis(5));
        }
        return START_NOT_STICKY;
    }
    @Override public void onTimeout(int startId, int fgsType) { stopSelf(); }
    @Override public void onDestroy() { if (executor != null) executor.shutdownNow(); super.onDestroy(); }
    @Override public IBinder onBind(Intent intent) { return null; }
}
