package io.bluewind.obsidiansync;

import android.app.Activity;
import android.content.ClipData;
import android.content.ClipboardManager;
import android.content.Intent;
import android.content.pm.PackageManager;
import android.graphics.Color;
import android.graphics.Typeface;
import android.graphics.drawable.GradientDrawable;
import android.net.Uri;
import android.os.Build;
import android.os.Bundle;
import android.os.Environment;
import android.os.Handler;
import android.os.Looper;
import android.provider.DocumentsContract;
import android.provider.Settings;
import android.text.InputType;
import android.view.Gravity;
import android.view.View;
import android.view.ViewGroup;
import android.widget.Button;
import android.widget.CheckBox;
import android.widget.EditText;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.TextView;
import androidx.work.Constraints;
import androidx.work.ExistingPeriodicWorkPolicy;
import androidx.work.NetworkType;
import androidx.work.PeriodicWorkRequest;
import androidx.work.WorkManager;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.TimeUnit;
import org.json.JSONArray;
import org.json.JSONObject;

/** 한 화면 설정: ① 볼트 ② 저장소 ③ 연결 확인 ④ 시작. 위쪽 상태 카드에서 동기화 상황을 봅니다. */
public final class MainActivity extends Activity {
    // 색상 (Windows 앱과 동일)
    static final int ACCENT = Color.rgb(124, 58, 237), ACCENT_SOFT = Color.rgb(243, 238, 255);
    static final int OK = Color.rgb(22, 150, 85), WARN = Color.rgb(214, 120, 0), DANGER = Color.rgb(208, 56, 56);
    static final int MUTED = Color.rgb(110, 114, 128), TEXT = Color.rgb(30, 32, 40), BG = Color.rgb(246, 247, 250), BORDER = Color.rgb(225, 227, 234);
    private static final int PICK_VAULT = 10;

    /** 저장소 종류: type, 이름, 설명 */
    private static final String[][] BACKENDS = {
            {"web_dav", "Synology NAS (WebDAV)", "NAS의 WebDAV Server로 연결"},
            {"one_drive", "OneDrive", "Microsoft 계정으로 로그인"},
            {"google_drive", "Google Drive", "Google 계정으로 로그인"},
            {"s3", "네이버 클라우드 / Google Cloud Storage", "S3 호환 스토리지 (액세스 키)"},
            {"local_folder", "휴대폰 안의 다른 폴더", "다른 동기화 앱이 관리하는 폴더 등"},
    };

    private final ExecutorService bg = Executors.newSingleThreadExecutor();
    private final Handler ui = new Handler(Looper.getMainLooper());
    private final Map<String, EditText> fields = new LinkedHashMap<>();
    private final Map<String, LinearLayout> backendCards = new LinkedHashMap<>();
    private JSONObject cfg = new JSONObject();
    private String backendType = "web_dav";

    private LinearLayout root, fieldBox, logBox, loginBox;
    private TextView statusTitle, statusSub, vaultPath, permWarn, testResult;
    private View statusDot;
    private Button toggleBtn, syncBtn;
    private CheckBox chkDeletes, chkObsidian;
    private boolean loggingIn;

    // ─────────────────────────── 화면 구성 ───────────────────────────
    @Override public void onCreate(Bundle state) {
        super.onCreate(state);
        try { String s = ConfigStore.load(this); if (!s.isEmpty()) cfg = new JSONObject(s); } catch (Exception ignored) { }
        JSONObject b = cfg.optJSONObject("backend");
        if (b != null) backendType = b.optString("type", backendType);

        ScrollView scroll = new ScrollView(this);
        scroll.setBackgroundColor(BG);
        root = vbox();
        root.setPadding(dp(16), dp(20), dp(16), dp(32));
        scroll.addView(root);
        setContentView(scroll);

        TextView title = text("Obsidian Sync", 26, TEXT);
        title.setTypeface(Typeface.DEFAULT_BOLD);
        root.addView(title);
        root.addView(text("노트를 내 저장소와 실시간으로 동기화합니다", 14, MUTED));
        space(root, 12);

        // 상태 카드
        LinearLayout status = card(root);
        LinearLayout row = hbox();
        statusDot = new View(this);
        row.addView(statusDot, new LinearLayout.LayoutParams(dp(14), dp(14)));
        statusTitle = text("", 20, TEXT);
        statusTitle.setTypeface(Typeface.DEFAULT_BOLD);
        statusTitle.setPadding(dp(10), 0, 0, 0);
        row.addView(statusTitle);
        status.addView(row);
        statusSub = text("", 14, MUTED);
        status.addView(statusSub);
        LinearLayout btns = hbox();
        toggleBtn = primary("동기화 시작");
        toggleBtn.setOnClickListener(v -> toggle());
        syncBtn = secondary("지금 동기화");
        syncBtn.setOnClickListener(v -> { if (!Native.syncNow()) toast("먼저 동기화를 시작하세요"); });
        btns.addView(toggleBtn, weight());
        btns.addView(syncBtn, weight());
        status.addView(btns);

        // ① 볼트
        LinearLayout c1 = step(root, "1", "Obsidian 볼트 폴더");
        c1.addView(text("Obsidian 앱에서 사용하는 볼트 폴더를 고르세요.", 14, MUTED));
        vaultPath = text(cfg.optString("vault_path", ""), 15, TEXT);
        vaultPath.setPadding(0, dp(6), 0, dp(6));
        c1.addView(vaultPath);
        Button pick = secondary("폴더 선택");
        pick.setOnClickListener(v -> startActivityForResult(new Intent(Intent.ACTION_OPEN_DOCUMENT_TREE), PICK_VAULT));
        c1.addView(pick);
        permWarn = text("⚠ 볼트 파일을 읽고 쓰려면 '모든 파일 접근' 권한이 필요합니다. 눌러서 허용하세요.", 14, WARN);
        permWarn.setPadding(0, dp(8), 0, 0);
        permWarn.setOnClickListener(v -> startActivity(new Intent(Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION, Uri.parse("package:" + getPackageName()))));
        c1.addView(permWarn);

        // ② 저장소
        LinearLayout c2 = step(root, "2", "저장소 선택");
        for (String[] be : BACKENDS) {
            LinearLayout bc = vbox();
            bc.setPadding(dp(12), dp(10), dp(12), dp(10));
            TextView n = text(be[1], 15, TEXT);
            n.setTypeface(Typeface.DEFAULT_BOLD);
            bc.addView(n);
            bc.addView(text(be[2], 13, MUTED));
            bc.setOnClickListener(v -> selectBackend(be[0]));
            LinearLayout.LayoutParams lp = new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT);
            lp.bottomMargin = dp(6);
            c2.addView(bc, lp);
            backendCards.put(be[0], bc);
        }
        fieldBox = vbox();
        c2.addView(fieldBox);

        // ③ 연결 확인
        LinearLayout c3 = step(root, "3", "연결 확인");
        loginBox = vbox();
        c3.addView(loginBox);
        Button test = secondary("연결 테스트");
        test.setOnClickListener(v -> testConnection());
        c3.addView(test);
        testResult = text("", 14, MUTED);
        c3.addView(testResult);

        // ④ 옵션 + 시작
        LinearLayout c4 = step(root, "4", "저장하고 시작");
        chkDeletes = check("삭제도 동기화 (지워진 파일은 .trash 폴더로 이동)", cfg.optBoolean("propagate_deletes", true));
        chkObsidian = check(".obsidian 설정 폴더도 동기화 (플러그인, 테마)", cfg.optBoolean("sync_obsidian_folder", true));
        c4.addView(chkDeletes);
        c4.addView(chkObsidian);
        Button save = primary("저장하고 동기화 시작");
        save.setOnClickListener(v -> { if (save()) startSync(); });
        c4.addView(save);
        Button saveOnly = secondary("저장만 하기");
        saveOnly.setOnClickListener(v -> { if (save()) toast("저장했습니다"); });
        c4.addView(saveOnly);

        // 최근 활동
        LinearLayout c5 = card(root);
        TextView lt = text("최근 활동", 16, TEXT);
        lt.setTypeface(Typeface.DEFAULT_BOLD);
        c5.addView(lt);
        logBox = vbox();
        c5.addView(logBox);

        LinearLayout more = hbox();
        Button help = secondary("도움말");
        help.setOnClickListener(v -> startActivity(new Intent(Intent.ACTION_VIEW, Uri.parse("https://github.com/bluewind201/obsidian-sync/blob/main/docs/SETUP.md"))));
        Button reset = secondary("동기화 기록 초기화");
        reset.setOnClickListener(v -> resetState());
        more.addView(help, weight());
        more.addView(reset, weight());
        root.addView(more);
        root.addView(text("충돌이 나면 두 버전을 모두 남깁니다: 노트 (충돌 기기 날짜).md", 12, MUTED));

        selectBackend(backendType);
    }

    @Override protected void onResume() {
        super.onResume();
        permWarn.setVisibility(Environment.isExternalStorageManager() ? View.GONE : View.VISIBLE);
        refreshLogin();
        tick();
    }

    @Override protected void onPause() {
        ui.removeCallbacksAndMessages(null);
        super.onPause();
    }

    @Override protected void onDestroy() {
        if (loggingIn) Native.loginCancel();
        bg.shutdown();
        super.onDestroy();
    }

    // ─────────────────────────── 저장소 입력칸 ───────────────────────────
    private void selectBackend(String type) {
        // 입력하던 값은 유지
        if (!fields.isEmpty()) try { cfg.put("backend", backendJson()); } catch (Exception ignored) { }
        backendType = type;
        for (Map.Entry<String, LinearLayout> e : backendCards.entrySet()) {
            boolean sel = e.getKey().equals(type);
            e.getValue().setBackground(round(sel ? ACCENT_SOFT : Color.WHITE, sel ? ACCENT : BORDER, sel ? 2 : 1, 10));
        }
        JSONObject saved = cfg.optJSONObject("backend");
        if (saved != null && !type.equals(saved.optString("type"))) saved = null;
        fieldBox.removeAllViews();
        fields.clear();
        switch (type) {
            case "web_dav":
                field("url", "WebDAV 주소", "https://내NAS주소:5006/home/Obsidian", false, saved);
                field("username", "NAS 아이디", "", false, saved);
                field("password", "NAS 비밀번호", "", true, saved);
                break;
            case "s3":
                LinearLayout presets = hbox();
                Button ncp = secondary("네이버 클라우드");
                ncp.setOnClickListener(v -> { fields.get("endpoint").setText("https://kr.object.ncloudstorage.com"); fields.get("region").setText("kr-standard"); });
                Button gcs = secondary("Google Cloud");
                gcs.setOnClickListener(v -> { fields.get("endpoint").setText("https://storage.googleapis.com"); fields.get("region").setText("auto"); });
                presets.addView(ncp, weight());
                presets.addView(gcs, weight());
                fieldBox.addView(presets);
                field("endpoint", "엔드포인트", "https://kr.object.ncloudstorage.com", false, saved, "https://kr.object.ncloudstorage.com");
                field("region", "리전", "kr-standard", false, saved, "kr-standard");
                field("bucket", "버킷 이름", "my-obsidian", false, saved);
                field("prefix", "저장 폴더", "obsidian", false, saved, "obsidian");
                field("access_key", "Access Key", "", false, saved);
                field("secret_key", "Secret Key", "", true, saved);
                break;
            case "one_drive":
                field("client_id", "앱 Client ID", "Azure 앱 등록의 애플리케이션 ID", false, saved);
                field("folder", "OneDrive 폴더", "ObsidianSync", false, saved, "ObsidianSync");
                break;
            case "google_drive":
                field("client_id", "Client ID", "TV 및 입력 제한 기기용 OAuth 클라이언트", false, saved);
                field("client_secret", "Client Secret", "", true, saved);
                field("folder", "Drive 폴더", "ObsidianSync", false, saved, "ObsidianSync");
                break;
            default:
                field("path", "대상 폴더 경로", "/storage/emulated/0/Sync/Obsidian", false, saved);
        }
        testResult.setText("");
        refreshLogin();
    }

    private void field(String key, String label, String hint, boolean secret, JSONObject saved) { field(key, label, hint, secret, saved, ""); }

    private void field(String key, String label, String hint, boolean secret, JSONObject saved, String def) {
        TextView l = text(label, 13, MUTED);
        l.setPadding(0, dp(8), 0, 0);
        fieldBox.addView(l);
        EditText e = new EditText(this);
        e.setSingleLine(true);
        e.setHint(hint);
        e.setTextSize(15);
        e.setInputType(InputType.TYPE_CLASS_TEXT | (secret ? InputType.TYPE_TEXT_VARIATION_PASSWORD : InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS));
        e.setText(saved != null ? saved.optString(key, def) : def);
        fieldBox.addView(e);
        fields.put(key, e);
    }

    private JSONObject backendJson() throws Exception {
        JSONObject b = new JSONObject();
        b.put("type", backendType);
        for (Map.Entry<String, EditText> e : fields.entrySet()) b.put(e.getKey(), e.getValue().getText().toString().trim());
        if (backendType.equals("one_drive")) b.put("tenant", "common");
        return b;
    }

    private String configJson() {
        try {
            JSONObject c = new JSONObject(cfg.toString());
            c.put("vault_path", vaultPath.getText().toString().trim());
            c.put("backend", backendJson());
            c.put("propagate_deletes", chkDeletes.isChecked());
            c.put("sync_obsidian_folder", chkObsidian.isChecked());
            if (!c.has("poll_interval_secs")) c.put("poll_interval_secs", 60);
            if (!c.has("device_name") || c.optString("device_name").isEmpty()) c.put("device_name", Build.MODEL.replace(' ', '-'));
            return c.toString();
        } catch (Exception e) {
            return "{}";
        }
    }

    // ─────────────────────────── 동작 ───────────────────────────
    private boolean save() {
        if (!Environment.isExternalStorageManager()) {
            toast("먼저 '모든 파일 접근' 권한을 허용해 주세요");
            permWarn.performClick();
            return false;
        }
        String json = configJson();
        try {
            JSONObject v = new JSONObject(Native.validate(json));
            if (!v.optBoolean("ok")) { toast(v.optString("error")); return false; }
            ConfigStore.save(this, json);
            cfg = new JSONObject(json);
            return true;
        } catch (Exception e) {
            toast("저장하지 못했습니다");
            return false;
        }
    }

    private void startSync() {
        String json = configJson();
        if (!Native.isLoggedIn(json, ConfigStore.dataDir(this))) { toast("먼저 ③에서 로그인해 주세요"); return; }
        if (Build.VERSION.SDK_INT >= 33 && checkSelfPermission(android.Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED)
            requestPermissions(new String[]{android.Manifest.permission.POST_NOTIFICATIONS}, 1);
        try {
            SyncService.start(this);
        } catch (RuntimeException e) {
            toast("Android가 백그라운드 시작을 막았습니다. 잠시 후 다시 눌러 주세요");
            return;
        }
        // 서비스가 시스템에 의해 꺼졌을 때를 대비한 예비 주기 동기화
        Constraints net = new Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build();
        WorkManager.getInstance(this).enqueueUniquePeriodicWork("obsidian-sync", ExistingPeriodicWorkPolicy.UPDATE,
                new PeriodicWorkRequest.Builder(SyncWorker.class, 15, TimeUnit.MINUTES).setConstraints(net).build());
        toast("실시간 동기화를 시작했습니다");
        ui.postDelayed(this::tick, 800);
    }

    private void toggle() {
        if (ConfigStore.enabled(this)) {
            SyncService.stop(this);
            WorkManager.getInstance(this).cancelUniqueWork("obsidian-sync");
            ui.postDelayed(this::tick, 500);
        } else if (save()) {
            startSync();
        }
    }

    private void testConnection() {
        String json = configJson();
        testResult.setTextColor(MUTED);
        testResult.setText("확인하는 중…");
        bg.execute(() -> {
            String r = Native.testConnection(json, ConfigStore.dataDir(this));
            ui.post(() -> {
                try {
                    JSONObject o = new JSONObject(r);
                    if (o.optBoolean("ok")) {
                        int n = o.optInt("count");
                        testResult.setTextColor(OK);
                        testResult.setText(n == 0 ? "✔ 연결 성공! 원격이 비어 있어 첫 동기화 때 노트를 올립니다." : "✔ 연결 성공! 원격에 파일 " + n + "개가 있습니다.");
                    } else {
                        testResult.setTextColor(DANGER);
                        testResult.setText("✖ " + friendly(o.optString("error")));
                    }
                } catch (Exception ignored) { }
            });
        });
    }

    private void refreshLogin() {
        if (loginBox == null) return;
        loginBox.removeAllViews();
        if (!backendType.equals("one_drive") && !backendType.equals("google_drive")) return;
        String json = configJson();
        boolean in = Native.isLoggedIn(json, ConfigStore.dataDir(this));
        if (loggingIn) return;
        LinearLayout row = hbox();
        TextView st = text(in ? "● 계정 연결됨" : "● 로그인 필요", 14, in ? OK : WARN);
        row.addView(st, weight());
        Button b = secondary(in ? "다시 로그인" : "로그인");
        b.setOnClickListener(v -> login());
        row.addView(b);
        if (in) {
            Button out = secondary("로그아웃");
            out.setOnClickListener(v -> { Native.logout(json, ConfigStore.dataDir(this)); refreshLogin(); });
            row.addView(out);
        }
        loginBox.addView(row);
    }

    private void login() {
        String json = configJson();
        loggingIn = true;
        loginBox.removeAllViews();
        loginBox.addView(text("로그인 준비 중…", 14, MUTED));
        bg.execute(() -> {
            try {
                JSONObject s = new JSONObject(Native.loginStart(json));
                if (!s.optBoolean("ok")) throw new Exception(s.optString("error"));
                String code = s.optString("user_code"), uri = s.optString("verification_uri");
                ui.post(() -> showCode(code, uri));
                JSONObject w = new JSONObject(Native.loginWait(json, ConfigStore.dataDir(this)));
                if (!w.optBoolean("ok")) throw new Exception(w.optString("error"));
                ui.post(() -> { loggingIn = false; refreshLogin(); toast("로그인했습니다. 연결 테스트로 확인해 보세요"); });
            } catch (Exception e) {
                ui.post(() -> { loggingIn = false; refreshLogin(); toast(friendly(String.valueOf(e.getMessage()))); });
            }
        });
    }

    private void showCode(String code, String uri) {
        loginBox.removeAllViews();
        LinearLayout box = vbox();
        box.setPadding(dp(12), dp(10), dp(12), dp(10));
        box.setBackground(round(ACCENT_SOFT, ACCENT_SOFT, 0, 10));
        box.addView(text("아래 코드를 복사한 뒤, 열리는 브라우저 화면에 붙여넣고 계정을 승인하세요.", 14, TEXT));
        TextView c = text(code, 28, ACCENT);
        c.setTypeface(Typeface.MONOSPACE, Typeface.BOLD);
        c.setGravity(Gravity.CENTER);
        box.addView(c);
        LinearLayout r = hbox();
        Button copy = primary("코드 복사 후 브라우저 열기");
        copy.setOnClickListener(v -> {
            getSystemService(ClipboardManager.class).setPrimaryClip(ClipData.newPlainText("code", code));
            startActivity(new Intent(Intent.ACTION_VIEW, Uri.parse(uri)));
        });
        Button cancel = secondary("취소");
        cancel.setOnClickListener(v -> Native.loginCancel());
        r.addView(copy, weight());
        r.addView(cancel);
        box.addView(r);
        box.addView(text("승인을 기다리는 중… 승인 후 이 앱으로 돌아오세요.", 13, MUTED));
        loginBox.addView(box);
    }

    private void resetState() {
        String json = configJson();
        bg.execute(() -> {
            String r = Native.resetState(json, ConfigStore.dataDir(this));
            ui.post(() -> toast(r.contains("\"ok\":true") ? "동기화 기록을 초기화했습니다" : "초기화하지 못했습니다"));
        });
    }

    // ─────────────────────────── 상태 갱신 ───────────────────────────
    private void tick() {
        ui.removeCallbacksAndMessages(null);
        try {
            JSONObject s = new JSONObject(Native.status());
            boolean enabled = ConfigStore.enabled(this);
            String startErr = getSharedPreferences("state", 0).getString("start_error", null);
            String err = s.isNull("last_error") ? null : s.optString("last_error", null);
            if (s.optBoolean("syncing")) setStatus(ACCENT, "동기화 중…", "변경된 파일을 주고받고 있습니다");
            else if (enabled && startErr != null) setStatus(DANGER, "시작하지 못했습니다", friendly(startErr));
            else if (s.optBoolean("running") && err != null && !err.isEmpty()) setStatus(DANGER, "확인이 필요합니다", friendly(err));
            else if (s.optBoolean("running")) setStatus(OK, "실시간 동기화 켜짐",
                    s.isNull("last_sync") ? "첫 동기화를 준비하고 있습니다" : "마지막 동기화 " + s.optString("last_sync") + " · " + s.optString("last_summary"));
            else if (enabled) setStatus(WARN, "다시 시작하는 중…", "잠시만 기다려 주세요");
            else if (cfg.optString("vault_path").isEmpty()) setStatus(WARN, "설정이 필요합니다", "아래 1~4단계를 차례로 진행하세요");
            else setStatus(MUTED, "동기화 꺼짐", "시작을 누르면 실시간 동기화가 켜집니다");
            toggleBtn.setText(enabled ? "일시 중지" : "동기화 시작");
            syncBtn.setEnabled(s.optBoolean("running") && !s.optBoolean("syncing"));

            logBox.removeAllViews();
            JSONArray logs = s.optJSONArray("logs");
            if (logs == null || logs.length() == 0) logBox.addView(text("아직 기록이 없습니다", 13, MUTED));
            else for (int i = logs.length() - 1; i >= Math.max(0, logs.length() - 10); i--) {
                JSONObject l = logs.getJSONObject(i);
                logBox.addView(text(l.optString("time").substring(6) + "  " + l.optString("text"), 13, l.optBoolean("error") ? DANGER : TEXT));
            }
        } catch (Exception ignored) { }
        ui.postDelayed(this::tick, 1500);
    }

    private void setStatus(int color, String title, String sub) {
        statusDot.setBackground(round(color, color, 0, 7));
        statusTitle.setText(title);
        statusSub.setText(sub);
    }

    @Override protected void onActivityResult(int req, int res, Intent data) {
        super.onActivityResult(req, res, data);
        if (req == PICK_VAULT && res == RESULT_OK && data != null && data.getData() != null) {
            String path = treeToPath(data.getData());
            if (path == null) toast("이 위치는 선택할 수 없습니다. 내부 저장소의 폴더를 골라 주세요");
            else vaultPath.setText(path);
        }
    }

    /** SAF 폴더 URI → 실제 경로 (예: primary:Documents/Vault → /storage/emulated/0/Documents/Vault) */
    static String treeToPath(Uri uri) {
        try {
            String id = DocumentsContract.getTreeDocumentId(uri);
            int i = id.indexOf(':');
            if (i < 0) return null;
            String vol = id.substring(0, i), rel = id.substring(i + 1);
            String base = vol.equalsIgnoreCase("primary") ? Environment.getExternalStorageDirectory().getAbsolutePath() : "/storage/" + vol;
            return rel.isEmpty() ? base : base + "/" + rel;
        } catch (Exception e) {
            return null;
        }
    }

    static String friendly(String raw) {
        if (raw == null) return "";
        String hint = "";
        if (raw.contains("HTTP 401") || raw.contains("HTTP 403") || raw.contains("SignatureDoesNotMatch") || raw.contains("InvalidAccessKeyId"))
            hint = "아이디/비밀번호 또는 키가 맞지 않거나 권한이 없습니다. ";
        else if (raw.contains("HTTP 404") || raw.contains("NoSuchBucket")) hint = "주소나 버킷/폴더 이름을 찾을 수 없습니다. ";
        else if (raw.contains("네트워크 오류")) hint = "서버에 연결할 수 없습니다. 주소와 인터넷 연결을 확인하세요. ";
        else if (raw.contains("다시 로그인") || raw.contains("invalid_grant")) hint = "로그인이 만료되었습니다. 다시 로그인해 주세요. ";
        String first = raw.split("\n")[0];
        if (first.length() > 200) first = first.substring(0, 200) + "…";
        return hint.isEmpty() ? first : hint + "(" + first + ")";
    }

    // ─────────────────────────── 위젯 도우미 ───────────────────────────
    private int dp(int v) { return Math.round(v * getResources().getDisplayMetrics().density); }

    private GradientDrawable round(int fill, int stroke, int strokeDp, int radiusDp) {
        GradientDrawable d = new GradientDrawable();
        d.setColor(fill);
        d.setCornerRadius(dp(radiusDp));
        if (strokeDp > 0) d.setStroke(dp(strokeDp), stroke);
        return d;
    }

    private LinearLayout vbox() { LinearLayout l = new LinearLayout(this); l.setOrientation(LinearLayout.VERTICAL); return l; }

    private LinearLayout hbox() {
        LinearLayout l = new LinearLayout(this);
        l.setOrientation(LinearLayout.HORIZONTAL);
        l.setGravity(Gravity.CENTER_VERTICAL);
        return l;
    }

    private LinearLayout.LayoutParams weight() {
        LinearLayout.LayoutParams p = new LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1);
        p.setMarginEnd(dp(6));
        return p;
    }

    private TextView text(String s, int sp, int color) {
        TextView t = new TextView(this);
        t.setText(s);
        t.setTextSize(sp);
        t.setTextColor(color);
        t.setLineSpacing(0, 1.15f);
        return t;
    }

    private void space(LinearLayout parent, int dpv) { parent.addView(new View(this), new LinearLayout.LayoutParams(1, dp(dpv))); }

    private LinearLayout card(LinearLayout parent) {
        LinearLayout c = vbox();
        c.setPadding(dp(16), dp(14), dp(16), dp(14));
        c.setBackground(round(Color.WHITE, BORDER, 1, 12));
        LinearLayout.LayoutParams lp = new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT);
        lp.bottomMargin = dp(10);
        parent.addView(c, lp);
        return c;
    }

    private LinearLayout step(LinearLayout parent, String n, String title) {
        LinearLayout c = card(parent);
        LinearLayout row = hbox();
        TextView num = text(n, 13, Color.WHITE);
        num.setGravity(Gravity.CENTER);
        num.setBackground(round(ACCENT, ACCENT, 0, 12));
        row.addView(num, new LinearLayout.LayoutParams(dp(24), dp(24)));
        TextView t = text(title, 17, TEXT);
        t.setTypeface(Typeface.DEFAULT_BOLD);
        t.setPadding(dp(10), 0, 0, 0);
        row.addView(t);
        c.addView(row);
        space(c, 6);
        return c;
    }

    private Button primary(String s) {
        Button b = new Button(this);
        b.setText(s);
        b.setAllCaps(false);
        b.setTextColor(Color.WHITE);
        b.setBackground(round(ACCENT, ACCENT, 0, 8));
        return b;
    }

    private Button secondary(String s) {
        Button b = new Button(this);
        b.setText(s);
        b.setAllCaps(false);
        b.setTextColor(TEXT);
        b.setBackground(round(Color.rgb(240, 241, 246), BORDER, 1, 8));
        return b;
    }

    private CheckBox check(String s, boolean on) {
        CheckBox c = new CheckBox(this);
        c.setText(s);
        c.setTextColor(TEXT);
        c.setChecked(on);
        return c;
    }

    private void toast(String s) { android.widget.Toast.makeText(this, s, android.widget.Toast.LENGTH_LONG).show(); }
}
