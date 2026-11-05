use jni::{
    objects::{JClass, JString},
    sys::jstring,
    JNIEnv,
};

#[no_mangle]
pub extern "system" fn Java_io_bluewind_obsidiansync_Native_sync(
    mut env: JNIEnv,
    _class: JClass,
    config: JString,
    state: JString,
) -> jstring {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let config: String = env.get_string(&config)?.into();
        let state: String = env.get_string(&state)?.into();
        Ok::<_, jni::errors::Error>(vault_sync_core::run_json(
            &config,
            std::path::Path::new(&state),
        ))
    }));
    let text = match result {
        Ok(Ok(text)) => text,
        _ => "{\"ok\":false,\"error\":\"Native sync failed safely\"}".into(),
    };
    env.new_string(text)
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
}
