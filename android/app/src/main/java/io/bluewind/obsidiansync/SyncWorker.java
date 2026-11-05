package io.bluewind.obsidiansync;
import android.content.Context;
import androidx.work.*;
public final class SyncWorker extends Worker {
    public SyncWorker(Context context, WorkerParameters parameters) { super(context, parameters); }
    @Override public Result doWork() {
        String result = ConfigStore.run(getApplicationContext());
        return result.contains("\"ok\":true") ? Result.success() : Result.retry();
    }
}
