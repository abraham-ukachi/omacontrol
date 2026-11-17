fn main() {
    let config = slint_build::CompilerConfiguration::new()
        .with_bundled_translations("translations")
        .with_default_translation_context(slint_build::DefaultTranslationContext::None);
    slint_build::compile_with_config("ui/app.slint", config).expect("the Slint UI compiles");
    if std::env::var("TARGET").is_ok_and(|t| t.contains("android")) {
        speech_dex();
    }
}

/// The phone's speech recognizer needs a Java listener: compile `Speech.java` into a dex the
/// app loads at run time (the same way Slint's Android backend ships its Java helper).
fn speech_dex() {
    use android_build::{Dexer, JavaBuild};
    use std::path::PathBuf;

    let src = "android/java/io/github/abraham_ukachi/omacontrol/Speech.java";
    println!("cargo:rerun-if-changed={src}");
    let out_dir: PathBuf = std::env::var_os("OUT_DIR").expect("OUT_DIR").into();
    let classes = out_dir.join("speech-classes");
    let _ = std::fs::remove_dir_all(&classes);
    std::fs::create_dir_all(&classes).expect("a classes directory");
    let android_jar = android_build::android_jar(None).expect("an Android platform (set ANDROID_HOME)");
    let javac = JavaBuild::new()
        .file(src)
        .class_path(&android_jar)
        .classes_out_dir(&classes)
        .java_source_version(8)
        .java_target_version(8)
        .command()
        .expect("a Java compiler command")
        .args(["-encoding", "UTF-8"])
        .output()
        .expect("javac runs");
    assert!(javac.status.success(), "Speech.java: {}", String::from_utf8_lossy(&javac.stderr));
    let dex_dir = out_dir.join("speech-dex");
    std::fs::create_dir_all(&dex_dir).expect("a dex directory");
    let d8 = Dexer::new()
        .android_jar(&android_jar)
        .class_path(&classes)
        .collect_classes(&classes)
        .expect("the compiled classes")
        .release(true)
        .android_min_api(26)
        .out_dir(&dex_dir)
        .command()
        .expect("a D8 command")
        .output()
        .expect("d8 runs");
    assert!(d8.status.success(), "dexing Speech.java: {}", String::from_utf8_lossy(&d8.stderr));
}
