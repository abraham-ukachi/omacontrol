//! Opens a link (GitHub, X, mail) with Android's `Intent.ACTION_VIEW`.

use jni::objects::{JObject, JValue};
use jni::JavaVM;

/// Opens `url` in the app that handles it (the browser, the X app, the mail app).
pub(crate) fn open(url: &str) -> bool {
    let ctx = ndk_context::android_context();
    let Ok(vm) = (unsafe { JavaVM::from_raw(ctx.vm().cast()) }) else { return false };
    let Ok(mut env) = vm.attach_current_thread_permanently() else { return false };
    let result = (|| -> jni::errors::Result<()> {
        let context = unsafe { JObject::from_raw(ctx.context().cast()) };
        let jurl = env.new_string(url)?;
        let uri = env.call_static_method("android/net/Uri", "parse", "(Ljava/lang/String;)Landroid/net/Uri;", &[JValue::Object(&jurl)])?.l()?;
        let action = env.new_string("android.intent.action.VIEW")?;
        let intent = env.new_object("android/content/Intent", "(Ljava/lang/String;Landroid/net/Uri;)V", &[JValue::Object(&action), JValue::Object(&uri)])?;
        // FLAG_ACTIVITY_NEW_TASK: we start it from the NativeActivity's context.
        env.call_method(&intent, "addFlags", "(I)Landroid/content/Intent;", &[JValue::Int(0x1000_0000)])?;
        env.call_method(&context, "startActivity", "(Landroid/content/Intent;)V", &[JValue::Object(&intent)])?;
        Ok(())
    })();
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
        return false;
    }
    result.is_ok()
}
