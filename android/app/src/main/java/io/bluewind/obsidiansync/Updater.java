package io.bluewind.obsidiansync;

import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.content.Context;
import android.content.Intent;
import android.content.SharedPreferences;
import android.content.pm.PackageInfo;
import android.content.pm.PackageInstaller;
import android.os.Build;
import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.HttpURLConnection;
import java.net.URL;
import java.nio.charset.StandardCharsets;
import org.json.JSONArray;
import org.json.JSONObject;

/**
 * GitHub 릴리즈에서 새 버전을 찾아 내려받고 설치합니다.
 * 릴리즈 태그는 v<버전>-b<빌드번호> 이고, 빌드번호가 곧 Android versionCode 입니다.
 * 처음 한 번은 Android가 설치 확인을 묻고, 그 뒤로는 (Android 12+) 확인 없이 업데이트될 수 있습니다.
 */
final class Updater {
    static final String REPO = "bluewind201/obsidian-sync";
    static final String ASSET = "ObsidianSync-android.apk";
    private static final String PREFS = "updater";
    private static final String CHANNEL = "update";
    static final int NOTIFY_ID = 7;

    static final class Release {
        final String tag;
        final long build;
        final String url;
        final long size;

        Release(String tag, long build, String url, long size) {
            this.tag = tag;
            this.build = build;
            this.url = url;
            this.size = size;
        }
    }

    private Updater() { }

    /** "v0.4.0-b12" → 12. 형식이 다르면 -1. */
    static long buildOf(String tag) {
        if (tag == null) return -1;
        int i = tag.lastIndexOf("-b");
        if (i < 0) return -1;
        try { return Long.parseLong(tag.substring(i + 2)); } catch (NumberFormatException e) { return -1; }
    }

    static long currentBuild(Context c) {
        try {
            PackageInfo i = c.getPackageManager().getPackageInfo(c.getPackageName(), 0);
            return i.getLongVersionCode();
        } catch (Exception e) {
            return Long.MAX_VALUE;
        }
    }

    static String currentName(Context c) {
        try {
            String n = c.getPackageManager().getPackageInfo(c.getPackageName(), 0).versionName;
            return n == null ? "" : n;
        } catch (Exception e) {
            return "";
        }
    }

    static SharedPreferences prefs(Context c) { return c.getSharedPreferences(PREFS, Context.MODE_PRIVATE); }

    static boolean autoEnabled(Context c) { return prefs(c).getBoolean("auto", true); }

    static void setAutoEnabled(Context c, boolean on) { prefs(c).edit().putBoolean("auto", on).apply(); }

    /** 마지막 업데이트 시도 결과 (화면에 보여 줄 문장). */
    static String lastMessage(Context c) { return prefs(c).getString("message", ""); }

    static void setMessage(Context c, String m) { prefs(c).edit().putString("message", m).apply(); }

    /** 최신 릴리즈. 실패하면 예외. */
    static Release latest() throws Exception {
        HttpURLConnection h = (HttpURLConnection) new URL("https://api.github.com/repos/" + REPO + "/releases/latest").openConnection();
        h.setRequestProperty("Accept", "application/vnd.github+json");
        h.setRequestProperty("User-Agent", "ObsidianSync-Android");
        h.setConnectTimeout(15000);
        h.setReadTimeout(15000);
        try {
            if (h.getResponseCode() != 200) throw new Exception("GitHub 응답 " + h.getResponseCode());
            String body;
            try (InputStream in = h.getInputStream()) { body = new String(readAll(in), StandardCharsets.UTF_8); }
            return parse(new JSONObject(body));
        } finally {
            h.disconnect();
        }
    }

    static Release parse(JSONObject r) {
        String tag = r.optString("tag_name");
        JSONArray assets = r.optJSONArray("assets");
        if (assets == null) return null;
        for (int i = 0; i < assets.length(); i++) {
            JSONObject a = assets.optJSONObject(i);
            if (a != null && ASSET.equals(a.optString("name"))) {
                return new Release(tag, buildOf(tag), a.optString("browser_download_url"), a.optLong("size", -1));
            }
        }
        return null;
    }

    /** 이 기기보다 새 버전이 있으면 그 릴리즈, 없으면 null. */
    static Release check(Context c) throws Exception {
        Release r = latest();
        return r != null && r.build > currentBuild(c) ? r : null;
    }

    static File download(Context c, Release r) throws Exception {
        File dir = new File(c.getCacheDir(), "update");
        File[] old = dir.listFiles();
        if (old != null) for (File f : old) if (!f.getName().equals(r.tag + ".apk")) f.delete();
        if (!dir.isDirectory() && !dir.mkdirs()) throw new Exception("임시 폴더를 만들 수 없습니다");
        File out = new File(dir, r.tag + ".apk");
        if (out.isFile() && r.size > 0 && out.length() == r.size) return out;
        File part = new File(dir, r.tag + ".part");
        HttpURLConnection h = (HttpURLConnection) new URL(r.url).openConnection();
        h.setInstanceFollowRedirects(true);
        h.setRequestProperty("User-Agent", "ObsidianSync-Android");
        h.setConnectTimeout(15000);
        h.setReadTimeout(60000);
        try (InputStream in = h.getInputStream(); OutputStream os = new FileOutputStream(part)) {
            if (h.getResponseCode() != 200) throw new Exception("내려받기 실패 (" + h.getResponseCode() + ")");
            byte[] buf = new byte[64 * 1024];
            for (int n; (n = in.read(buf)) > 0; ) os.write(buf, 0, n);
        } finally {
            h.disconnect();
        }
        if (r.size > 0 && part.length() != r.size) throw new Exception("내려받은 파일 크기가 맞지 않습니다");
        if (!part.renameTo(out)) throw new Exception("내려받은 파일을 저장할 수 없습니다");
        return out;
    }

    /** Android 패키지 설치기로 넘깁니다. 결과는 UpdateReceiver 로 옵니다. */
    static void install(Context c, File apk) throws Exception {
        PackageInstaller pi = c.getPackageManager().getPackageInstaller();
        PackageInstaller.SessionParams p = new PackageInstaller.SessionParams(PackageInstaller.SessionParams.MODE_FULL_INSTALL);
        p.setAppPackageName(c.getPackageName());
        p.setSize(apk.length());
        if (Build.VERSION.SDK_INT >= 31) p.setRequireUserAction(PackageInstaller.SessionParams.USER_ACTION_NOT_REQUIRED);
        int id = pi.createSession(p);
        try (PackageInstaller.Session s = pi.openSession(id)) {
            try (InputStream in = new FileInputStream(apk); OutputStream out = s.openWrite("base.apk", 0, apk.length())) {
                byte[] buf = new byte[64 * 1024];
                for (int n; (n = in.read(buf)) > 0; ) out.write(buf, 0, n);
                s.fsync(out);
            }
            Intent done = new Intent(c, UpdateReceiver.class);
            int flags = PendingIntent.FLAG_UPDATE_CURRENT | (Build.VERSION.SDK_INT >= 31 ? PendingIntent.FLAG_MUTABLE : 0);
            s.commit(PendingIntent.getBroadcast(c, id, done, flags).getIntentSender());
        }
    }

    /** 확인하고, 새 버전이 있으면 내려받아 설치까지. 새 버전이 없으면 null, 있으면 그 태그. */
    static String run(Context c) throws Exception {
        Release r = check(c);
        if (r == null) return null;
        setMessage(c, "새 버전 " + r.tag + " 내려받는 중");
        File apk = download(c, r);
        setMessage(c, "새 버전 " + r.tag + " 설치 중");
        install(c, apk);
        return r.tag;
    }

    static void notify(Context c, String title, String text, PendingIntent tap) {
        NotificationManager nm = c.getSystemService(NotificationManager.class);
        nm.createNotificationChannel(new NotificationChannel(CHANNEL, "앱 업데이트", NotificationManager.IMPORTANCE_DEFAULT));
        Notification.Builder b = new Notification.Builder(c, CHANNEL)
                .setSmallIcon(android.R.drawable.stat_sys_download_done)
                .setContentTitle(title)
                .setContentText(text)
                .setStyle(new Notification.BigTextStyle().bigText(text))
                .setAutoCancel(true);
        if (tap != null) b.setContentIntent(tap);
        try { nm.notify(NOTIFY_ID, b.build()); } catch (SecurityException ignored) { }
    }

    private static byte[] readAll(InputStream in) throws Exception {
        java.io.ByteArrayOutputStream o = new java.io.ByteArrayOutputStream();
        byte[] buf = new byte[16 * 1024];
        for (int n; (n = in.read(buf)) > 0; ) o.write(buf, 0, n);
        return o.toByteArray();
    }
}
