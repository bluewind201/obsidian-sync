package io.bluewind.obsidiansync;

import static org.junit.Assert.*;

import androidx.test.ext.junit.runners.AndroidJUnit4;
import androidx.test.platform.app.InstrumentationRegistry;
import java.io.File;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import org.json.JSONObject;
import org.junit.Test;
import org.junit.runner.RunWith;

@RunWith(AndroidJUnit4.class)
public final class NativeSyncTest {
    private static String config(File vault, File remote) throws Exception {
        return new JSONObject()
                .put("vault_path", vault.getAbsolutePath())
                .put("backend", new JSONObject().put("type", "local_folder").put("path", remote.getAbsolutePath()))
                .toString();
    }

    private static File tmp() {
        return new File(InstrumentationRegistry.getInstrumentation().getTargetContext().getCacheDir(), java.util.UUID.randomUUID().toString());
    }

    @Test public void nativeEngineReplicatesFilesOnAndroid() throws Exception {
        File root = tmp();
        File a = new File(root, "a"), b = new File(root, "b"), remote = new File(root, "remote");
        assertTrue(a.mkdirs() && b.mkdirs() && remote.mkdirs());
        Files.write(new File(a, "한글.md").toPath(), "Android JNI roundtrip".getBytes(StandardCharsets.UTF_8));
        assertTrue(new JSONObject(Native.syncOnce(config(a, remote), new File(root, "sa").getAbsolutePath())).getBoolean("ok"));
        assertTrue(new File(remote, "한글.md").exists());
        assertTrue(new JSONObject(Native.syncOnce(config(b, remote), new File(root, "sb").getAbsolutePath())).getBoolean("ok"));
        assertEquals("Android JNI roundtrip", new String(Files.readAllBytes(new File(b, "한글.md").toPath()), StandardCharsets.UTF_8));
    }

    @Test public void realtimeServiceSyncsChanges() throws Exception {
        File root = tmp();
        File a = new File(root, "a"), remote = new File(root, "remote");
        assertTrue(a.mkdirs() && remote.mkdirs());
        JSONObject r = new JSONObject(Native.start(config(a, remote), new File(root, "s").getAbsolutePath()));
        assertTrue(r.toString(), r.getBoolean("ok"));
        try {
            Files.write(new File(a, "live.md").toPath(), "watch".getBytes(StandardCharsets.UTF_8));
            File target = new File(remote, "live.md");
            for (int i = 0; i < 100 && !target.exists(); i++) Thread.sleep(100);
            assertTrue("file watcher should upload within 10s", target.exists());
            assertTrue(new JSONObject(Native.status()).getBoolean("running"));
        } finally {
            Native.stop();
        }
    }

    @Test public void multipleProfilesWithBackupMode() throws Exception {
        File root = tmp();
        File notes = new File(root, "notes"), photos = new File(root, "photos"), remote = new File(root, "remote");
        assertTrue(notes.mkdirs() && photos.mkdirs() && remote.mkdirs());
        Files.write(new File(notes, "n.md").toPath(), "note".getBytes(StandardCharsets.UTF_8));
        Files.write(new File(photos, "p.jpg").toPath(), new byte[200_000]);
        JSONObject cfg = new JSONObject().put("profiles", new org.json.JSONArray()
                .put(new JSONObject().put("id", "n").put("local_path", notes.getAbsolutePath())
                        .put("backend", new JSONObject().put("type", "local_folder").put("path", new File(remote, "notes").getAbsolutePath())))
                .put(new JSONObject().put("id", "p").put("kind", "folder").put("mode", "upload_only").put("local_path", photos.getAbsolutePath())
                        .put("backend", new JSONObject().put("type", "local_folder").put("path", new File(remote, "photos").getAbsolutePath()))));
        assertTrue(new JSONObject(Native.validate(cfg.toString())).getBoolean("ok"));
        assertTrue(new JSONObject(Native.syncOnce(cfg.toString(), new File(root, "s").getAbsolutePath())).getBoolean("ok"));
        assertTrue(new File(remote, "notes/n.md").exists());
        assertEquals(200_000, new File(remote, "photos/p.jpg").length());
        // 백업 모드: 휴대폰에서 지워도 원격에 남음
        assertTrue(new File(photos, "p.jpg").delete());
        assertTrue(new JSONObject(Native.syncOnce(cfg.toString(), new File(root, "s").getAbsolutePath())).getBoolean("ok"));
        assertTrue(new File(remote, "photos/p.jpg").exists());
    }

    @Test public void legacyConfigIsNormalized() throws Exception {
        JSONObject r = new JSONObject(Native.normalize("{\"vault_path\":\"/sdcard/V\",\"backend\":{\"type\":\"web_dav\",\"url\":\"https://nas:5006/home/V\",\"username\":\"u\",\"password\":\"p\"}}"));
        assertTrue(r.getBoolean("ok"));
        JSONObject p = r.getJSONObject("config").getJSONArray("profiles").getJSONObject(0);
        assertEquals("/sdcard/V", p.getString("local_path"));
        assertEquals("synology", p.getJSONObject("backend").getString("type"));
        assertEquals("https://nas:5001", p.getJSONObject("backend").getString("url"));
        assertFalse(Native.newProfileId().isEmpty());
    }

    @Test public void synologyErrorsDoNotCrash() throws Exception {
        String dir = InstrumentationRegistry.getInstrumentation().getTargetContext().getCacheDir().getAbsolutePath();
        String b = new JSONObject().put("type", "synology").put("url", "http://127.0.0.1:9").put("username", "u").put("password", "p").put("folder", "/home/x").toString();
        assertFalse(new JSONObject(Native.testConnection(b, dir)).getBoolean("ok"));
        assertFalse(new JSONObject(Native.synologyLogin(b, dir, "123456")).getBoolean("ok"));
        assertFalse(Native.synologyRemembered(b, dir));
    }

    @Test public void invalidConfigFailsWithoutCrashingJvm() throws Exception {
        String dir = InstrumentationRegistry.getInstrumentation().getTargetContext().getCacheDir().getAbsolutePath();
        assertFalse(new JSONObject(Native.syncOnce("invalid json", dir)).getBoolean("ok"));
        assertFalse(new JSONObject(Native.validate("{}")).getBoolean("ok"));
    }

    @Test public void treeUriConvertsToPath() {
        android.net.Uri u = android.provider.DocumentsContract.buildTreeDocumentUri("com.android.externalstorage.documents", "primary:Documents/Vault");
        assertTrue(MainActivity.treeToPath(u).endsWith("/Documents/Vault"));
    }

    @Test public void activityLaunches() {
        try (androidx.test.core.app.ActivityScenario<MainActivity> s = androidx.test.core.app.ActivityScenario.launch(MainActivity.class)) {
            s.onActivity(act -> assertNotNull(act.findViewById(android.R.id.content)));
        }
    }
}
