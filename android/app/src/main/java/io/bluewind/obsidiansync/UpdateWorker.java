package io.bluewind.obsidiansync;

import android.content.Context;
import androidx.annotation.NonNull;
import androidx.work.Worker;
import androidx.work.WorkerParameters;

/** 몇 시간마다 GitHub 릴리즈를 확인해 새 버전이 있으면 내려받아 설치합니다. */
public final class UpdateWorker extends Worker {
    public UpdateWorker(@NonNull Context c, @NonNull WorkerParameters p) { super(c, p); }

    @NonNull @Override public Result doWork() {
        Context c = getApplicationContext();
        if (!Updater.autoEnabled(c)) return Result.success();
        try {
            Updater.run(c);
            return Result.success();
        } catch (Exception e) {
            Updater.setMessage(c, "업데이트 확인 실패: " + e.getMessage());
            return Result.retry();
        }
    }
}
