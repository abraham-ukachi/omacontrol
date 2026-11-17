package io.github.abraham_ukachi.omacontrol;

import android.content.Context;
import android.content.Intent;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.speech.RecognitionListener;
import android.speech.RecognizerIntent;
import android.speech.SpeechRecognizer;
import java.util.ArrayList;

/**
 * The phone's own speech recognizer, for TVs that don't take the phone's microphone.
 *
 * Built into app/android/speech.dex by scripts/build_speech_dex.sh and loaded at run time with an
 * InMemoryDexClassLoader (the app is a NativeActivity, there is no Java build). SpeechRecognizer
 * must live on the main Looper, so every call is posted there; events go back to Rust through
 * the native {@link #event} method.
 */
public final class Speech implements RecognitionListener {
    static final int READY = 1;
    static final int BEGIN = 2;
    static final int END = 3;
    static final int PARTIAL = 4;
    static final int RESULT = 5;
    static final int ERROR = 6;
    static final int UNAVAILABLE = 7;

    private static native void event(int kind, String text);

    private static final Handler MAIN = new Handler(Looper.getMainLooper());
    private static SpeechRecognizer recognizer;
    private static Speech current;

    private Speech() {}

    public static boolean available(Context context) {
        return SpeechRecognizer.isRecognitionAvailable(context);
    }

    public static void start(final Context context, final String language) {
        MAIN.post(new Runnable() {
            @Override
            public void run() {
                release();
                if (!SpeechRecognizer.isRecognitionAvailable(context)) {
                    event(UNAVAILABLE, "");
                    return;
                }
                try {
                    current = new Speech();
                    recognizer = SpeechRecognizer.createSpeechRecognizer(context);
                    recognizer.setRecognitionListener(current);
                    Intent intent = new Intent(RecognizerIntent.ACTION_RECOGNIZE_SPEECH);
                    intent.putExtra(RecognizerIntent.EXTRA_LANGUAGE_MODEL, RecognizerIntent.LANGUAGE_MODEL_FREE_FORM);
                    intent.putExtra(RecognizerIntent.EXTRA_PARTIAL_RESULTS, true);
                    intent.putExtra(RecognizerIntent.EXTRA_MAX_RESULTS, 1);
                    intent.putExtra(RecognizerIntent.EXTRA_CALLING_PACKAGE, context.getPackageName());
                    intent.putExtra(RecognizerIntent.EXTRA_SPEECH_INPUT_COMPLETE_SILENCE_LENGTH_MILLIS, 2500L);
                    if (language != null && !language.isEmpty()) {
                        intent.putExtra(RecognizerIntent.EXTRA_LANGUAGE, language);
                    }
                    recognizer.startListening(intent);
                } catch (Throwable t) {
                    release();
                    event(ERROR, String.valueOf(t.getMessage()));
                }
            }
        });
    }

    /** Stops listening; the result still comes. */
    public static void stop() {
        MAIN.post(new Runnable() {
            @Override
            public void run() {
                if (recognizer != null) {
                    try {
                        recognizer.stopListening();
                    } catch (Throwable ignored) {
                    }
                }
            }
        });
    }

    /** Stops listening and drops the result. */
    public static void cancel() {
        MAIN.post(new Runnable() {
            @Override
            public void run() {
                release();
            }
        });
    }

    private static void release() {
        current = null;
        if (recognizer != null) {
            try {
                recognizer.cancel();
                recognizer.destroy();
            } catch (Throwable ignored) {
            }
            recognizer = null;
        }
    }

    private boolean stale() {
        return current != this;
    }

    private static String best(Bundle results) {
        if (results == null) {
            return "";
        }
        ArrayList<String> texts = results.getStringArrayList(SpeechRecognizer.RESULTS_RECOGNITION);
        return texts == null || texts.isEmpty() || texts.get(0) == null ? "" : texts.get(0);
    }

    @Override
    public void onReadyForSpeech(Bundle params) {
        if (!stale()) event(READY, "");
    }

    @Override
    public void onBeginningOfSpeech() {
        if (!stale()) event(BEGIN, "");
    }

    @Override
    public void onRmsChanged(float rmsdB) {}

    @Override
    public void onBufferReceived(byte[] buffer) {}

    @Override
    public void onEndOfSpeech() {
        if (!stale()) event(END, "");
    }

    @Override
    public void onError(int error) {
        if (stale()) return;
        release();
        event(ERROR, Integer.toString(error));
    }

    @Override
    public void onResults(Bundle results) {
        if (stale()) return;
        String text = best(results);
        release();
        event(RESULT, text);
    }

    @Override
    public void onPartialResults(Bundle partialResults) {
        if (stale()) return;
        String text = best(partialResults);
        if (!text.isEmpty()) event(PARTIAL, text);
    }

    @Override
    public void onEvent(int eventType, Bundle params) {}
}
