//! The phone's own speech recognizer (`android.speech.SpeechRecognizer`), for TVs that won't
//! take the phone's microphone: the phone turns speech into text and the text goes to the TV.
//!
//! A `RecognitionListener` has to be a Java class, so `android/java/…/Speech.java` is compiled
//! into a dex by `build.rs` and loaded here with an `InMemoryDexClassLoader`. The Java side posts
//! everything to the main Looper (SpeechRecognizer lives there) and calls back [`event`].

use std::ffi::c_void;
use std::sync::{Mutex, OnceLock};

use jni::objects::{GlobalRef, JClass, JObject, JString, JValue};
use jni::sys::jint;
use jni::{JNIEnv, JavaVM, NativeMethod};

static DEX: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/speech-dex/classes.dex"));
static CLASS: OnceLock<GlobalRef> = OnceLock::new();

/// What the recognizer reports, as the Java side numbers it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Event {
    Ready,
    Begin,
    End,
    Partial(String),
    Result(String),
    /// A `SpeechRecognizer.ERROR_*` code (or a message when starting failed).
    Error(String),
    Unavailable,
}

type Sink = Box<dyn Fn(Event) + Send>;
static SINK: Mutex<Option<Sink>> = Mutex::new(None);

/// Where events go (the app forwards them to the UI thread).
pub(crate) fn set_sink(sink: impl Fn(Event) + Send + 'static) {
    *SINK.lock().unwrap() = Some(Box::new(sink));
}

extern "system" fn event(mut env: JNIEnv, _class: JClass, kind: jint, text: JString) {
    let text: String = if text.is_null() {
        String::new()
    } else {
        env.get_string(&text).map(Into::into).unwrap_or_default()
    };
    let event = match kind {
        1 => Event::Ready,
        2 => Event::Begin,
        3 => Event::End,
        4 => Event::Partial(text),
        5 => Event::Result(text),
        7 => Event::Unavailable,
        _ => Event::Error(text),
    };
    if let Some(sink) = SINK.lock().unwrap().as_ref() {
        sink(event);
    }
}

fn jerr(error: impl std::fmt::Display) -> String {
    format!("speech: {error}")
}

fn vm() -> Result<JavaVM, String> {
    unsafe { JavaVM::from_raw(ndk_context::android_context().vm().cast()) }.map_err(jerr)
}

fn context<'a>() -> JObject<'a> {
    unsafe { JObject::from_raw(ndk_context::android_context().context().cast()) }
}

fn clear_exception(env: &mut JNIEnv) {
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_describe();
        let _ = env.exception_clear();
    }
}

/// Loads `Speech` from the embedded dex once, and registers its native callback.
fn class(env: &mut JNIEnv) -> Result<&'static GlobalRef, String> {
    if let Some(class) = CLASS.get() {
        return Ok(class);
    }
    let result = (|| -> jni::errors::Result<GlobalRef> {
        // The JVM only reads the buffer, and DEX lives as long as the process.
        let buffer = unsafe { env.new_direct_byte_buffer(DEX.as_ptr() as *mut u8, DEX.len()) }?;
        let parent = env
            .call_method(context(), "getClassLoader", "()Ljava/lang/ClassLoader;", &[])?
            .l()?;
        let loader = env.new_object(
            "dalvik/system/InMemoryDexClassLoader",
            "(Ljava/nio/ByteBuffer;Ljava/lang/ClassLoader;)V",
            &[JValue::Object(&buffer), JValue::Object(&parent)],
        )?;
        let name = env.new_string("io.github.abraham_ukachi.omacontrol.Speech")?;
        let class = env
            .call_method(
                &loader,
                "loadClass",
                "(Ljava/lang/String;)Ljava/lang/Class;",
                &[JValue::Object(&name)],
            )?
            .l()?;
        let class = JClass::from(class);
        env.register_native_methods(
            &class,
            &[NativeMethod {
                name: "event".into(),
                sig: "(ILjava/lang/String;)V".into(),
                fn_ptr: event as *mut c_void,
            }],
        )?;
        env.new_global_ref(class)
    })();
    match result {
        Ok(global) => Ok(CLASS.get_or_init(|| global)),
        Err(error) => {
            clear_exception(env);
            Err(jerr(error))
        }
    }
}

fn call(method: &str, sig: &str, args: &[JValue]) -> Result<(), String> {
    let vm = vm()?;
    let mut env = vm.attach_current_thread().map_err(jerr)?;
    let class = class(&mut env)?;
    let class: &JClass = class.as_obj().into();
    let result = env.call_static_method(class, method, sig, args);
    if result.is_err() {
        clear_exception(&mut env);
    }
    result.map(|_| ()).map_err(jerr)
}

/// Starts listening (the microphone permission must be granted already). `language` is a
/// BCP 47 tag, or empty for the phone's own language.
pub(crate) fn start(language: &str) -> Result<(), String> {
    let vm = vm()?;
    let mut jenv = vm.attach_current_thread().map_err(jerr)?;
    let class = class(&mut jenv)?;
    let result = (|| {
        let language = jenv.new_string(language)?;
        let class: &JClass = class.as_obj().into();
        jenv.call_static_method(
            class,
            "start",
            "(Landroid/content/Context;Ljava/lang/String;)V",
            &[JValue::Object(&context()), JValue::Object(&language)],
        )
        .map(|_| ())
    })();
    if result.is_err() {
        clear_exception(&mut jenv);
    }
    result.map_err(jerr)
}

/// Stops listening; the result still arrives as [`Event::Result`].
pub(crate) fn stop() {
    let _ = call("stop", "()V", &[]);
}

/// Stops listening and drops the result.
pub(crate) fn cancel() {
    let _ = call("cancel", "()V", &[]);
}
