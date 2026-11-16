package io.bluewind.obsidiansync;

import android.annotation.SuppressLint;
import android.app.Activity;
import android.app.AlertDialog;
import android.content.Intent;
import android.content.pm.PackageManager;
import android.database.Cursor;
import android.graphics.Color;
import android.graphics.Insets;
import android.net.Uri;
import android.os.Build;
import android.os.Bundle;
import android.os.CancellationSignal;
import android.os.Environment;
import android.os.Handler;
import android.os.Looper;
import android.os.ParcelFileDescriptor;
import android.print.PageRange;
import android.print.PrintAttributes;
import android.print.PrintDocumentAdapter;
import android.print.PrintManager;
import android.provider.DocumentsContract;
import android.provider.OpenableColumns;
import android.provider.Settings;
import android.util.Log;
import android.view.ViewGroup;
import android.view.WindowInsets;
import android.webkit.ConsoleMessage;
import android.webkit.JavascriptInterface;
import android.webkit.ValueCallback;
import android.webkit.WebChromeClient;
import android.webkit.WebResourceError;
import android.webkit.WebResourceRequest;
import android.webkit.WebResourceResponse;
import android.webkit.WebSettings;
import android.webkit.WebView;
import android.webkit.WebViewClient;
import android.widget.FrameLayout;
import android.widget.Toast;
import androidx.work.Constraints;
import androidx.work.ExistingPeriodicWorkPolicy;
import androidx.work.NetworkType;
import androidx.work.PeriodicWorkRequest;
import androidx.work.WorkManager;
import java.io.ByteArrayInputStream;
import java.io.File;
import java.io.FileOutputStream;
import java.io.InputStream;
import java.io.OutputStream;
import java.nio.charset.StandardCharsets;
import java.util.Arrays;
import java.util.HashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.UUID;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.CopyOnWriteArrayList;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.TimeUnit;
import org.json.JSONObject;

/**
 * Obsidian Sync Studio 통합 Android 호스트 액티비티.
 * Windows 앱과 동일한 Notion + TOAST UI 에디터 · 볼트 · AES-256 암호화 · A4/PDF · 6종 실시간 동기화 UI를
 * Android WebView + Rust JNI 백엔드로 구동하고, SAF 파일 선택기·권한·PrintManager·자동 업데이터를 연결합니다.
 */
public final class MainActivity extends Activity {
    private static final String APP_ORIGIN = "https://appassets.androidplatform.net";
    private static final int REQ_PICK_FOLDER = 10;
    private static final int REQ_MANAGE_FILES = 11;
    private static final int REQ_PICK_ATTACHMENT = 12;
    private static final int REQ_WEB_FILE_CHOOSER = 13;

    private final ExecutorService bg = Executors.newCachedThreadPool();
    private final Handler ui = new Handler(Looper.getMainLooper());
    private final ConcurrentHashMap<String, String> pendingResponses = new ConcurrentHashMap<>();
    private final CopyOnWriteArrayList<String> webErrors = new CopyOnWriteArrayList<>();

    static volatile boolean visible;
    private WebView webView;
    private Updater.Release availableUpdate;

    WebView getWebViewForTest() {
        return webView;
    }

    List<String> getWebErrorsForTest() {
        return webErrors;
    }

    // Pending Android Intent picker state
    private String pendingFolderCallbackId;
    private String pendingFolderEndpoint;
    private String pendingAttachCallbackId;
    private String pendingAttachVault;
    private boolean pendingAttachImageOnly;
    private ValueCallback<Uri[]> webFileChooserCallback;

    @SuppressLint({"SetJavaScriptEnabled", "JavascriptInterface"})
    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);

        String dataDir = ConfigStore.dataDir(this);
        String legacyConfig = ConfigStore.load(this);
        Native.studioInit(dataDir, legacyConfig);

        if (ConfigStore.enabled(this)) {
            try { SyncService.attachForeground(this); } catch (RuntimeException ignored) { }
        }
        scheduleUpdateCheck();
        checkUpdateQuietly();

        webView = new WebView(this);
        WebSettings ws = webView.getSettings();
        ws.setJavaScriptEnabled(true);
        ws.setDomStorageEnabled(true);
        ws.setTextZoom(100);
        ws.setBuiltInZoomControls(false);
        ws.setDisplayZoomControls(false);
        ws.setAllowFileAccess(false);
        ws.setAllowContentAccess(false);

        webView.addJavascriptInterface(new StudioBridge(), "__OBSIDIAN_ANDROID__");

        webView.setWebViewClient(new WebViewClient() {
            @Override
            public WebResourceResponse shouldInterceptRequest(WebView view, WebResourceRequest req) {
                Uri uri = req.getUrl();
                if (uri != null && "https".equals(uri.getScheme())
                        && "appassets.androidplatform.net".equals(uri.getHost())) {
                    if ("OPTIONS".equalsIgnoreCase(req.getMethod())) {
                        return new WebResourceResponse(
                                "text/plain", "utf-8", 200, "OK",
                                corsHeaders("no-store"),
                                new ByteArrayInputStream(new byte[0]));
                    }
                    String path = uri.getPath();
                    if (path == null || path.isEmpty()) path = "/index.html";
                    if ("/vault-media".equals(path)) {
                        String q = uri.getEncodedQuery();
                        return toWebResourceResponse(q == null ? null : Native.studioVaultMedia(q));
                    }
                    return toWebResourceResponse(Native.studioWebAsset(path));
                }
                return super.shouldInterceptRequest(view, req);
            }

            @Override
            public void onReceivedError(WebView view, WebResourceRequest req, WebResourceError err) {
                super.onReceivedError(view, req, err);
                Uri uri = req == null ? null : req.getUrl();
                if (uri != null && "appassets.androidplatform.net".equals(uri.getHost())
                        && !"/favicon.ico".equals(uri.getPath())) {
                    String msg = "WebResourceError " + err.getErrorCode() + " (" + err.getDescription() + ") for " + uri;
                    Log.e("ObsidianSyncWeb", msg);
                    webErrors.add(msg);
                }
            }

            @Override
            public void onReceivedHttpError(WebView view, WebResourceRequest req, WebResourceResponse errResp) {
                super.onReceivedHttpError(view, req, errResp);
                Uri uri = req == null ? null : req.getUrl();
                if (uri != null && "appassets.androidplatform.net".equals(uri.getHost())
                        && !"/favicon.ico".equals(uri.getPath())) {
                    String msg = "WebHttpError " + errResp.getStatusCode() + " for " + uri;
                    Log.e("ObsidianSyncWeb", msg);
                    webErrors.add(msg);
                }
            }

            @Override
            public boolean shouldOverrideUrlLoading(WebView view, WebResourceRequest req) {
                Uri uri = req.getUrl();
                if (uri != null && "appassets.androidplatform.net".equals(uri.getHost())) {
                    return false;
                }
                if (uri != null && ("http".equals(uri.getScheme()) || "https".equals(uri.getScheme()))) {
                    try {
                        startActivity(new Intent(Intent.ACTION_VIEW, uri));
                    } catch (Exception ignored) { }
                    return true;
                }
                return false;
            }
        });

        webView.setWebChromeClient(new WebChromeClient() {
            @Override
            public boolean onConsoleMessage(ConsoleMessage cm) {
                if (cm != null && cm.messageLevel() == ConsoleMessage.MessageLevel.ERROR) {
                    String src = cm.sourceId() == null ? "" : cm.sourceId();
                    if (!src.contains("cdn.jsdelivr.net")) {
                        String msg = src + ":" + cm.lineNumber() + " " + cm.message();
                        Log.e("ObsidianSyncWeb", msg);
                        webErrors.add(msg);
                    }
                }
                return super.onConsoleMessage(cm);
            }

            @Override
            public boolean onShowFileChooser(WebView view, ValueCallback<Uri[]> cb, FileChooserParams params) {
                if (webFileChooserCallback != null) {
                    webFileChooserCallback.onReceiveValue(null);
                }
                webFileChooserCallback = cb;
                try {
                    Intent i = params.createIntent();
                    startActivityForResult(i, REQ_WEB_FILE_CHOOSER);
                    return true;
                } catch (Exception e) {
                    webFileChooserCallback = null;
                    return false;
                }
            }
        });

        FrameLayout root = new FrameLayout(this);
        root.setBackgroundColor(Color.WHITE);
        root.addView(webView, new FrameLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.MATCH_PARENT));
        if (Build.VERSION.SDK_INT >= 30) {
            root.setOnApplyWindowInsetsListener((v, windowInsets) -> {
                Insets bars = windowInsets.getInsets(
                        WindowInsets.Type.systemBars() | WindowInsets.Type.displayCutout());
                v.setPadding(bars.left, bars.top, bars.right, bars.bottom);
                return WindowInsets.CONSUMED;
            });
        }
        setContentView(root);
        webView.loadUrl(APP_ORIGIN + "/index.html");
    }

    @Override
    protected void onResume() {
        super.onResume();
        visible = true;
        if (webView != null) webView.onResume();
    }

    @Override
    protected void onPause() {
        visible = false;
        if (webView != null) webView.onPause();
        super.onPause();
    }

    @Override
    protected void onDestroy() {
        visible = false;
        bg.shutdown();
        if (webView != null) {
            webView.removeJavascriptInterface("__OBSIDIAN_ANDROID__");
            webView.destroy();
            webView = null;
        }
        super.onDestroy();
    }

    @Override
    public void onBackPressed() {
        if (webView != null) {
            webView.evaluateJavascript(
                    "window.__handleAndroidBack ? window.__handleAndroidBack() : false",
                    val -> {
                        if (!"true".equals(val) && !isFinishing() && !isDestroyed()) {
                            MainActivity.super.onBackPressed();
                        }
                    });
        } else {
            super.onBackPressed();
        }
    }

    private static Map<String, String> corsHeaders(String cacheControl) {
        Map<String, String> headers = new HashMap<>();
        headers.put("Access-Control-Allow-Origin", "*");
        headers.put("Access-Control-Allow-Methods", "GET, HEAD, OPTIONS");
        headers.put("Access-Control-Allow-Headers", "*");
        headers.put("Cache-Control", cacheControl);
        return headers;
    }

    private static WebResourceResponse toWebResourceResponse(byte[] packed) {
        if (packed == null) {
            return new WebResourceResponse("text/plain", "utf-8", 404, "Not Found",
                    corsHeaders("no-store"), new ByteArrayInputStream(new byte[0]));
        }
        int nl = -1;
        int limit = Math.min(packed.length, 160);
        for (int i = 0; i < limit; i++) {
            if (packed[i] == '\n') {
                nl = i;
                break;
            }
        }
        if (nl < 0) {
            return new WebResourceResponse("application/octet-stream", null, 200, "OK",
                    corsHeaders("no-store"), new ByteArrayInputStream(packed));
        }
        String fullMime = new String(packed, 0, nl, StandardCharsets.UTF_8);
        String mime = fullMime;
        String encoding = null;
        int semi = fullMime.indexOf(';');
        if (semi >= 0) {
            mime = fullMime.substring(0, semi).trim();
            String rest = fullMime.substring(semi + 1).toLowerCase(Locale.ROOT);
            if (rest.contains("utf-8")) encoding = "utf-8";
        }
        byte[] body = Arrays.copyOfRange(packed, nl + 1, packed.length);
        return new WebResourceResponse(
                mime,
                encoding,
                200,
                "OK",
                corsHeaders(mime.startsWith("image/") ? "private, max-age=60" : "no-store"),
                new ByteArrayInputStream(body));
    }

    // ─────────────────────────── JS Bridge ───────────────────────────
    final class StudioBridge {
        @JavascriptInterface
        public void invokeApiAsync(String callbackId, String endpoint, String bodyJson) {
            bg.execute(() -> handleApiCall(callbackId, endpoint, bodyJson == null ? "{}" : bodyJson));
        }

        @JavascriptInterface
        public String takeApiResponse(String callbackId) {
            String v = pendingResponses.remove(callbackId);
            return v == null ? "{\"ok\":false,\"error\":\"응답이 만료되었습니다\"}" : v;
        }
    }

    private void completeRpc(String callbackId, String jsonResponse) {
        pendingResponses.put(callbackId, jsonResponse);
        ui.post(() -> {
            if (webView != null) {
                webView.evaluateJavascript(
                        "window.__onAndroidRpcDone && window.__onAndroidRpcDone("
                                + JSONObject.quote(callbackId) + ");",
                        null);
            }
        });
    }

    private static String okJson(Object data) {
        try {
            return new JSONObject().put("ok", true).put("data", data).toString();
        } catch (Exception e) {
            return "{\"ok\":false,\"error\":\"직렬화 오류\"}";
        }
    }

    private static String errJson(String msg) {
        try {
            return new JSONObject().put("ok", false).put("error", msg == null ? "오류" : msg).toString();
        } catch (Exception e) {
            return "{\"ok\":false,\"error\":\"오류\"}";
        }
    }

    private void handleApiCall(String cbId, String endpoint, String bodyJson) {
        try {
            if ("workspace/open_vault_dialog".equals(endpoint) || "sync/pick_folder".equals(endpoint)) {
                ui.post(() -> startFolderPickerFlow(cbId, endpoint));
                return;
            }
            if ("vault/pick_attachment".equals(endpoint) || "vault/pick_file_attachment".equals(endpoint)) {
                JSONObject req = new JSONObject(bodyJson);
                String vault = req.optString("vault", "");
                boolean imageOnly = "vault/pick_attachment".equals(endpoint);
                ui.post(() -> startAttachmentPickerFlow(cbId, vault, imageOnly));
                return;
            }
            if ("note/export_pdf".equals(endpoint)) {
                JSONObject req = new JSONObject(bodyJson);
                String name = req.optString("default_filename", "document.pdf").replaceAll("\\.pdf$", "");
                boolean landscape = req.optBoolean("landscape", false);
                ui.post(() -> startAndroidPrint(cbId, name, landscape));
                return;
            }
            if ("sync/open_path".equals(endpoint)) {
                String path = new JSONObject(bodyJson).optString("path", "").trim();
                if (path.startsWith("http://") || path.startsWith("https://")) {
                    ui.post(() -> {
                        try { startActivity(new Intent(Intent.ACTION_VIEW, Uri.parse(path))); }
                        catch (Exception ignored) { }
                    });
                } else if (!path.isEmpty()) {
                    ui.post(() -> Toast.makeText(this, "경로: " + path, Toast.LENGTH_SHORT).show());
                }
                completeRpc(cbId, okJson(new JSONObject()));
                return;
            }
            if ("sync/open_data_dir".equals(endpoint)) {
                String dir = ConfigStore.dataDir(this);
                ui.post(() -> Toast.makeText(this, "설정 폴더: " + dir, Toast.LENGTH_SHORT).show());
                completeRpc(cbId, okJson(new JSONObject()));
                return;
            }
            if ("android/info".equals(endpoint)) {
                JSONObject info = new JSONObject()
                        .put("version", Updater.currentName(this))
                        .put("auto_update", Updater.autoEnabled(this))
                        .put("all_files_granted", Environment.isExternalStorageManager())
                        .put("available_update_tag", availableUpdate == null ? JSONObject.NULL : availableUpdate.tag)
                        .put("last_update_message", Updater.lastMessage(this));
                completeRpc(cbId, okJson(info));
                return;
            }
            if ("android/grant_files_access".equals(endpoint)) {
                ui.post(() -> {
                    try {
                        startActivity(new Intent(
                                Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION,
                                Uri.parse("package:" + getPackageName())));
                    } catch (Exception e) {
                        Toast.makeText(this, "설정 화면을 열 수 없습니다.", Toast.LENGTH_SHORT).show();
                    }
                });
                completeRpc(cbId, okJson(new JSONObject().put("opened", true)));
                return;
            }
            if ("android/set_auto_update".equals(endpoint)) {
                boolean on = new JSONObject(bodyJson).optBoolean("enabled", true);
                Updater.setAutoEnabled(this, on);
                scheduleUpdateCheck();
                completeRpc(cbId, okJson(new JSONObject().put("auto_update", on)));
                return;
            }
            if ("android/update_now".equals(endpoint)) {
                handleUpdateNowRpc(cbId);
                return;
            }

            String res = Native.studioApi("/api/" + endpoint, bodyJson);
            if ("sync/save_config".equals(endpoint)
                    || "sync/start".equals(endpoint)
                    || "sync/stop".equals(endpoint)
                    || "sync/set_autostart".equals(endpoint)) {
                syncAndroidServiceHook(endpoint, res);
            }
            completeRpc(cbId, res);
        } catch (Exception e) {
            completeRpc(cbId, errJson(e.getMessage()));
        }
    }

    private void syncAndroidServiceHook(String endpoint, String resJson) {
        try {
            String savedCfg = Native.studioSavedConfigJson();
            if (!savedCfg.isEmpty()) {
                ConfigStore.save(this, savedCfg);
            }
            JSONObject resp = new JSONObject(resJson);
            if (!resp.optBoolean("ok")) return;
            JSONObject snap = resp.optJSONObject("data");
            boolean running = snap != null && (snap.optBoolean("service_running") || snap.optBoolean("transitioning"));

            if ("sync/stop".equals(endpoint)) {
                ConfigStore.setEnabled(this, false);
                WorkManager.getInstance(this).cancelUniqueWork("obsidian-sync");
                SyncService.stop(this);
            } else if (running || "sync/start".equals(endpoint)) {
                ConfigStore.setEnabled(this, true);
                ui.post(() -> {
                    if (Build.VERSION.SDK_INT >= 33
                            && checkSelfPermission(android.Manifest.permission.POST_NOTIFICATIONS)
                                    != PackageManager.PERMISSION_GRANTED) {
                        requestPermissions(new String[]{android.Manifest.permission.POST_NOTIFICATIONS}, 1);
                    }
                    try { SyncService.attachForeground(this); } catch (RuntimeException ignored) { }
                    Constraints net = new Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build();
                    WorkManager.getInstance(this).enqueueUniquePeriodicWork(
                            "obsidian-sync",
                            ExistingPeriodicWorkPolicy.UPDATE,
                            new PeriodicWorkRequest.Builder(SyncWorker.class, 15, TimeUnit.MINUTES)
                                    .setConstraints(net)
                                    .build());
                });
            }
        } catch (Exception ignored) { }
    }

    // ─────────────────────────── Folder / Attachment Pickers ───────────────────────────
    private void finishPendingFolderCancelled() {
        String cbId = pendingFolderCallbackId;
        String ep = pendingFolderEndpoint;
        pendingFolderCallbackId = null;
        pendingFolderEndpoint = null;
        if (cbId == null) return;
        bg.execute(() -> {
            try {
                if ("sync/pick_folder".equals(ep)) {
                    completeRpc(cbId, okJson(new JSONObject().put("path", JSONObject.NULL)));
                    return;
                }
                JSONObject boot = new JSONObject(Native.studioApi("/api/bootstrap", "{}"));
                JSONObject data = boot.optJSONObject("data");
                if (data != null) data.put("picked", JSONObject.NULL);
                completeRpc(cbId, boot.toString());
            } catch (Exception e) {
                completeRpc(cbId, errJson(e.getMessage()));
            }
        });
    }

    private void startFolderPickerFlow(String cbId, String endpoint) {
        if (pendingFolderCallbackId != null) {
            completeRpc(cbId, errJson("이미 폴더 선택기가 열려 있습니다."));
            return;
        }
        pendingFolderCallbackId = cbId;
        pendingFolderEndpoint = endpoint;

        if (!Environment.isExternalStorageManager()) {
            new AlertDialog.Builder(this)
                    .setTitle("모든 파일 접근 권한 필요")
                    .setMessage("휴대폰 저장소의 Obsidian 볼트 폴더(.md 파일 및 첨부파일)를 읽고 쓰려면 '모든 파일 접근 권한' 허용이 필요합니다.")
                    .setPositiveButton("권한 설정 열기", (d, w) -> {
                        try {
                            startActivityForResult(
                                    new Intent(Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION,
                                            Uri.parse("package:" + getPackageName())),
                                    REQ_MANAGE_FILES);
                        } catch (Exception e) {
                            finishPendingFolderCancelled();
                        }
                    })
                    .setNegativeButton("취소", (d, w) -> finishPendingFolderCancelled())
                    .setOnCancelListener(d -> finishPendingFolderCancelled())
                    .show();
            return;
        }
        launchDocumentTreePicker();
    }

    private void launchDocumentTreePicker() {
        try {
            startActivityForResult(new Intent(Intent.ACTION_OPEN_DOCUMENT_TREE), REQ_PICK_FOLDER);
        } catch (Exception e) {
            finishPendingFolderCancelled();
        }
    }

    private void startAttachmentPickerFlow(String cbId, String vault, boolean imageOnly) {
        if (pendingAttachCallbackId != null) {
            completeRpc(cbId, errJson("이미 파일 선택기가 열려 있습니다."));
            return;
        }
        pendingAttachCallbackId = cbId;
        pendingAttachVault = vault;
        pendingAttachImageOnly = imageOnly;
        try {
            Intent i = new Intent(Intent.ACTION_OPEN_DOCUMENT);
            i.addCategory(Intent.CATEGORY_OPENABLE);
            i.setType(imageOnly ? "image/*" : "*/*");
            startActivityForResult(i, REQ_PICK_ATTACHMENT);
        } catch (Exception e) {
            pendingAttachCallbackId = null;
            completeRpc(cbId, errJson("파일 선택기를 열 수 없습니다."));
        }
    }

    @Override
    protected void onActivityResult(int req, int res, Intent data) {
        super.onActivityResult(req, res, data);

        if (req == REQ_WEB_FILE_CHOOSER) {
            ValueCallback<Uri[]> cb = webFileChooserCallback;
            webFileChooserCallback = null;
            if (cb != null) {
                Uri[] uris = WebChromeClient.FileChooserParams.parseResult(res, data);
                cb.onReceiveValue(uris);
            }
            return;
        }

        if (req == REQ_MANAGE_FILES) {
            if (pendingFolderCallbackId != null) {
                if (Environment.isExternalStorageManager()) {
                    launchDocumentTreePicker();
                } else {
                    String cbId = pendingFolderCallbackId;
                    pendingFolderCallbackId = null;
                    pendingFolderEndpoint = null;
                    completeRpc(cbId, errJson("모든 파일 접근 권한이 허용되지 않았습니다."));
                }
            }
            return;
        }

        if (req == REQ_PICK_FOLDER) {
            String cbId = pendingFolderCallbackId;
            String ep = pendingFolderEndpoint;
            pendingFolderCallbackId = null;
            pendingFolderEndpoint = null;
            if (cbId == null) return;
            if (res != RESULT_OK || data == null || data.getData() == null) {
                pendingFolderCallbackId = cbId;
                pendingFolderEndpoint = ep;
                finishPendingFolderCancelled();
                return;
            }
            String pickedPath = treeToPath(data.getData());
            if (pickedPath == null) {
                completeRpc(cbId, errJson("내부 저장소의 폴더를 골라 주세요."));
                return;
            }
            bg.execute(() -> {
                try {
                    if ("sync/pick_folder".equals(ep)) {
                        completeRpc(cbId, okJson(new JSONObject().put("path", pickedPath)));
                    } else {
                        String raw = Native.studioApi("/api/workspace/select_vault",
                                new JSONObject().put("path", pickedPath).toString());
                        JSONObject o = new JSONObject(raw);
                        if (o.optBoolean("ok")) {
                            JSONObject d = o.optJSONObject("data");
                            if (d != null) d.put("picked", pickedPath);
                        }
                        completeRpc(cbId, o.toString());
                    }
                } catch (Exception e) {
                    completeRpc(cbId, errJson(e.getMessage()));
                }
            });
            return;
        }

        if (req == REQ_PICK_ATTACHMENT) {
            String cbId = pendingAttachCallbackId;
            String vault = pendingAttachVault;
            boolean imageOnly = pendingAttachImageOnly;
            pendingAttachCallbackId = null;
            pendingAttachVault = null;
            if (cbId == null) return;

            if (res != RESULT_OK || data == null || data.getData() == null) {
                completeRpc(cbId, Native.studioSavePickedAttachment(vault == null ? "" : vault, "", imageOnly));
                return;
            }
            Uri uri = data.getData();
            bg.execute(() -> {
                File tmpDir = new File(getCacheDir(), "pick_" + UUID.randomUUID());
                try {
                    if (!tmpDir.mkdirs() && !tmpDir.isDirectory()) {
                        throw new Exception("임시 캐시 폴더를 만들 수 없습니다.");
                    }
                    String name = queryDisplayName(uri);
                    if (name == null || name.isEmpty()) {
                        name = imageOnly ? "image.png" : "attachment.bin";
                    }
                    name = name.replaceAll("[\\\\/:*?\"<>|]", "_");
                    File tmpFile = new File(tmpDir, name);
                    try (InputStream in = getContentResolver().openInputStream(uri);
                            OutputStream out = new FileOutputStream(tmpFile)) {
                        if (in == null) throw new Exception("선택한 파일을 읽을 수 없습니다.");
                        byte[] buf = new byte[32 * 1024];
                        for (int n; (n = in.read(buf)) > 0; ) out.write(buf, 0, n);
                    }
                    completeRpc(cbId, Native.studioSavePickedAttachment(
                            vault == null ? "" : vault, tmpFile.getAbsolutePath(), imageOnly));
                } catch (Exception e) {
                    completeRpc(cbId, errJson(e.getMessage()));
                } finally {
                    deleteRecursive(tmpDir);
                }
            });
        }
    }

    private String queryDisplayName(Uri uri) {
        try (Cursor c = getContentResolver().query(uri, new String[]{OpenableColumns.DISPLAY_NAME}, null, null, null)) {
            if (c != null && c.moveToFirst()) {
                int idx = c.getColumnIndex(OpenableColumns.DISPLAY_NAME);
                if (idx >= 0) return c.getString(idx);
            }
        } catch (Exception ignored) { }
        return null;
    }

    private static void deleteRecursive(File f) {
        if (f == null || !f.exists()) return;
        File[] children = f.listFiles();
        if (children != null) {
            for (File c : children) deleteRecursive(c);
        }
        //noinspection ResultOfMethodCallIgnored
        f.delete();
    }

    /** SAF 폴더 URI → 실제 경로 (예: primary:Documents/Vault → /storage/emulated/0/Documents/Vault) */
    static String treeToPath(Uri uri) {
        try {
            String id = DocumentsContract.getTreeDocumentId(uri);
            int i = id.indexOf(':');
            if (i < 0) return null;
            String vol = id.substring(0, i), rel = id.substring(i + 1);
            String base = vol.equalsIgnoreCase("primary")
                    ? Environment.getExternalStorageDirectory().getAbsolutePath()
                    : "/storage/" + vol;
            return rel.isEmpty() ? base : base + "/" + rel;
        } catch (Exception e) {
            return null;
        }
    }

    // ─────────────────────────── Print / PDF Export ───────────────────────────
    private void startAndroidPrint(String cbId, String docTitle, boolean landscape) {
        if (webView == null) {
            completeRpc(cbId, errJson("WebView가 준비되지 않았습니다."));
            return;
        }
        PrintManager pm = (PrintManager) getSystemService(PRINT_SERVICE);
        if (pm == null) {
            completeRpc(cbId, errJson("인쇄 서비스에 접근할 수 없습니다."));
            return;
        }
        String cleanName = docTitle == null || docTitle.trim().isEmpty() ? "ObsidianSyncDocument" : docTitle.trim();
        PrintAttributes.MediaSize mediaSize = landscape
                ? PrintAttributes.MediaSize.ISO_A4.asLandscape()
                : PrintAttributes.MediaSize.ISO_A4.asPortrait();
        PrintAttributes attrs = new PrintAttributes.Builder()
                .setMediaSize(mediaSize)
                .setMinMargins(PrintAttributes.Margins.NO_MARGINS)
                .build();

        PrintDocumentAdapter base = webView.createPrintDocumentAdapter(cleanName);
        PrintDocumentAdapter wrapped = new PrintDocumentAdapter() {
            @Override public void onStart() { base.onStart(); }

            @Override
            public void onLayout(PrintAttributes oldA, PrintAttributes newA,
                    CancellationSignal cancel, LayoutResultCallback cb, Bundle extras) {
                base.onLayout(oldA, newA, cancel, cb, extras);
            }

            @Override
            public void onWrite(PageRange[] pages, ParcelFileDescriptor dest,
                    CancellationSignal cancel, WriteResultCallback cb) {
                base.onWrite(pages, dest, cancel, cb);
            }

            @Override
            public void onFinish() {
                try {
                    base.onFinish();
                } finally {
                    ui.post(() -> {
                        if (webView != null) {
                            webView.evaluateJavascript(
                                    "var p=document.getElementById('print-root');if(p)p.innerHTML='';",
                                    null);
                        }
                    });
                }
            }
        };
        pm.print(cleanName, wrapped, attrs);
        try {
            JSONObject d = new JSONObject()
                    .put("exported", true)
                    .put("path", JSONObject.NULL)
                    .put("delegated_to_system_print", true);
            completeRpc(cbId, okJson(d));
        } catch (Exception e) {
            completeRpc(cbId, errJson(e.getMessage()));
        }
    }

    // ─────────────────────────── App Updater ───────────────────────────
    private void scheduleUpdateCheck() {
        WorkManager wm = WorkManager.getInstance(this);
        if (!Updater.autoEnabled(this)) {
            wm.cancelUniqueWork("app-update");
            return;
        }
        Constraints net = new Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build();
        wm.enqueueUniquePeriodicWork("app-update", ExistingPeriodicWorkPolicy.KEEP,
                new PeriodicWorkRequest.Builder(UpdateWorker.class, 6, TimeUnit.HOURS)
                        .setConstraints(net)
                        .build());
    }

    private void checkUpdateQuietly() {
        bg.execute(() -> {
            try {
                availableUpdate = Updater.check(this);
            } catch (Exception ignored) { }
        });
    }

    private void handleUpdateNowRpc(String cbId) {
        if (!getPackageManager().canRequestPackageInstalls()) {
            ui.post(() -> {
                Toast.makeText(this, "'이 출처 허용'을 켠 뒤 돌아와서 다시 눌러 주세요", Toast.LENGTH_LONG).show();
                try {
                    startActivity(new Intent(
                            Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES,
                            Uri.parse("package:" + getPackageName())));
                } catch (Exception ignored) { }
            });
            try {
                completeRpc(cbId, okJson(new JSONObject().put("message", "'알 수 없는 앱 설치' 권한을 허용해 주세요.")));
            } catch (Exception e) {
                completeRpc(cbId, errJson(e.getMessage()));
            }
            return;
        }
        try {
            String tag = Updater.run(this);
            availableUpdate = null;
            String msg = tag == null ? "이미 최신 버전입니다." : tag + " 업데이트 설치를 시작했습니다.";
            completeRpc(cbId, okJson(new JSONObject().put("message", msg).put("tag", tag == null ? JSONObject.NULL : tag)));
        } catch (Exception e) {
            String errMsg = "업데이트 확인/설치 실패: " + e.getMessage();
            Updater.setMessage(this, errMsg);
            completeRpc(cbId, errJson(errMsg));
        }
    }
}
