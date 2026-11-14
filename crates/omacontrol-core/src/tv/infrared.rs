//! A TV driven by the phone's IR blaster.

use std::sync::Arc;

use super::{Brand, Remote};
use crate::ir::{tv_code, IrTransmitter};
use crate::{App, Error, Key, Result};

pub(crate) struct Infrared {
    brand: Brand,
    blaster: Arc<dyn IrTransmitter>,
}

impl Infrared {
    pub(crate) fn new(brand: Brand, blaster: Arc<dyn IrTransmitter>) -> Infrared {
        Infrared { brand, blaster }
    }
}

impl Remote for Infrared {
    fn press(&mut self, key: Key) -> Result<()> {
        let pattern = tv_code(self.brand, key).ok_or_else(|| Error::Unsupported(format!("{key:?} over IR")))?;
        self.blaster.transmit(&pattern)
    }

    fn type_text(&mut self, _text: &str) -> Result<()> {
        Err(Error::Unsupported("typing over IR".into()))
    }

    fn launch(&mut self, app: App) -> Result<()> {
        Err(Error::Unsupported(format!("{} over IR", app.label())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::Pattern;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Recorder(Mutex<Vec<Pattern>>);
    impl IrTransmitter for Recorder {
        fn has_emitter(&self) -> bool {
            true
        }
        fn transmit(&self, p: &Pattern) -> Result<()> {
            self.0.lock().unwrap().push(p.clone());
            Ok(())
        }
    }

    #[test]
    fn presses_go_through_the_blaster() {
        let rec = Arc::new(Recorder::default());
        let mut tv = Infrared::new(Brand::Lg, rec.clone());
        tv.press(Key::VolumeUp).unwrap();
        assert_eq!(rec.0.lock().unwrap().len(), 1);
        assert!(tv.type_text("hi").is_err());
    }
}
