package io.bluewind.obsidiansync;

import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;

/** 휴대폰을 다시 켜면 동기화를 자동으로 다시 시작 */
public final class BootReceiver extends BroadcastReceiver {
    @Override public void onReceive(Context c, Intent i) {
        String a = i.getAction();
        if ((Intent.ACTION_BOOT_COMPLETED.equals(a) || Intent.ACTION_MY_PACKAGE_REPLACED.equals(a)) && ConfigStore.enabled(c)) {
            try { c.startForegroundService(new Intent(c, SyncService.class)); } catch (RuntimeException ignored) { }
        }
    }
}
