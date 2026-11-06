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
