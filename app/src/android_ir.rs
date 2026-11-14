//! The phone's IR blaster, through Android's `ConsumerIrManager` (JNI).

use jni::objects::{JObject, JValue};
use jni::JavaVM;

use omacontrol_core::ir::{IrTransmitter, Pattern};
use omacontrol_core::{Error, Result};

pub(crate) struct AndroidIr;

fn jerr(e: impl std::fmt::Display) -> Error {
    Error::Protocol(format!("IR blaster: {e}"))
}

/// Runs `f` with the JNI env and the `ConsumerIrManager` (`None` when the phone has none).
fn with_manager<T>(f: impl FnOnce(&mut jni::JNIEnv, Option<&JObject>) -> Result<T>) -> Result<T> {
    let ctx = ndk_context::android_context();
    let vm = unsafe { JavaVM::from_raw(ctx.vm().cast()) }.map_err(jerr)?;
    let mut env = vm.attach_current_thread().map_err(jerr)?;
    let context = unsafe { JObject::from_raw(ctx.context().cast()) };
    let name = env.new_string("consumer_ir").map_err(jerr)?;
    let manager = env
        .call_method(&context, "getSystemService", "(Ljava/lang/String;)Ljava/lang/Object;", &[JValue::Object(&name)])
        .and_then(|v| v.l())
        .map_err(jerr)?;
    if manager.is_null() {
        f(&mut env, None)
    } else {
        f(&mut env, Some(&manager))
    }
}

impl IrTransmitter for AndroidIr {
    fn has_emitter(&self) -> bool {
        with_manager(|env, manager| match manager {
            Some(m) => env.call_method(m, "hasIrEmitter", "()Z", &[]).and_then(|v| v.z()).map_err(jerr),
            None => Ok(false),
        })
        .unwrap_or(false)
    }

    fn transmit(&self, pattern: &Pattern) -> Result<()> {
        with_manager(|env, manager| {
            let m = manager.ok_or_else(|| Error::Unsupported("IR (this phone has no IR blaster)".into()))?;
            let pulses: Vec<i32> = pattern.pulses.iter().map(|&p| p as i32).collect();
            let array = env.new_int_array(pulses.len() as i32).map_err(jerr)?;
            env.set_int_array_region(&array, 0, &pulses).map_err(jerr)?;
            env.call_method(m, "transmit", "(I[I)V", &[JValue::Int(pattern.carrier as i32), JValue::Object(&array)])
                .map_err(jerr)?;
            Ok(())
        })
    }
}
