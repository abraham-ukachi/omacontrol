//! A faint haptic tick (Android's `EFFECT_TICK`), played on every key press.

use std::sync::OnceLock;

use jni::objects::{GlobalRef, JObject, JValue};
use jni::{JNIEnv, JavaVM};

struct Haptics {
    vm: JavaVM,
    vibrator: GlobalRef,
    effect: GlobalRef,
}

static HAPTICS: OnceLock<Option<Haptics>> = OnceLock::new();

fn clear(env: &mut JNIEnv) {
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
    }
}

fn init() -> Option<Haptics> {
    let ctx = ndk_context::android_context();
    let vm = unsafe { JavaVM::from_raw(ctx.vm().cast()) }.ok()?;
    let mut env = vm.attach_current_thread_permanently().ok()?;
    let found = (|| -> jni::errors::Result<Option<(GlobalRef, GlobalRef)>> {
        let context = unsafe { JObject::from_raw(ctx.context().cast()) };
        let name = env.new_string("vibrator")?;
        let vibrator = env
            .call_method(&context, "getSystemService", "(Ljava/lang/String;)Ljava/lang/Object;", &[JValue::Object(&name)])?
            .l()?;
        if vibrator.is_null() || !env.call_method(&vibrator, "hasVibrator", "()Z", &[])?.z()? {
            return Ok(None);
        }
        let sdk = env.get_static_field("android/os/Build$VERSION", "SDK_INT", "I")?.i()?;
        let effect = if sdk >= 29 {
            // VibrationEffect.EFFECT_TICK: the lightest built-in click.
            env.call_static_method("android/os/VibrationEffect", "createPredefined", "(I)Landroid/os/VibrationEffect;", &[JValue::Int(2)])?
        } else {
            env.call_static_method("android/os/VibrationEffect", "createOneShot", "(JI)Landroid/os/VibrationEffect;", &[JValue::Long(8), JValue::Int(40)])?
        }
        .l()?;
        Ok(Some((env.new_global_ref(vibrator)?, env.new_global_ref(effect)?)))
    })();
    clear(&mut env);
    let (vibrator, effect) = found.ok()??;
    Some(Haptics { vm, vibrator, effect })
}

/// Plays the tick. Cheap after the first call (everything is cached), and silent when the phone can't vibrate.
pub(crate) fn tick() {
    let Some(h) = HAPTICS.get_or_init(init) else { return };
    let Ok(mut env) = h.vm.attach_current_thread_permanently() else { return };
    let _ = env.call_method(h.vibrator.as_obj(), "vibrate", "(Landroid/os/VibrationEffect;)V", &[JValue::Object(h.effect.as_obj())]);
    clear(&mut env);
}
