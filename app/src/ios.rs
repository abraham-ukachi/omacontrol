//! iOS glue through UIKit / Foundation: a light haptic tap, opening links, the phone's language
//! and where the app keeps its data. Everything here runs on the main (UI) thread.

use std::cell::RefCell;
use std::path::PathBuf;

use block2::Block;
use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyObject, Bool};
use objc2::{class, msg_send};
use objc2_foundation::{NSDictionary, NSLocale, NSString, NSURL};

thread_local! {
    static IMPACT: RefCell<Option<Retained<AnyObject>>> = const { RefCell::new(None) };
}

/// A light `UIImpactFeedbackGenerator` tap (silent on devices without a Taptic Engine).
pub(crate) fn tick() {
    IMPACT.with(|cell| {
        let mut cell = cell.borrow_mut();
        if cell.is_none() {
            // UIImpactFeedbackStyleLight = 0
            let alloc: Allocated<AnyObject> = unsafe { msg_send![class!(UIImpactFeedbackGenerator), alloc] };
            *cell = unsafe { msg_send![alloc, initWithStyle: 0isize] };
        }
        if let Some(generator) = cell.as_ref() {
            let _: () = unsafe { msg_send![&**generator, impactOccurred] };
        }
    });
}

/// Opens a link in the app that handles it (Safari, Mail, X).
pub(crate) fn open_url(url: &str) {
    let Some(url) = NSURL::URLWithString(&NSString::from_str(url)) else { return };
    let options = NSDictionary::<AnyObject, AnyObject>::new();
    let done: Option<&Block<dyn Fn(Bool)>> = None;
    unsafe {
        let app: *mut AnyObject = msg_send![class!(UIApplication), sharedApplication];
        if app.is_null() {
            return;
        }
        let _: () = msg_send![app, openURL: &*url, options: &*options, completionHandler: done];
    }
}

/// The phone's first preferred language, e.g. "fr-FR".
pub(crate) fn language_tag() -> Option<String> {
    NSLocale::preferredLanguages().firstObject().map(|tag| tag.to_string())
}

/// `~/Library/Application Support/omacontrol` inside the app's sandbox (backed up, never shown in Files).
pub(crate) fn data_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("Library")
        .join("Application Support")
        .join("omacontrol")
}
