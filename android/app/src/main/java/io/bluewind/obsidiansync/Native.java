package io.bluewind.obsidiansync;

/** Rust 동기화 엔진(JNI). 모든 결과는 JSON 문자열입니다. */
public final class Native {
    static { System.loadLibrary("obsidian_sync"); }
    private Native() {}

    /** 실시간 동기화 시작 (파일 감시 + 주기 확인). {"ok":bool,"error"?} */
    public static native String start(String config, String dataDir);
    public static native void stop();
    public static native boolean syncNow();
    /** {"running","syncing","last_sync","last_summary","last_error","logs":[{time,text,error}]} */
    public static native String status();
    /** 한 번만 동기화. {"ok","summary"?,"error"?} */
    public static native String syncOnce(String config, String dataDir);
    public static native String testConnection(String config, String dataDir);
    public static native String validate(String config);
    /** OneDrive/Google Drive 로그인 1단계. {"ok","user_code","verification_uri"} */
    public static native String loginStart(String config);
    /** 로그인 2단계: 승인될 때까지 대기 (백그라운드 스레드에서 호출) */
    public static native String loginWait(String config, String dataDir);
    public static native void loginCancel();
    public static native boolean isLoggedIn(String config, String dataDir);
    public static native void logout(String config, String dataDir);
    public static native String resetState(String config, String dataDir);
}
