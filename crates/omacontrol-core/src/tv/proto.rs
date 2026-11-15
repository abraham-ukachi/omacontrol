//! Just enough protobuf for the Android TV Remote protocol v2: varints, nested messages,
//! strings and bytes. The two `.proto` files (polo and remotemessage) are small enough that
//! a hand-written encoder beats a code generator and a build step.

/// A protobuf message being built, field by field.
#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct Msg(pub Vec<u8>);

impl Msg {
    pub fn new() -> Msg {
        Msg(Vec::new())
    }

    /// A varint field (int32, uint32, enum, bool).
    pub fn int(mut self, field: u32, value: u64) -> Msg {
        varint(&mut self.0, u64::from(field) << 3);
        varint(&mut self.0, value);
        self
    }

    /// A length-delimited field (string, bytes, nested message).
    pub fn bytes(mut self, field: u32, value: &[u8]) -> Msg {
        varint(&mut self.0, (u64::from(field) << 3) | 2);
        varint(&mut self.0, value.len() as u64);
        self.0.extend_from_slice(value);
        self
    }

    pub fn str(self, field: u32, value: &str) -> Msg {
        self.bytes(field, value.as_bytes())
    }

    pub fn msg(self, field: u32, value: Msg) -> Msg {
        self.bytes(field, &value.0)
    }

    /// The message with its varint length in front, as both ports expect it on the wire.
    pub fn framed(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.0.len() + 2);
        varint(&mut out, self.0.len() as u64);
        out.extend_from_slice(&self.0);
        out
    }
}

pub(crate) fn varint(out: &mut Vec<u8>, mut v: u64) {
    loop {
        let byte = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

/// Reads a varint at `pos`; `None` when the buffer ends first.
pub(crate) fn read_varint(buf: &[u8], pos: &mut usize) -> Option<u64> {
    let mut v = 0u64;
    for shift in (0..64).step_by(7) {
        let b = *buf.get(*pos)?;
        *pos += 1;
        v |= u64::from(b & 0x7f) << shift;
        if b & 0x80 == 0 {
            return Some(v);
        }
    }
    None
}

/// One field of a decoded message.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Value<'a> {
    Int(u64),
    Bytes(&'a [u8]),
}

/// The fields of a message, in order. Fixed-width fields (never used here) end the parse.
pub(crate) fn fields(buf: &[u8]) -> Vec<(u32, Value<'_>)> {
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < buf.len() {
        let Some(tag) = read_varint(buf, &mut pos) else { break };
        let field = (tag >> 3) as u32;
        match tag & 7 {
            0 => match read_varint(buf, &mut pos) {
                Some(v) => out.push((field, Value::Int(v))),
                None => break,
            },
            2 => {
                let Some(len) = read_varint(buf, &mut pos) else { break };
                let end = pos + len as usize;
                if end > buf.len() {
                    break;
                }
                out.push((field, Value::Bytes(&buf[pos..end])));
                pos = end;
            }
            _ => break,
        }
    }
    out
}

/// The first varint `field` of a message.
pub(crate) fn int(buf: &[u8], field: u32) -> Option<u64> {
    fields(buf).into_iter().find_map(|(f, v)| match v {
        Value::Int(i) if f == field => Some(i),
        _ => None,
    })
}

/// The first length-delimited `field` of a message (a nested message, a string, bytes).
pub(crate) fn sub(buf: &[u8], field: u32) -> Option<&[u8]> {
    fields(buf).into_iter().find_map(|(f, v)| match v {
        Value::Bytes(b) if f == field => Some(b),
        _ => None,
    })
}

/// Takes every complete framed message off the front of `buf`, leaving a partial one behind.
pub(crate) fn drain_frames(buf: &mut Vec<u8>) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    while let Some(msg) = drain_frames_one(buf) {
        out.push(msg);
    }
    out
}

/// Takes the first complete framed message off the front of `buf`, if there is one.
pub(crate) fn drain_frames_one(buf: &mut Vec<u8>) -> Option<Vec<u8>> {
    let mut pos = 0;
    let len = read_varint(buf, &mut pos)?;
    let end = pos + len as usize;
    if end > buf.len() {
        return None;
    }
    let msg = buf[pos..end].to_vec();
    buf.drain(..end);
    Some(msg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_like_protobuf() {
        // OuterMessage { protocol_version: 2, status: STATUS_OK (200) }
        let m = Msg::new().int(1, 2).int(2, 200);
        assert_eq!(m.0, vec![0x08, 0x02, 0x10, 0xC8, 0x01]);
        assert_eq!(m.framed(), vec![0x05, 0x08, 0x02, 0x10, 0xC8, 0x01]);
        // RemoteKeyInject { key_code: 19, direction: SHORT } inside RemoteMessage field 10.
        let k = Msg::new().msg(10, Msg::new().int(1, 19).int(2, 3));
        assert_eq!(k.0, vec![0x52, 0x04, 0x08, 0x13, 0x10, 0x03]);
    }

    #[test]
    fn decodes_what_it_encodes() {
        let m = Msg::new().int(1, 622).msg(2, Msg::new().str(1, "Freebox").int(3, 1)).str(5, "atvremote");
        assert_eq!(int(&m.0, 1), Some(622));
        let info = sub(&m.0, 2).unwrap();
        assert_eq!(sub(info, 1), Some(&b"Freebox"[..]));
        assert_eq!(int(info, 3), Some(1));
        assert_eq!(sub(&m.0, 5), Some(&b"atvremote"[..]));
    }

    #[test]
    fn drains_whole_frames_only() {
        let a = Msg::new().int(1, 2).framed();
        let b = Msg::new().str(1, "hello").framed();
        let mut buf = [a.clone(), b[..3].to_vec()].concat();
        assert_eq!(drain_frames(&mut buf), vec![a[1..].to_vec()]);
        buf.extend_from_slice(&b[3..]);
        assert_eq!(drain_frames(&mut buf), vec![b[1..].to_vec()]);
        assert!(buf.is_empty());
    }
}
