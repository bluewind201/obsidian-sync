package io.bluewind.obsidiansync;

import android.app.PendingIntent;
import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.content.pm.PackageInstaller;

/** 패키지 설치기가 알려 주는 업데이트 설치 결과를 처리합니다. */
public final class UpdateReceiver extends BroadcastReceiver {
    @SuppressWarnings("deprecation")
    @Override public void onReceive(Context c, Intent i) {
        int status = i.getIntExtra(PackageInstaller.EXTRA_STATUS, PackageInstaller.STATUS_FAILURE);
        switch (status) {
            case PackageInstaller.STATUS_PENDING_USER_ACTION: {
                // 처음 한 번은 Android가 설치 확인을 받습니다. 앱이 열려 있으면 바로 띄우고, 아니면 알림으로 알려 줍니다.
                Intent confirm = i.getParcelableExtra(Intent.EXTRA_INTENT);
                if (confirm == null) return;
                confirm.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
                Updater.setMessage(c, "설치 확인을 기다리는 중");
                if (MainActivity.visible) {
                    try { c.startActivity(confirm); return; } catch (RuntimeException ignored) { }
                }
                PendingIntent tap = PendingIntent.getActivity(c, 0, confirm, PendingIntent.FLAG_IMMUTABLE | PendingIntent.FLAG_UPDATE_CURRENT);
                Updater.notify(c, "Obsidian Sync 새 버전", "눌러서 업데이트를 설치하세요", tap);
                return;
            }
            case PackageInstaller.STATUS_SUCCESS:
                Updater.setMessage(c, "");
                return;
            case PackageInstaller.STATUS_FAILURE_ABORTED:
                Updater.setMessage(c, "업데이트 설치를 취소했습니다");
                return;
            case PackageInstaller.STATUS_FAILURE_CONFLICT:
            case PackageInstaller.STATUS_FAILURE_INCOMPATIBLE: {
                String m = "새 버전의 서명이 지금 앱과 달라 업데이트할 수 없습니다. 앱을 한 번 지우고 새 버전을 설치해 주세요.";
                Updater.setMessage(c, m);
                Updater.notify(c, "Obsidian Sync 업데이트 실패", m, null);
                return;
            }
            default: {
                String msg = i.getStringExtra(PackageInstaller.EXTRA_STATUS_MESSAGE);
                Updater.setMessage(c, "업데이트 설치 실패" + (msg == null ? "" : ": " + msg));
            }
        }
    }
}
