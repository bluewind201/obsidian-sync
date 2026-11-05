package io.bluewind.obsidiansync;

import androidx.test.ext.junit.runners.AndroidJUnit4;
import androidx.test.platform.app.InstrumentationRegistry;
import org.junit.Test;
import org.junit.runner.RunWith;
import org.json.JSONObject;
import java.io.File;
import java.nio.file.Files;
import static org.junit.Assert.*;

@RunWith(AndroidJUnit4.class)
public final class NativeSyncTest {
    private String config(File vault, File remote) throws Exception {
        return new JSONObject().put("vault", vault.getAbsolutePath()).put("backend", "fs")
            .put("options", new JSONObject().put("root", remote.getAbsolutePath())).toString();
    }
    @Test public void nativeEngineReplicatesFilesOnAndroid() throws Exception {
        File root = new File(InstrumentationRegistry.getInstrumentation().getTargetContext().getCacheDir(), java.util.UUID.randomUUID().toString());
        File a = new File(root, "a"), b = new File(root, "b"), remote = new File(root, "remote");
        assertTrue(a.mkdirs()); assertTrue(b.mkdirs()); assertTrue(remote.mkdirs());
        Files.write(new File(a, "한글.md").toPath(), "Android JNI roundtrip".getBytes(java.nio.charset.StandardCharsets.UTF_8));
        assertTrue(new JSONObject(Native.sync(config(a, remote), new File(root, "sa").getAbsolutePath())).getBoolean("ok"));
        assertTrue(new JSONObject(Native.sync(config(b, remote), new File(root, "sb").getAbsolutePath())).getBoolean("ok"));
        assertEquals("Android JNI roundtrip", new String(Files.readAllBytes(new File(b, "한글.md").toPath()), java.nio.charset.StandardCharsets.UTF_8));
    }
    @Test public void invalidConfigFailsWithoutCrashingJvm() throws Exception {
        String result = Native.sync("invalid json", InstrumentationRegistry.getInstrumentation().getTargetContext().getCacheDir().getAbsolutePath());
        assertFalse(new JSONObject(result).getBoolean("ok"));
    }
    @Test public void activityLaunches() {
        try (androidx.test.core.app.ActivityScenario<MainActivity> activity = androidx.test.core.app.ActivityScenario.launch(MainActivity.class)) {
            activity.onActivity(a -> assertNotNull(a.findViewById(android.R.id.content)));
        }
    }
}
