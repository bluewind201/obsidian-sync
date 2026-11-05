package io.bluewind.obsidiansync;
import android.app.Activity;
import android.os.*;
import android.content.Intent;
import android.net.Uri;
import android.provider.Settings;
import android.graphics.Color;
import android.view.View;
import android.widget.*;
import androidx.work.*;
import org.json.JSONObject;
import java.util.concurrent.TimeUnit;

public final class MainActivity extends Activity {
    private LinearLayout body, fields;
    private EditText vault;
    private Spinner backend;
    private CheckBox deletes;
    private TextView status;
    private JSONObject loaded = new JSONObject();
    private final java.util.LinkedHashMap<String, EditText> options = new java.util.LinkedHashMap<>();
    private final String[] backends = {"webdav", "s3", "onedrive", "gdrive", "fs"};
    private final java.util.concurrent.ExecutorService executor = java.util.concurrent.Executors.newSingleThreadExecutor();
    private boolean busy;
    @Override public void onCreate(Bundle state) {
        super.onCreate(state);
        try { String saved = ConfigStore.load(this); if (!saved.isEmpty()) loaded = new JSONObject(saved); } catch (Exception ignored) {}
        ScrollView scroll = new ScrollView(this); body = new LinearLayout(this); body.setOrientation(LinearLayout.VERTICAL); body.setPadding(28, 44, 28, 36); body.setBackgroundColor(Color.rgb(245,247,252)); scroll.addView(body); setContentView(scroll);
        TextView title = label("Obsidian Sync"); title.setTextSize(30); title.setTextColor(Color.rgb(48,65,124));
        label("내 노트를, 내가 선택한 저장소로");
        status = label("연결 설정을 저장한 뒤 동기화하세요.");
        button("보관함 접근 권한 허용", v -> startActivity(new Intent(Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION, Uri.parse("package:" + getPackageName()))));
        label("Obsidian 보관함의 실제 폴더 경로");
        vault = input(body, loaded.optString("vault", "/storage/emulated/0/Documents/Obsidian"), false);
        label("연결 대상: WebDAV / S3 / OneDrive / Google Drive / 폴더");
        backend = new Spinner(this); backend.setAdapter(new ArrayAdapter<>(this, android.R.layout.simple_spinner_dropdown_item, backends)); body.addView(backend);
        fields = new LinearLayout(this); fields.setOrientation(LinearLayout.VERTICAL); body.addView(fields);
        String selected = loaded.optString("backend", "webdav");
        for (int i=0; i<backends.length; i++) if (backends[i].equals(selected)) backend.setSelection(i);
        backend.setOnItemSelectedListener(new android.widget.AdapterView.OnItemSelectedListener() {
            @Override public void onNothingSelected(android.widget.AdapterView<?> p) {}
            @Override public void onItemSelected(android.widget.AdapterView<?> p, View v, int pos, long id) { renderOptions(backends[pos]); }
        });
        deletes = new CheckBox(this); deletes.setText("삭제 전파 허용 (기본값: 삭제한 파일 복원)"); deletes.setChecked(loaded.optBoolean("propagate_deletes", false)); body.addView(deletes);
        button("연결 설정 저장", v -> save());
        button("지금 동기화", v -> {
            if (busy || !save()) return;
            busy = true; status.setText("동기화 중…");
            executor.execute(() -> { String result = ConfigStore.run(this); runOnUiThread(() -> { busy = false; status.setText(result); }); });
        });
        button("실시간 동기화 시작", v -> {
            if (!save()) return;
            if (Build.VERSION.SDK_INT >= 33 && checkSelfPermission(android.Manifest.permission.POST_NOTIFICATIONS) != android.content.pm.PackageManager.PERMISSION_GRANTED) requestPermissions(new String[]{android.Manifest.permission.POST_NOTIFICATIONS}, 1);
            try { startForegroundService(new Intent(this, SyncService.class)); status.setText("실시간 동기화 시작. 15초 간격으로 확인합니다."); }
            catch (RuntimeException e) { status.setText("Android 실행 제한입니다. 잠시 후 다시 시작하세요."); }
        });
        button("백그라운드 주기 동기화 켜기", v -> {
            if (!save()) return;
            Constraints network = new Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build();
            PeriodicWorkRequest request = new PeriodicWorkRequest.Builder(SyncWorker.class, 15, TimeUnit.MINUTES).setConstraints(network).build();
            WorkManager.getInstance(this).enqueueUniquePeriodicWork("vault-sync", ExistingPeriodicWorkPolicy.UPDATE, request);
            status.setText("15분 이상 간격으로 예약했습니다. 배터리 정책에 따라 지연될 수 있습니다.");
        });
        button("모든 자동 동기화 중지", v -> { stopService(new Intent(this, SyncService.class)); WorkManager.getInstance(this).cancelUniqueWork("vault-sync"); status.setText("자동 동기화를 중지했습니다. 진행 중인 작업은 안전하게 완료될 수 있습니다."); });
        button("연결·복구 안내", v -> startActivity(new Intent(Intent.ACTION_VIEW, Uri.parse("https://github.com/bluewind201/obsidian-sync/blob/main/docs/SETUP.md"))));
        label("충돌본: .sync-conflicts\n이전 파일: .sync-backups\n실시간 서비스는 최대 5시간 후 종료됩니다. Android 정책에 따라 더 일찍 종료될 수 있습니다.\nPreview 0.1 · arm64 / x86_64");
    }
    private void renderOptions(String service) {
        fields.removeAllViews(); options.clear();
        String[] keys = switch(service) {
            case "webdav" -> new String[]{"root", "endpoint", "username", "password"};
            case "s3" -> new String[]{"root", "endpoint", "region", "bucket", "access_key_id", "secret_access_key"};
            case "onedrive", "gdrive" -> new String[]{"root", "access_token", "refresh_token", "client_id", "client_secret"};
            default -> new String[]{"root"};
        };
        JSONObject saved = service.equals(loaded.optString("backend")) ? loaded.optJSONObject("options") : null;
        for (String key : keys) {
            TextView label = new TextView(this); label.setText(key); fields.addView(label);
            String fallback = key.equals("root") ? "/ObsidianSync/" : "";
            EditText edit = input(fields, saved == null ? fallback : saved.optString(key, fallback), key.contains("token") || key.contains("secret") || key.equals("password") || key.equals("access_key_id"));
            options.put(key, edit);
        }
    }
    private boolean save() {
        if (!Environment.isExternalStorageManager()) { status.setText("먼저 보관함 접근 권한을 허용하세요."); return false; }
        try {
            JSONObject config = new JSONObject(); config.put("vault", vault.getText().toString().trim()); config.put("backend", backends[backend.getSelectedItemPosition()]);
            JSONObject opts = new JSONObject(); for (var entry : options.entrySet()) { String value = entry.getValue().getText().toString(); if (!value.isEmpty()) opts.put(entry.getKey(), value); }
            config.put("options", opts); config.put("propagate_deletes", deletes.isChecked()); config.put("interval_seconds", 15);
            ConfigStore.save(this, config.toString()); loaded = config; status.setText("Android Keystore로 암호화하여 저장했습니다."); return true;
        } catch (Exception e) { status.setText("설정 저장에 실패했습니다."); return false; }
    }
    private TextView label(String text) { TextView view = new TextView(this); view.setText(text); view.setTextSize(16); view.setPadding(0,12,0,12); body.addView(view); return view; }
    private EditText input(LinearLayout parent, String text, boolean secret) { EditText edit = new EditText(this); edit.setSingleLine(true); edit.setInputType(android.text.InputType.TYPE_CLASS_TEXT | (secret ? android.text.InputType.TYPE_TEXT_VARIATION_PASSWORD : android.text.InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS)); edit.setText(text); parent.addView(edit); return edit; }
    private void button(String text, View.OnClickListener listener) { Button button = new Button(this); button.setText(text); button.setOnClickListener(listener); body.addView(button); }
    @Override protected void onResume() { super.onResume(); if (status != null && !busy) status.setText(getSharedPreferences("status", 0).getString("last", "연결 설정을 저장한 뒤 동기화하세요.")); }
    @Override protected void onDestroy() { executor.shutdown(); super.onDestroy(); }
}
