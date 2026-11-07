package io.bluewind.obsidiansync;

/**
 * Rust 동기화 엔진(JNI). 모든 결과는 JSON 문자열입니다.
 * config = 앱 전체 설정 JSON ({"profiles":[...], ...}), backend = 프로필의 "backend" JSON.
 */
public final class Native {
    static { System.loadLibrary("obsidian_sync"); }
    private Native() {}

    /** 켜진 프로필 전체의 실시간 동기화 시작 (파일 감시 + 주기 확인). {"ok":bool,"error"?} */
    public static native String start(String config, String dataDir);
    public static native void stop();
    public static native boolean syncNow();
    /** {"running","syncing","last_sync","last_summary","last_error","logs":[...],"profiles":[{id,name,active,syncing,last_sync,last_summary,last_error}]} */
    public static native String status();
    /** 켜진 프로필을 한 번씩 동기화. {"ok","summary"?,"error"?} */
    public static native String syncOnce(String config, String dataDir);
    /** 저장소 연결 테스트. {"ok","count"?,"error"?,"need_otp"?} */
    public static native String testConnection(String backend, String dataDir);
    public static native String validate(String config);
    /** 설정을 현재 형식으로 정리 (이전 버전 자동 변환). {"ok","config"} */
    public static native String normalize(String config);
    public static native String newProfileId();
    /** Synology 로그인. otp가 있으면 함께 보내고 이 기기를 기억. {"ok","error"?,"need_otp"?} */
    public static native String synologyLogin(String backend, String dataDir, String otp);
    public static native boolean synologyRemembered(String backend, String dataDir);
    public static native void synologyForget(String backend, String dataDir);
    /** OneDrive/Google Drive 로그인 1단계. {"ok","user_code","verification_uri"} */
    public static native String loginStart(String backend);
    /** 로그인 2단계: 승인될 때까지 대기 (백그라운드 스레드에서 호출) */
    public static native String loginWait(String backend, String dataDir);
    public static native void loginCancel();
    public static native boolean isLoggedIn(String backend, String dataDir);
    public static native void logout(String backend, String dataDir);
    public static native String resetState(String config, String dataDir, String profileId);
}
