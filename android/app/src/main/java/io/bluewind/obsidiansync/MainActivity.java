package io.bluewind.obsidiansync;

import android.app.Activity;
import android.app.AlertDialog;
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
import android.widget.RadioButton;
import android.widget.RadioGroup;
import android.widget.ScrollView;
import android.widget.TextView;
import androidx.work.Constraints;
import androidx.work.ExistingPeriodicWorkPolicy;
import androidx.work.NetworkType;
import androidx.work.PeriodicWorkRequest;
import androidx.work.WorkManager;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.TimeUnit;
import org.json.JSONArray;
import org.json.JSONObject;

/**
 * 메인 화면: 위쪽 상태 카드 + 동기화 프로필 목록.
 * 프로필 하나 = 휴대폰 폴더 1개 ↔ 저장소 폴더 1개. 프로필을 누르면 편집 화면으로 바뀝니다.
 */
public final class MainActivity extends Activity {
    // 색상 (Windows 앱과 동일)
    static final int ACCENT = Color.rgb(124, 58, 237), ACCENT_SOFT = Color.rgb(243, 238, 255);
    static final int OK = Color.rgb(22, 150, 85), WARN = Color.rgb(214, 120, 0), DANGER = Color.rgb(208, 56, 56);
    static final int MUTED = Color.rgb(110, 114, 128), TEXT = Color.rgb(30, 32, 40), BG = Color.rgb(246, 247, 250), BORDER = Color.rgb(225, 227, 234);
    private static final int PICK_LOCAL = 10;

    /** 저장소 종류: type, 이름, 설명 */
    private static final String[][] BACKENDS = {
            {"synology", "Synology NAS", "DSM 계정으로 로그인 (File Station)"},
            {"one_drive", "OneDrive", "Microsoft 계정으로 로그인"},
            {"google_drive", "Google Drive", "Google 계정으로 로그인"},
            {"s3", "네이버 클라우드 / Google Cloud Storage", "S3 호환 스토리지 (액세스 키)"},
            {"local_folder", "휴대폰 안의 다른 폴더", "다른 동기화 앱이 관리하는 폴더 등"},
    };

    private final ExecutorService bg = Executors.newSingleThreadExecutor();
    private final Handler ui = new Handler(Looper.getMainLooper());
    private JSONObject cfg = new JSONObject();

    private LinearLayout root;
    // 목록 화면
    private TextView statusTitle, statusSub, permWarn;
    private View statusDot;
    private Button toggleBtn, syncBtn;
    private LinearLayout logBox, profileBox;
    private final List<TextView> profileLines = new ArrayList<>();
    // 편집 화면
    private boolean editing;
    private int editIndex = -1;
    private JSONObject edit;
    private String backendType = "synology";
    private final Map<String, EditText> fields = new LinkedHashMap<>();
    private final Map<String, LinearLayout> backendCards = new LinkedHashMap<>();
    private LinearLayout fieldBox, loginBox, otpBox;
    private TextView localPath, testResult;
    private EditText nameEdit, ignoreEdit, otpEdit;
    private RadioButton kindObsidian, modeTwoWay;
    private CheckBox chkEnabled, chkDeletes, chkObsidian;
    private boolean loggingIn;

    // ─────────────────────────── 시작 ───────────────────────────
    @Override public void onCreate(Bundle state) {
        super.onCreate(state);
        loadConfig();
        ScrollView scroll = new ScrollView(this);
        scroll.setBackgroundColor(BG);
        root = vbox();
        root.setPadding(dp(16), dp(20), dp(16), dp(32));
        scroll.addView(root);
        setContentView(scroll);
        showList();
    }

    private void loadConfig() {
        try {
            JSONObject r = new JSONObject(Native.normalize(ConfigStore.load(this)));
            if (r.optBoolean("ok")) cfg = r.getJSONObject("config");
        } catch (Exception ignored) { }
    }

    private JSONArray profiles() {
        JSONArray a = cfg.optJSONArray("profiles");
        if (a == null) {
            a = new JSONArray();
            try { cfg.put("profiles", a); } catch (Exception ignored) { }
        }
        return a;
    }

    @Override protected void onResume() {
        super.onResume();
        if (permWarn != null) permWarn.setVisibility(Environment.isExternalStorageManager() ? View.GONE : View.VISIBLE);
        if (editing) refreshLogin(); else tick();
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

    @Override public void onBackPressed() {
        if (editing) showList(); else super.onBackPressed();
    }

    // ─────────────────────────── 목록 화면 ───────────────────────────
    private void showList() {
        editing = false;
        if (loggingIn) { Native.loginCancel(); loggingIn = false; }
        root.removeAllViews();
        profileLines.clear();

        TextView title = text("Obsidian Sync", 26, TEXT);
        title.setTypeface(Typeface.DEFAULT_BOLD);
        root.addView(title);
        root.addView(text("노트와 사진을 내 저장소와 실시간으로 동기화합니다", 14, MUTED));
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
        permWarn = text("⚠ 폴더의 파일을 읽고 쓰려면 '모든 파일 접근' 권한이 필요합니다. 눌러서 허용하세요.", 14, WARN);
        permWarn.setPadding(0, dp(8), 0, 0);
        permWarn.setOnClickListener(v -> startActivity(new Intent(Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION, Uri.parse("package:" + getPackageName()))));
        permWarn.setVisibility(Environment.isExternalStorageManager() ? View.GONE : View.VISIBLE);
        status.addView(permWarn);

        // 프로필 목록
        LinearLayout pc = card(root);
        TextView pt = text("동기화 폴더", 16, TEXT);
        pt.setTypeface(Typeface.DEFAULT_BOLD);
        pc.addView(pt);
        pc.addView(text("폴더마다 저장소 폴더를 1:1로 연결합니다. 눌러서 편집하세요.", 13, MUTED));
        space(pc, 6);
        profileBox = vbox();
        pc.addView(profileBox);
        JSONArray ps = profiles();
        if (ps.length() == 0) profileBox.addView(text("아직 없습니다. 아래 버튼으로 추가하세요.", 14, MUTED));
        for (int i = 0; i < ps.length(); i++) profileBox.addView(profileRow(i, ps.optJSONObject(i)));
        LinearLayout add = hbox();
        Button addVault = secondary("+ Obsidian 볼트");
        addVault.setOnClickListener(v -> newProfile("obsidian"));
        Button addFolder = secondary("+ 사진/일반 폴더");
        addFolder.setOnClickListener(v -> newProfile("folder"));
        add.addView(addVault, weight());
        add.addView(addFolder, weight());
        pc.addView(add);

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
        more.addView(help, weight());
        root.addView(more);
        root.addView(text("충돌이 나면 두 버전을 모두 남깁니다: 노트 (충돌 기기 날짜).md", 12, MUTED));
        tick();
    }

    private View profileRow(int index, JSONObject p) {
        LinearLayout r = vbox();
        r.setPadding(dp(12), dp(10), dp(12), dp(10));
        r.setBackground(round(ACCENT_SOFT, ACCENT_SOFT, 0, 10));
        LinearLayout top = hbox();
        TextView n = text(displayName(p), 16, p.optBoolean("enabled", true) ? TEXT : MUTED);
        n.setTypeface(Typeface.DEFAULT_BOLD);
        top.addView(n, weight());
        CheckBox on = new CheckBox(this);
        on.setText("켜기");
        on.setChecked(p.optBoolean("enabled", true));
        on.setOnCheckedChangeListener((b, checked) -> {
            try {
                p.put("enabled", checked);
                persist(false);
                if (ConfigStore.enabled(this)) SyncService.start(this);
            } catch (Exception ignored) { }
            showList();
        });
        top.addView(on);
        r.addView(top);
        String kind = "folder".equals(p.optString("kind")) ? "폴더" : "Obsidian";
        String mode = "upload_only".equals(p.optString("mode")) ? "백업 (올리기만)" : "양방향";
        r.addView(text(kind + " · " + mode, 12, MUTED));
        r.addView(text(p.optString("local_path") + "  ⇄  " + backendSummary(p.optJSONObject("backend")), 13, TEXT));
        TextView line = text("", 13, MUTED);
        line.setTag(p.optString("id"));
        r.addView(line);
        profileLines.add(line);
        r.setOnClickListener(v -> showEditor(index));
        LinearLayout.LayoutParams lp = new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT);
        lp.bottomMargin = dp(8);
        r.setLayoutParams(lp);
        return r;
    }

    static String displayName(JSONObject p) {
        String n = p.optString("name", "").trim();
        if (!n.isEmpty()) return n;
        String path = p.optString("local_path", "");
        int i = path.lastIndexOf('/');
        String last = i >= 0 ? path.substring(i + 1) : path;
        return last.isEmpty() ? "이름 없음" : last;
    }

    static String backendSummary(JSONObject b) {
        if (b == null) return "-";
        switch (b.optString("type")) {
            case "synology":
                String host = b.optString("url").replaceFirst("^https?://", "");
                return "Synology · " + host + " " + b.optString("folder");
            case "s3": return "S3 · " + b.optString("bucket") + "/" + b.optString("prefix");
            case "one_drive": return "OneDrive · /" + b.optString("folder");
            case "google_drive": return "Google Drive · /" + b.optString("folder");
            default: return "폴더 · " + b.optString("path");
        }
    }

    private void newProfile(String kind) {
        try {
            JSONObject p = new JSONObject();
            p.put("id", Native.newProfileId());
            p.put("name", kind.equals("obsidian") ? "Obsidian" : "사진");
            p.put("kind", kind);
            p.put("enabled", true);
            p.put("mode", kind.equals("obsidian") ? "two_way" : "upload_only");
            p.put("propagate_deletes", kind.equals("obsidian"));
            p.put("sync_obsidian_folder", true);
            if (kind.equals("folder")) p.put("local_path", Environment.getExternalStorageDirectory().getAbsolutePath() + "/DCIM/Camera");
            // 같은 NAS를 쓰는 경우가 많으므로 기존 Synology 계정을 복사 (폴더만 비움)
            JSONObject b = new JSONObject().put("type", "synology");
            JSONArray ps = profiles();
            for (int i = ps.length() - 1; i >= 0; i--) {
                JSONObject ob = ps.getJSONObject(i).optJSONObject("backend");
                if (ob != null && "synology".equals(ob.optString("type"))) {
                    b = new JSONObject(ob.toString()).put("folder", "");
                    break;
                }
            }
            p.put("backend", b);
            edit = p;
            editIndex = -1;
            buildEditor();
        } catch (Exception e) {
            toast("프로필을 만들 수 없습니다");
        }
    }

    // ─────────────────────────── 편집 화면 ───────────────────────────
    private void showEditor(int index) {
        try {
            edit = new JSONObject(profiles().getJSONObject(index).toString());
            editIndex = index;
            buildEditor();
        } catch (Exception e) {
            toast("프로필을 열 수 없습니다");
        }
    }

    private void buildEditor() {
        editing = true;
        ui.removeCallbacksAndMessages(null);
        root.removeAllViews();
        fields.clear();
        backendCards.clear();
        JSONObject b = edit.optJSONObject("backend");
        backendType = b != null ? b.optString("type", "synology") : "synology";

        Button back = secondary("← 목록으로 (저장 안 함)");
        back.setOnClickListener(v -> showList());
        root.addView(back);
        TextView title = text(editIndex < 0 ? "새 프로필" : "프로필 편집", 24, TEXT);
        title.setTypeface(Typeface.DEFAULT_BOLD);
        title.setPadding(0, dp(8), 0, dp(8));
        root.addView(title);

        // ① 이름과 종류
        LinearLayout c1 = step(root, "1", "이름과 종류");
        c1.addView(label("프로필 이름"));
        nameEdit = input(edit.optString("name"), "예: Obsidian, 사진", false);
        c1.addView(nameEdit);
        RadioGroup kinds = new RadioGroup(this);
        kindObsidian = radio("Obsidian 볼트");
        RadioButton kindFolder = radio("일반 폴더 (사진·문서 등)");
        kinds.addView(kindObsidian);
        kinds.addView(kindFolder);
        boolean isObsidian = !"folder".equals(edit.optString("kind"));
        (isObsidian ? kindObsidian : kindFolder).setChecked(true);
        kinds.setOnCheckedChangeListener((g, id) -> chkObsidian.setVisibility(kindObsidian.isChecked() ? View.VISIBLE : View.GONE));
        c1.addView(kinds);
        chkEnabled = check("이 프로필 동기화하기", edit.optBoolean("enabled", true));
        c1.addView(chkEnabled);

        // ② 휴대폰 폴더
        LinearLayout c2 = step(root, "2", "휴대폰 폴더");
        c2.addView(text("동기화할 폴더를 고르세요. 사진은 보통 DCIM/Camera 입니다.", 14, MUTED));
        localPath = text(edit.optString("local_path", ""), 15, TEXT);
        localPath.setPadding(0, dp(6), 0, dp(6));
        c2.addView(localPath);
        Button pick = secondary("폴더 선택");
        pick.setOnClickListener(v -> startActivityForResult(new Intent(Intent.ACTION_OPEN_DOCUMENT_TREE), PICK_LOCAL));
        c2.addView(pick);

        // ③ 저장소
        LinearLayout c3 = step(root, "3", "저장소");
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
            c3.addView(bc, lp);
            backendCards.put(be[0], bc);
        }
        fieldBox = vbox();
        c3.addView(fieldBox);

        // ④ 연결 확인
        LinearLayout c4 = step(root, "4", "연결 확인");
        loginBox = vbox();
        c4.addView(loginBox);
        Button test = secondary("연결 테스트");
        test.setOnClickListener(v -> testConnection());
        c4.addView(test);
        testResult = text("", 14, MUTED);
        c4.addView(testResult);
        otpBox = vbox();
        otpBox.setPadding(dp(12), dp(10), dp(12), dp(10));
        otpBox.setBackground(round(ACCENT_SOFT, ACCENT_SOFT, 0, 10));
        otpBox.addView(text("🔐 2단계 인증 코드(OTP) 6자리를 입력하세요. 한 번 인증하면 이 휴대폰을 기억합니다.", 14, TEXT));
        LinearLayout otpRow = hbox();
        otpEdit = input("", "123456", false);
        otpEdit.setInputType(InputType.TYPE_CLASS_NUMBER);
        otpRow.addView(otpEdit, weight());
        Button otpBtn = primary("인증");
        otpBtn.setOnClickListener(v -> submitOtp());
        otpRow.addView(otpBtn);
        otpBox.addView(otpRow);
        otpBox.setVisibility(View.GONE);
        c4.addView(otpBox);

        // ⑤ 동기화 방식
        LinearLayout c5 = step(root, "5", "동기화 방식");
        RadioGroup modes = new RadioGroup(this);
        modeTwoWay = radio("양방향 — 어느 쪽에서 바꿔도 서로 맞춥니다");
        RadioButton modeUp = radio("백업 (올리기만) — 휴대폰 → 저장소로만 올립니다. 휴대폰에서 지워도 저장소에는 남습니다");
        modes.addView(modeTwoWay);
        modes.addView(modeUp);
        boolean two = !"upload_only".equals(edit.optString("mode"));
        (two ? modeTwoWay : modeUp).setChecked(true);
        modes.setOnCheckedChangeListener((g, id) -> chkDeletes.setVisibility(modeTwoWay.isChecked() ? View.VISIBLE : View.GONE));
        c5.addView(modes);
        chkDeletes = check("삭제도 동기화 (지워진 파일은 폴더 안 .trash 로 이동)", edit.optBoolean("propagate_deletes", true));
        chkDeletes.setVisibility(two ? View.VISIBLE : View.GONE);
        c5.addView(chkDeletes);
        chkObsidian = check(".obsidian 설정 폴더도 동기화 (플러그인, 테마)", edit.optBoolean("sync_obsidian_folder", true));
        chkObsidian.setVisibility(isObsidian ? View.VISIBLE : View.GONE);
        c5.addView(chkObsidian);
        c5.addView(label("동기화하지 않을 파일/폴더 (한 줄에 하나, 예: *.mp4)"));
        StringBuilder ig = new StringBuilder();
        JSONArray igs = edit.optJSONArray("ignore_patterns");
        if (igs != null) for (int i = 0; i < igs.length(); i++) ig.append(i > 0 ? "\n" : "").append(igs.optString(i));
        ignoreEdit = input(ig.toString(), "", false);
        ignoreEdit.setSingleLine(false);
        ignoreEdit.setMinLines(2);
        c5.addView(ignoreEdit);

        // 저장 / 삭제
        Button save = primary("저장");
        save.setOnClickListener(v -> saveProfile());
        root.addView(save);
        if (editIndex >= 0) {
            LinearLayout more = hbox();
            Button reset = secondary("동기화 기록 초기화");
            reset.setOnClickListener(v -> resetState());
            Button del = secondary("프로필 삭제");
            del.setTextColor(DANGER);
            del.setOnClickListener(v -> confirmDelete());
            more.addView(reset, weight());
            more.addView(del, weight());
            space(root, 8);
            root.addView(more);
        }

        selectBackend(backendType);
    }

    private void selectBackend(String type) {
        // 입력하던 값은 유지
        if (!fields.isEmpty()) try { edit.put("backend", backendJson()); } catch (Exception ignored) { }
        backendType = type;
        for (Map.Entry<String, LinearLayout> e : backendCards.entrySet()) {
            boolean sel = e.getKey().equals(type);
            e.getValue().setBackground(round(sel ? ACCENT_SOFT : Color.WHITE, sel ? ACCENT : BORDER, sel ? 2 : 1, 10));
        }
        JSONObject saved = edit.optJSONObject("backend");
        if (saved != null && !type.equals(saved.optString("type"))) saved = null;
        fieldBox.removeAllViews();
        fields.clear();
        boolean folderKind = "folder".equals(edit.optString("kind"));
        switch (type) {
            case "synology":
                field("url", "NAS 주소", "https://내NAS주소:5001", false, saved);
                field("username", "아이디", "DSM 로그인 아이디", false, saved);
                field("password", "비밀번호", "", true, saved);
                field("folder", "NAS 폴더", folderKind ? "/photo/휴대폰" : "/home/Obsidian", false, saved);
                fieldBox.addView(text("공유 폴더 이름부터 적습니다. 없는 하위 폴더는 자동으로 만듭니다.", 12, MUTED));
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
        otpBox.setVisibility(View.GONE);
        refreshLogin();
    }

    private void field(String key, String label, String hint, boolean secret, JSONObject saved) { field(key, label, hint, secret, saved, ""); }

    private void field(String key, String label, String hint, boolean secret, JSONObject saved, String def) {
        fieldBox.addView(label(label));
        EditText e = input(saved != null ? saved.optString(key, def) : def, hint, secret);
        fieldBox.addView(e);
        fields.put(key, e);
    }

    private JSONObject backendJson() throws Exception {
        JSONObject b = new JSONObject();
        b.put("type", backendType);
        for (Map.Entry<String, EditText> e : fields.entrySet()) {
            String v = e.getValue().getText().toString();
            b.put(e.getKey(), e.getKey().equals("password") ? v : v.trim());
        }
        if (backendType.equals("one_drive")) b.put("tenant", "common");
        return b;
    }

    private String backendStr() {
        try { return backendJson().toString(); } catch (Exception e) { return "{}"; }
    }

    /** 편집 중인 내용을 프로필 JSON으로 */
    private JSONObject profileJson() throws Exception {
        JSONObject p = new JSONObject(edit.toString());
        p.put("name", nameEdit.getText().toString().trim());
        p.put("kind", kindObsidian.isChecked() ? "obsidian" : "folder");
        p.put("enabled", chkEnabled.isChecked());
        p.put("local_path", localPath.getText().toString().trim());
        p.put("backend", backendJson());
        p.put("mode", modeTwoWay.isChecked() ? "two_way" : "upload_only");
        p.put("propagate_deletes", chkDeletes.isChecked());
        p.put("sync_obsidian_folder", chkObsidian.isChecked());
        JSONArray ig = new JSONArray();
        for (String l : ignoreEdit.getText().toString().split("\n")) if (!l.trim().isEmpty()) ig.put(l.trim());
        p.put("ignore_patterns", ig);
        return p;
    }

    private void saveProfile() {
        if (!Environment.isExternalStorageManager()) {
            toast("먼저 목록 화면에서 '모든 파일 접근' 권한을 허용해 주세요");
            return;
        }
        try {
            JSONObject p = profileJson();
            JSONObject next = new JSONObject(cfg.toString());
            JSONArray ps = next.optJSONArray("profiles");
            if (ps == null) { ps = new JSONArray(); next.put("profiles", ps); }
            if (editIndex >= 0) ps.put(editIndex, p); else ps.put(p);
            JSONObject v = new JSONObject(Native.validate(withDevice(next).toString()));
            if (!v.optBoolean("ok")) { toast(friendly(v.optString("error"))); return; }
            cfg = next;
            persist(true);
            if (ConfigStore.enabled(this)) SyncService.start(this); // 바뀐 설정으로 다시 시작
            toast("저장했습니다");
            showList();
        } catch (Exception e) {
            toast("저장하지 못했습니다");
        }
    }

    private JSONObject withDevice(JSONObject c) throws Exception {
        if (!c.has("poll_interval_secs")) c.put("poll_interval_secs", 60);
        if (c.optString("device_name").isEmpty()) c.put("device_name", Build.MODEL.replace(' ', '-'));
        return c;
    }

    private void persist(boolean strict) throws Exception {
        try {
            ConfigStore.save(this, withDevice(cfg).toString());
        } catch (Exception e) {
            if (strict) throw e;
        }
    }

    private void confirmDelete() {
        new AlertDialog.Builder(this)
                .setTitle("프로필 삭제")
                .setMessage("'" + displayName(edit) + "' 프로필을 지울까요? 폴더 안의 파일은 지워지지 않습니다.")
                .setPositiveButton("삭제", (d, w) -> {
                    try {
                        profiles().remove(editIndex);
                        persist(true);
                        if (ConfigStore.enabled(this)) {
                            if (profiles().length() == 0) SyncService.stop(this); else SyncService.start(this);
                        }
                        showList();
                    } catch (Exception e) { toast("삭제하지 못했습니다"); }
                })
                .setNegativeButton("취소", null)
                .show();
    }

    // ─────────────────────────── 동작 ───────────────────────────
    private void startSync() {
        String dir = ConfigStore.dataDir(this);
        JSONArray ps = profiles();
        for (int i = 0; i < ps.length(); i++) {
            JSONObject p = ps.optJSONObject(i);
            if (p == null || !p.optBoolean("enabled", true)) continue;
            JSONObject b = p.optJSONObject("backend");
            if (b != null && !Native.isLoggedIn(b.toString(), dir)) { toast("'" + displayName(p) + "' 프로필: 먼저 로그인해 주세요"); return; }
        }
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
            return;
        }
        if (!Environment.isExternalStorageManager()) {
            toast("먼저 '모든 파일 접근' 권한을 허용해 주세요");
            permWarn.performClick();
            return;
        }
        try {
            JSONObject v = new JSONObject(Native.validate(withDevice(cfg).toString()));
            if (!v.optBoolean("ok")) { toast(friendly(v.optString("error"))); return; }
            persist(true);
        } catch (Exception e) {
            toast("설정을 저장하지 못했습니다");
            return;
        }
        startSync();
    }

    private void testConnection() {
        String backend = backendStr();
        testResult.setTextColor(MUTED);
        testResult.setText("확인하는 중…");
        bg.execute(() -> {
            String r = Native.testConnection(backend, ConfigStore.dataDir(this));
            ui.post(() -> showTestResult(r, false));
        });
    }

    private void submitOtp() {
        String backend = backendStr(), otp = otpEdit.getText().toString().trim();
        if (otp.isEmpty()) { toast("인증 코드를 입력하세요"); return; }
        otpEdit.setText("");
        testResult.setTextColor(MUTED);
        testResult.setText("인증하는 중…");
        bg.execute(() -> {
            String dir = ConfigStore.dataDir(this);
            String r = Native.synologyLogin(backend, dir, otp);
            try { if (new JSONObject(r).optBoolean("ok")) r = Native.testConnection(backend, dir); } catch (Exception ignored) { }
            String fr = r;
            ui.post(() -> { showTestResult(fr, true); refreshLogin(); });
        });
    }

    private void showTestResult(String r, boolean afterOtp) {
        if (!editing) return;
        try {
            JSONObject o = new JSONObject(r);
            if (o.optBoolean("ok")) {
                int n = o.optInt("count");
                otpBox.setVisibility(View.GONE);
                testResult.setTextColor(OK);
                String pre = afterOtp ? "✔ 인증 완료! 다음부터는 코드를 묻지 않습니다. " : "✔ 연결 성공! ";
                testResult.setText(pre + (n == 0 ? "원격 폴더가 비어 있어(또는 아직 없어) 첫 동기화 때 파일을 올립니다." : "원격에 파일 " + n + "개가 있습니다."));
            } else if (o.optBoolean("need_otp")) {
                otpBox.setVisibility(View.VISIBLE);
                testResult.setTextColor(WARN);
                testResult.setText(o.optString("error").split("\n")[0]);
                otpEdit.requestFocus();
            } else {
                testResult.setTextColor(DANGER);
                testResult.setText("✖ " + friendly(o.optString("error")));
            }
        } catch (Exception ignored) { }
    }

    private void refreshLogin() {
        if (loginBox == null || !editing) return;
        loginBox.removeAllViews();
        String backend = backendStr(), dir = ConfigStore.dataDir(this);
        if (backendType.equals("synology")) {
            if (Native.synologyRemembered(backend, dir)) {
                LinearLayout row = hbox();
                row.addView(text("● 2단계 인증 완료 (이 휴대폰 기억됨)", 14, OK), weight());
                Button f = secondary("기억 해제");
                f.setOnClickListener(v -> { Native.synologyForget(backend, dir); refreshLogin(); });
                row.addView(f);
                loginBox.addView(row);
            }
            return;
        }
        if (!backendType.equals("one_drive") && !backendType.equals("google_drive")) return;
        if (loggingIn) return;
        boolean in = Native.isLoggedIn(backend, dir);
        LinearLayout row = hbox();
        TextView st = text(in ? "● 계정 연결됨" : "● 로그인 필요", 14, in ? OK : WARN);
        row.addView(st, weight());
        Button b = secondary(in ? "다시 로그인" : "로그인");
        b.setOnClickListener(v -> login());
        row.addView(b);
        if (in) {
            Button out = secondary("로그아웃");
            out.setOnClickListener(v -> { Native.logout(backend, dir); refreshLogin(); });
            row.addView(out);
        }
        loginBox.addView(row);
    }

    private void login() {
        String backend = backendStr();
        loggingIn = true;
        loginBox.removeAllViews();
        loginBox.addView(text("로그인 준비 중…", 14, MUTED));
        bg.execute(() -> {
            try {
                JSONObject s = new JSONObject(Native.loginStart(backend));
                if (!s.optBoolean("ok")) throw new Exception(s.optString("error"));
                String code = s.optString("user_code"), uri = s.optString("verification_uri");
                ui.post(() -> showCode(code, uri));
                JSONObject w = new JSONObject(Native.loginWait(backend, ConfigStore.dataDir(this)));
                if (!w.optBoolean("ok")) throw new Exception(w.optString("error"));
                ui.post(() -> { loggingIn = false; refreshLogin(); toast("로그인했습니다. 연결 테스트로 확인해 보세요"); });
            } catch (Exception e) {
                ui.post(() -> { loggingIn = false; refreshLogin(); toast(friendly(String.valueOf(e.getMessage()))); });
            }
        });
    }

    private void showCode(String code, String uri) {
        if (!editing) return;
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
        String json = cfg.toString(), id = edit.optString("id");
        bg.execute(() -> {
            String r = Native.resetState(json, ConfigStore.dataDir(this), id);
            ui.post(() -> toast(r.contains("\"ok\":true") ? "동기화 기록을 초기화했습니다" : "초기화하지 못했습니다: " + friendly(r)));
        });
    }

    // ─────────────────────────── 상태 갱신 ───────────────────────────
    private void tick() {
        ui.removeCallbacksAndMessages(null);
        if (editing || statusTitle == null) return;
        try {
            JSONObject s = new JSONObject(Native.status());
            boolean enabled = ConfigStore.enabled(this);
            String startErr = getSharedPreferences("state", 0).getString("start_error", null);
            String err = s.isNull("last_error") ? null : s.optString("last_error", null);
            if (s.optBoolean("syncing")) setStatus(ACCENT, "동기화 중…", "변경된 파일을 주고받고 있습니다");
            else if (enabled && startErr != null) setStatus(DANGER, "시작하지 못했습니다", friendly(startErr));
            else if (s.optBoolean("running") && err != null && !err.isEmpty()) setStatus(DANGER, "확인이 필요합니다", friendly(err));
            else if (s.optBoolean("running")) setStatus(OK, "실시간 동기화 켜짐",
                    s.isNull("last_sync") ? "첫 동기화를 준비하고 있습니다" : "마지막 동기화 " + s.optString("last_sync"));
            else if (enabled) setStatus(WARN, "다시 시작하는 중…", "잠시만 기다려 주세요");
            else if (profiles().length() == 0) setStatus(WARN, "설정이 필요합니다", "아래에서 동기화할 폴더를 추가하세요");
            else setStatus(MUTED, "동기화 꺼짐", "시작을 누르면 실시간 동기화가 켜집니다");
            toggleBtn.setText(enabled ? "일시 중지" : "동기화 시작");
            syncBtn.setEnabled(s.optBoolean("running") && !s.optBoolean("syncing"));

            // 프로필별 상태
            JSONArray pst = s.optJSONArray("profiles");
            for (TextView line : profileLines) {
                JSONObject st = null;
                if (pst != null) for (int i = 0; i < pst.length(); i++)
                    if (pst.getJSONObject(i).optString("id").equals(line.getTag())) st = pst.getJSONObject(i);
                if (st == null) { line.setText(s.optBoolean("running") ? "꺼짐" : ""); line.setTextColor(MUTED); }
                else if (st.optBoolean("syncing")) { line.setText("● 동기화 중…"); line.setTextColor(ACCENT); }
                else if (!st.isNull("last_error")) { line.setText("● " + friendly(st.optString("last_error"))); line.setTextColor(DANGER); }
                else if (!st.isNull("last_sync")) { line.setText("● " + st.optString("last_sync").substring(5) + " · " + st.optString("last_summary")); line.setTextColor(OK); }
                else { line.setText("● 준비 중"); line.setTextColor(MUTED); }
            }

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
        if (req == PICK_LOCAL && res == RESULT_OK && data != null && data.getData() != null && editing) {
            String path = treeToPath(data.getData());
            if (path == null) toast("이 위치는 선택할 수 없습니다. 내부 저장소의 폴더를 골라 주세요");
            else {
                localPath.setText(path);
                if (nameEdit.getText().toString().trim().isEmpty()) nameEdit.setText(path.substring(path.lastIndexOf('/') + 1));
            }
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
        if (raw.contains("Synology:")) hint = "";
        else if (raw.contains("HTTP 401") || raw.contains("HTTP 403") || raw.contains("SignatureDoesNotMatch") || raw.contains("InvalidAccessKeyId"))
            hint = "아이디/비밀번호 또는 키가 맞지 않거나 권한이 없습니다. ";
        else if (raw.contains("HTTP 404") || raw.contains("NoSuchBucket")) hint = "주소나 버킷/폴더 이름을 찾을 수 없습니다. ";
        else if (raw.contains("네트워크 오류")) hint = "서버에 연결할 수 없습니다. 주소와 인터넷 연결을 확인하세요. ";
        else if (raw.contains("다시 로그인") || raw.contains("invalid_grant")) hint = "로그인이 만료되었습니다. 다시 로그인해 주세요. ";
        else if (raw.contains("ertificate")) hint = "NAS 인증서를 신뢰할 수 없습니다. 정식 인증서(Let's Encrypt)를 쓰거나 집에서는 http://NAS주소:5000 을 사용하세요. ";
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

    private TextView label(String s) {
        TextView l = text(s, 13, MUTED);
        l.setPadding(0, dp(8), 0, 0);
        return l;
    }

    private EditText input(String value, String hint, boolean secret) {
        EditText e = new EditText(this);
        e.setSingleLine(true);
        e.setHint(hint);
        e.setTextSize(15);
        e.setInputType(InputType.TYPE_CLASS_TEXT | (secret ? InputType.TYPE_TEXT_VARIATION_PASSWORD : InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS));
        e.setText(value);
        return e;
    }

    private RadioButton radio(String s) {
        RadioButton r = new RadioButton(this);
        r.setText(s);
        r.setTextColor(TEXT);
        r.setId(View.generateViewId());
        return r;
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
