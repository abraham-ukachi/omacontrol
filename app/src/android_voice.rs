//! Android microphone capture for Android TV voice search (8 kHz, 16-bit mono PCM).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, OnceLock};
use std::thread;

use jni::objects::{JObject, JValue};
use jni::{JNIEnv, JavaVM};

use super::{AppWindow, Job};

static VM: OnceLock<JavaVM> = OnceLock::new();

pub(crate) enum StartError {
    Permission,
    Other(String),
}

fn jerr(error: impl std::fmt::Display) -> String {
    format!("microphone: {error}")
}

fn clear_exception(env: &mut JNIEnv) {
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
    }
}

fn vm() -> Result<&'static JavaVM, String> {
    if let Some(vm) = VM.get() {
        return Ok(vm);
    }
    let context = ndk_context::android_context();
    let vm = unsafe { JavaVM::from_raw(context.vm().cast()) }.map_err(jerr)?;
    Ok(VM.get_or_init(|| vm))
}

fn request_permission(env: &mut JNIEnv, context: &JObject) -> Result<bool, String> {
    let permission = env
        .new_string("android.permission.RECORD_AUDIO")
        .map_err(jerr)?;
    let result = env
        .call_method(
            context,
            "checkSelfPermission",
            "(Ljava/lang/String;)I",
            &[JValue::Object(&permission)],
        )
        .and_then(|value| value.i())
        .map_err(jerr)?;
    if result == 0 {
        return Ok(true);
    }

    let permissions = env
        .new_object_array(1, "java/lang/String", JObject::null())
        .map_err(jerr)?;
    env.set_object_array_element(&permissions, 0, &permission)
        .map_err(jerr)?;
    if let Err(error) = env.call_method(
        context,
        "requestPermissions",
        "([Ljava/lang/String;I)V",
        &[JValue::Object(&permissions), JValue::Int(4381)],
    ) {
        clear_exception(env);
        return Err(jerr(error));
    }
    Ok(false)
}

/// Whether the app may use the microphone; asks for it (and answers false) when not yet.
pub(crate) fn microphone_allowed() -> Result<bool, String> {
    let vm = vm()?;
    let mut env = vm.attach_current_thread().map_err(jerr)?;
    let context = unsafe { JObject::from_raw(ndk_context::android_context().context().cast()) };
    let allowed = request_permission(&mut env, &context);
    clear_exception(&mut env);
    allowed
}

fn record(stop: &AtomicBool, tx: &Sender<Job>) -> Result<(), String> {
    let vm = vm()?;
    let mut env = vm.attach_current_thread().map_err(jerr)?;
    let result = (|| -> Result<(), String> {
        let context = unsafe { JObject::from_raw(ndk_context::android_context().context().cast()) };
        if !request_permission(&mut env, &context)? {
            return Err("permission".into());
        }

        let recorder_class = env.find_class("android/media/AudioRecord").map_err(jerr)?;
        let recorder = env
            .new_object(
                &recorder_class,
                "(IIIII)V",
                &[
                    JValue::Int(6),
                    JValue::Int(8000),
                    JValue::Int(16),
                    JValue::Int(2),
                    JValue::Int(32000),
                ],
            )
            .map_err(jerr)?;
        if env
            .call_method(&recorder, "getState", "()I", &[])
            .and_then(|value| value.i())
            .map_err(jerr)?
            != 1
        {
            let _ = env.call_method(&recorder, "release", "()V", &[]);
            return Err("the microphone could not be initialized".into());
        }
        env.call_method(&recorder, "startRecording", "()V", &[])
            .map_err(jerr)?;
        tx.send(Job::VoiceStart)
            .map_err(|error| error.to_string())?;

        let buffer = env.new_byte_array(8192).map_err(jerr)?;
        let mut samples = vec![0i8; 8192];
        let mut capture_result = Ok(());
        // Voice queries are short: stop by itself after 12 s if nobody taps again.
        let started = std::time::Instant::now();
        while !stop.load(Ordering::Relaxed) && started.elapsed() < std::time::Duration::from_secs(12) {
            let count = match env
                .call_method(
                    &recorder,
                    "read",
                    "([BII)I",
                    &[JValue::Object(&buffer), JValue::Int(0), JValue::Int(8192)],
                )
                .and_then(|value| value.i())
            {
                Ok(count) if count > 0 => count as usize,
                Ok(_) => continue,
                Err(error) => {
                    capture_result = Err(jerr(error));
                    break;
                }
            };
            if let Err(error) = env.get_byte_array_region(&buffer, 0, &mut samples[..count]) {
                capture_result = Err(jerr(error));
                break;
            }
            let chunk: Vec<u8> = samples[..count]
                .iter()
                .map(|sample| *sample as u8)
                .collect();
            if tx.send(Job::VoiceAudio(chunk)).is_err() {
                capture_result = Err("voice session ended".into());
                break;
            }
        }
        let _ = env.call_method(&recorder, "stop", "()V", &[]);
        let _ = env.call_method(&recorder, "release", "()V", &[]);
        capture_result
    })();
    clear_exception(&mut env);
    result
}

pub(crate) fn start(
    tx: Sender<Job>,
    weak: slint::Weak<AppWindow>,
) -> Result<Arc<AtomicBool>, StartError> {
    let vm = vm().map_err(StartError::Other)?;
    let mut env = vm
        .attach_current_thread()
        .map_err(|error| StartError::Other(jerr(error)))?;
    let context = unsafe { JObject::from_raw(ndk_context::android_context().context().cast()) };
    let permission = request_permission(&mut env, &context);
    clear_exception(&mut env);
    match permission {
        Ok(true) => {}
        Ok(false) => return Err(StartError::Permission),
        Err(error) => return Err(StartError::Other(error)),
    }

    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = stop.clone();
    thread::spawn(move || {
        if let Err(error) = record(&thread_stop, &tx) {
            if error != "permission" {
                let _ = tx.send(Job::VoiceError(error));
            } else {
                // This thread isn't the UI's: the status goes through its event loop.
                crate::voice_report(&weak, crate::VoiceSource::Stream, "permission", String::new(), false);
            }
        }
        let _ = tx.send(Job::VoiceStop);
    });
    Ok(stop)
}
