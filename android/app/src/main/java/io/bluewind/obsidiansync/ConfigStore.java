package io.bluewind.obsidiansync;
import android.content.Context;
import android.security.keystore.KeyGenParameterSpec;
import android.security.keystore.KeyProperties;
import android.util.Base64;
import java.security.KeyStore;
import javax.crypto.Cipher;
import javax.crypto.KeyGenerator;
import javax.crypto.SecretKey;
import javax.crypto.spec.GCMParameterSpec;

final class ConfigStore {
    private static final String ALIAS = "obsidian-sync-config";
    private static SecretKey key() throws Exception {
        KeyStore store = KeyStore.getInstance("AndroidKeyStore"); store.load(null);
        if (!store.containsAlias(ALIAS)) {
            KeyGenerator generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore");
            generator.init(new KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_ENCRYPT | KeyProperties.PURPOSE_DECRYPT).setBlockModes(KeyProperties.BLOCK_MODE_GCM).setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE).build());
            generator.generateKey();
        }
        return (SecretKey) store.getKey(ALIAS, null);
    }
    static synchronized void save(Context c, String value) throws Exception {
        Cipher cipher = Cipher.getInstance("AES/GCM/NoPadding"); cipher.init(Cipher.ENCRYPT_MODE, key());
        String encrypted = Base64.encodeToString(cipher.doFinal(value.getBytes(java.nio.charset.StandardCharsets.UTF_8)), Base64.NO_WRAP);
        String iv = Base64.encodeToString(cipher.getIV(), Base64.NO_WRAP);
        if (!c.getSharedPreferences("secure", 0).edit().putString("data", encrypted).putString("iv", iv).commit()) throw new java.io.IOException("Cannot save settings");
    }
    static synchronized String load(Context c) throws Exception {
        var prefs = c.getSharedPreferences("secure", 0);
        if (!prefs.contains("data")) return "";
        Cipher cipher = Cipher.getInstance("AES/GCM/NoPadding");
        cipher.init(Cipher.DECRYPT_MODE, key(), new GCMParameterSpec(128, Base64.decode(prefs.getString("iv", ""), Base64.NO_WRAP)));
        return new String(cipher.doFinal(Base64.decode(prefs.getString("data", ""), Base64.NO_WRAP)), java.nio.charset.StandardCharsets.UTF_8);
    }
    static synchronized String run(Context c) {
        try {
            String config = load(c);
            if (config.isEmpty()) return "{\"ok\":false,\"error\":\"Save connection settings first\"}";
            String result = Native.sync(config, new java.io.File(c.getFilesDir(), "state").getAbsolutePath());
            c.getSharedPreferences("status", 0).edit().putString("last", result).apply();
            return result;
        } catch (Exception | LinkageError e) { return "{\"ok\":false,\"error\":\"Check storage permission and connection settings\"}"; }
    }
}
