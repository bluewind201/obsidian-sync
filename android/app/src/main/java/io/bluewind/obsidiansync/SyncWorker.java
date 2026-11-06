package io.bluewind.obsidiansync;

import android.content.Context;
import androidx.annotation.NonNull;
import androidx.work.Worker;
import androidx.work.WorkerParameters;

/** 실시간 서비스가 시스템에 의해 꺼졌을 때를 대비한 주기 동기화 (15분 이상 간격) */
public final class SyncWorker extends Worker {
    public SyncWorker(@NonNull Context c, @NonNull WorkerParameters p) { super(c, p); }

    @NonNull @Override public Result doWork() {
        Context c = getApplicationContext();
        String config = ConfigStore.load(c);
        if (config.isEmpty() || !ConfigStore.enabled(c)) return Result.success();
        String r = Native.syncOnce(config, ConfigStore.dataDir(c));
        return r.contains("\"ok\":true") ? Result.success() : Result.retry();
    }
}
