package io.bluewind.obsidiansync;
public final class Native {
    static { System.loadLibrary("vault_sync_android"); }
    private Native() {}
    public static native String sync(String config, String state);
}
