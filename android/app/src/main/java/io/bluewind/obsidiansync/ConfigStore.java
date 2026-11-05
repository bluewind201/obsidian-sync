package io.bluewind.obsidiansync;

import android.content.Context;
import android.security.keystore.KeyGenParameterSpec;
import android.security.keystore.KeyProperties;
import android.util.Base64;
import java.io.File;
import java.nio.charset.StandardCharsets;
import java.security.KeyStore;
import javax.crypto.Cipher;
import javax.crypto.KeyGenerator;
import javax.crypto.SecretKey;
import javax.crypto.spec.GCMParameterSpec;

/** 설정(JSON)을 Android Keystore로 암호화해 저장합니다. 비밀번호/키가 포함되기 때문입니다. */
final class ConfigStore {
    private static final String ALIAS = "obsidian-sync-config";
    private static final String PREFS = "secure-v2";

    private static SecretKey key() throws Exception {
        KeyStore store = KeyStore.getInstance("AndroidKeyStore");
        store.load(null);
        if (!store.containsAlias(ALIAS)) {
            KeyGenerator g = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore");
            g.init(new KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_ENCRYPT | KeyProperties.PURPOSE_DECRYPT)
                    .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                    .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                    .build());
            g.generateKey();
        }
        return (SecretKey) store.getKey(ALIAS, null);
    }

    static synchronized void save(Context c, String json) throws Exception {
        Cipher cipher = Cipher.getInstance("AES/GCM/NoPadding");
        cipher.init(Cipher.ENCRYPT_MODE, key());
        String data = Base64.encodeToString(cipher.doFinal(json.getBytes(StandardCharsets.UTF_8)), Base64.NO_WRAP);
        String iv = Base64.encodeToString(cipher.getIV(), Base64.NO_WRAP);
        if (!c.getSharedPreferences(PREFS, 0).edit().putString("data", data).putString("iv", iv).commit())
            throw new java.io.IOException("설정을 저장할 수 없습니다");
    }

    /** 저장된 설정 JSON. 없으면 빈 문자열 */
    static synchronized String load(Context c) {
        try {
            var p = c.getSharedPreferences(PREFS, 0);
            if (!p.contains("data")) return "";
            Cipher cipher = Cipher.getInstance("AES/GCM/NoPadding");
            cipher.init(Cipher.DECRYPT_MODE, key(), new GCMParameterSpec(128, Base64.decode(p.getString("iv", ""), Base64.NO_WRAP)));
            return new String(cipher.doFinal(Base64.decode(p.getString("data", ""), Base64.NO_WRAP)), StandardCharsets.UTF_8);
        } catch (Exception e) {
            return "";
        }
    }

    static String dataDir(Context c) {
        File d = new File(c.getFilesDir(), "sync");
        //noinspection ResultOfMethodCallIgnored
        d.mkdirs();
        return d.getAbsolutePath();
    }

    /** 실시간 동기화를 켜둔 상태인지 (재부팅 후 자동 시작용) */
    static boolean enabled(Context c) { return c.getSharedPreferences("state", 0).getBoolean("enabled", false); }
    static void setEnabled(Context c, boolean on) { c.getSharedPreferences("state", 0).edit().putBoolean("enabled", on).apply(); }
}
