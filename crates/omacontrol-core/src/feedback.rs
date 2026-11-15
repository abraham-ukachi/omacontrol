//! # Feedback 💌
//!
//! The Settings › Feedback form: an idea or an issue, sent straight to the maintainer's inbox
//! through [Web3Forms](https://web3forms.com) (no server of our own, no account for the user).
//!
//! The access key below is a **public** key by design (Web3Forms keys live in client code); all
//! it can do is deliver a message to one inbox, and Web3Forms rate-limits it.

use std::time::Duration;

use crate::{Error, Result};

const ENDPOINT: &str = "https://api.web3forms.com/submit";
const ACCESS_KEY: &str = "0ce028dc-4c3d-466d-93e9-2ca94ec5eeb0";

/// What the user is sending.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Feedback,
    Issue,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Feedback => "Feedback",
            Kind::Issue => "Issue",
        }
    }

    pub fn from_label(label: &str) -> Kind {
        if label.eq_ignore_ascii_case("issue") {
            Kind::Issue
        } else {
            Kind::Feedback
        }
    }
}

/// One message from the form.
#[derive(Debug, Clone, PartialEq)]
pub struct Feedback {
    pub kind: Kind,
    /// The sender's email, for a reply (optional).
    pub email: String,
    pub title: String,
    pub message: String,
    /// The app version and platform (`0.3.0 · Android`), so an issue says where it happened.
    pub version: String,
}

impl Feedback {
    /// Checks the form before anything leaves the phone.
    pub fn validate(&self) -> std::result::Result<(), &'static str> {
        let email = self.email.trim();
        if !email.is_empty() && !looks_like_email(email) {
            return Err("that email doesn't look right");
        }
        if self.title.trim().is_empty() {
            return Err("add a title");
        }
        if self.message.trim().len() < 5 {
            return Err("write a few more words");
        }
        Ok(())
    }

    /// The JSON body Web3Forms expects.
    pub(crate) fn body(&self) -> serde_json::Value {
        let mut body = serde_json::json!({
            "access_key": ACCESS_KEY,
            "subject": format!("[OmaControl] {}: {}", self.kind.label(), self.title.trim()),
            "from_name": "OmaControl",
            "kind": self.kind.label(),
            "title": self.title.trim(),
            "message": self.message.trim(),
            "version": self.version,
            "botcheck": false,
        });
        if !self.email.trim().is_empty() {
            body["email"] = self.email.trim().into();
        }
        body
    }

    /// Sends it. Blocks for the network round trip, so call it from the worker thread.
    pub fn send(&self) -> Result<()> {
        self.validate().map_err(|e| Error::Protocol(e.into()))?;
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(15))
            .user_agent(&format!("OmaControl/{} ({})", env!("CARGO_PKG_VERSION"), std::env::consts::OS))
            .build();
        let reply: serde_json::Value = match agent.post(ENDPOINT).set("Accept", "application/json").send_json(self.body()) {
            Ok(r) => r.into_json().map_err(Error::Io)?,
            Err(ureq::Error::Status(_, r)) => r.into_json().unwrap_or_default(),
            Err(e) => return Err(e.into()),
        };
        if reply["success"].as_bool() == Some(true) {
            Ok(())
        } else {
            let why = reply["message"].as_str().unwrap_or("the message didn't go through").to_string();
            Err(Error::Http(why))
        }
    }
}

fn looks_like_email(s: &str) -> bool {
    let Some((user, domain)) = s.split_once('@') else { return false };
    !user.is_empty() && domain.contains('.') && !domain.starts_with('.') && !domain.ends_with('.') && !s.contains(char::is_whitespace)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form() -> Feedback {
        Feedback { kind: Kind::Issue, email: "me@example.com".into(), title: "OK key".into(), message: "OK doesn't work on my TV".into(), version: "0.3.0 · Android".into() }
    }

    #[test]
    fn validates_the_form() {
        assert!(form().validate().is_ok());
        assert!(Feedback { email: String::new(), ..form() }.validate().is_ok());
        assert!(Feedback { email: "nope".into(), ..form() }.validate().is_err());
        assert!(Feedback { title: " ".into(), ..form() }.validate().is_err());
        assert!(Feedback { message: "hi".into(), ..form() }.validate().is_err());
    }

    #[test]
    fn builds_the_web3forms_body() {
        let b = form().body();
        assert_eq!(b["subject"], "[OmaControl] Issue: OK key");
        assert_eq!(b["email"], "me@example.com");
        assert_eq!(b["access_key"], ACCESS_KEY);
        let anonymous = Feedback { email: String::new(), ..form() }.body();
        assert!(anonymous.get("email").is_none());
    }

    #[test]
    fn kinds_round_trip() {
        assert_eq!(Kind::from_label("Issue"), Kind::Issue);
        assert_eq!(Kind::from_label("feedback"), Kind::Feedback);
    }
}
