//! What the moved modules' host tests share: a call that must succeed, and
//! files in and out as the JSON carries them (base64).

use serde_json::{json, Value};

use super::Host;

pub(crate) fn call(h: &Host, c: &str, args: Value) -> Value {
    h.dispatch(c, args).unwrap_or_else(|e| panic!("{c}: {e}"))
}

/// A picked file as the page hands it in (`FileIn`).
pub(crate) fn file(name: &str, bytes: &[u8]) -> Value {
    json!({"name": name, "bytes": super::b64_encode(bytes)})
}

/// The bytes of a `FileOut` reply.
pub(crate) fn bytes_of(out: &Value) -> Vec<u8> {
    super::b64_decode(out["bytes"].as_str().expect("base64 bytes")).unwrap()
}
