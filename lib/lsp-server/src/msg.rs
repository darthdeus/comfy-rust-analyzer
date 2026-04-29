use std::{
    fmt,
    io::{self, BufRead, Write},
};

use serde::de::DeserializeOwned;

use crate::error::ExtractError;

#[derive(Debug, Clone)]
pub enum Message {
    Request(Request),
    Response(Response),
    Notification(Notification),
}

impl From<Request> for Message {
    fn from(request: Request) -> Message {
        Message::Request(request)
    }
}

impl From<Response> for Message {
    fn from(response: Response) -> Message {
        Message::Response(response)
    }
}

impl From<Notification> for Message {
    fn from(notification: Notification) -> Message {
        Message::Notification(notification)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RequestId(IdRepr);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum IdRepr {
    I32(i32),
    String(String),
}

impl From<i32> for RequestId {
    fn from(id: i32) -> RequestId {
        RequestId(IdRepr::I32(id))
    }
}

impl From<String> for RequestId {
    fn from(id: String) -> RequestId {
        RequestId(IdRepr::String(id))
    }
}

impl fmt::Display for RequestId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            IdRepr::I32(it) => fmt::Display::fmt(it, f),
            IdRepr::String(it) => fmt::Debug::fmt(it, f),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Request {
    pub id: RequestId,
    pub method: String,
    pub params: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct Response {
    // JSON-RPC allows this to be null if we can't find or parse the
    // request id. We fail deserialization in that case, so we just
    // make this field mandatory.
    pub id: RequestId,
    pub result: Option<serde_json::Value>,
    pub error: Option<ResponseError>,
}

#[derive(Debug, Clone)]
pub struct ResponseError {
    pub code: i32,
    pub message: String,
    pub data: Option<serde_json::Value>,
}

#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub enum ErrorCode {
    // Defined by JSON RPC:
    ParseError = -32700,
    InvalidRequest = -32600,
    MethodNotFound = -32601,
    InvalidParams = -32602,
    InternalError = -32603,
    ServerErrorStart = -32099,
    ServerErrorEnd = -32000,

    /// Error code indicating that a server received a notification or
    /// request before the server has received the `initialize` request.
    ServerNotInitialized = -32002,
    UnknownErrorCode = -32001,

    // Defined by the protocol:
    /// The client has canceled a request and a server has detected
    /// the cancel.
    RequestCanceled = -32800,

    /// The server detected that the content of a document got
    /// modified outside normal conditions. A server should
    /// NOT send this error code if it detects a content change
    /// in it unprocessed messages. The result even computed
    /// on an older state might still be useful for the client.
    ///
    /// If a client decides that a result is not of any use anymore
    /// the client should cancel the request.
    ContentModified = -32801,

    /// The server cancelled the request. This error code should
    /// only be used for requests that explicitly support being
    /// server cancellable.
    ///
    /// @since 3.17.0
    ServerCancelled = -32802,

    /// A request failed but it was syntactically correct, e.g the
    /// method name was known and the parameters were valid. The error
    /// message should contain human readable information about why
    /// the request failed.
    ///
    /// @since 3.17.0
    RequestFailed = -32803,
}

#[derive(Debug, Clone)]
pub struct Notification {
    pub method: String,
    pub params: serde_json::Value,
}

// --- Serialize impls ---

impl serde::Serialize for IdRepr {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            IdRepr::I32(n) => s.serialize_i32(*n),
            IdRepr::String(v) => s.serialize_str(v),
        }
    }
}

impl serde::Serialize for RequestId {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(s)
    }
}

impl serde::Serialize for Request {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let n = 2 + usize::from(!self.params.is_null());
        let mut map = s.serialize_map(Some(n))?;
        map.serialize_entry("id", &self.id)?;
        map.serialize_entry("method", &self.method)?;
        if !self.params.is_null() {
            map.serialize_entry("params", &self.params)?;
        }
        map.end()
    }
}

impl serde::Serialize for Response {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let n = 1 + usize::from(self.result.is_some()) + usize::from(self.error.is_some());
        let mut map = s.serialize_map(Some(n))?;
        map.serialize_entry("id", &self.id)?;
        if let Some(r) = &self.result {
            map.serialize_entry("result", r)?;
        }
        if let Some(e) = &self.error {
            map.serialize_entry("error", e)?;
        }
        map.end()
    }
}

impl serde::Serialize for ResponseError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let n = 2 + usize::from(self.data.is_some());
        let mut map = s.serialize_map(Some(n))?;
        map.serialize_entry("code", &self.code)?;
        map.serialize_entry("message", &self.message)?;
        if let Some(d) = &self.data {
            map.serialize_entry("data", d)?;
        }
        map.end()
    }
}

impl serde::Serialize for Notification {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let n = 1 + usize::from(!self.params.is_null());
        let mut map = s.serialize_map(Some(n))?;
        map.serialize_entry("method", &self.method)?;
        if !self.params.is_null() {
            map.serialize_entry("params", &self.params)?;
        }
        map.end()
    }
}

// Untagged: serialize as the inner variant directly.
impl serde::Serialize for Message {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Message::Request(r) => r.serialize(s),
            Message::Response(r) => r.serialize(s),
            Message::Notification(n) => n.serialize(s),
        }
    }
}

// --- Deserialize impls ---

impl<'de> serde::Deserialize<'de> for IdRepr {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = IdRepr;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an integer or string id")
            }
            fn visit_i32<E: serde::de::Error>(self, v: i32) -> Result<IdRepr, E> {
                Ok(IdRepr::I32(v))
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<IdRepr, E> {
                Ok(IdRepr::I32(v as i32))
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<IdRepr, E> {
                Ok(IdRepr::I32(v as i32))
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<IdRepr, E> {
                Ok(IdRepr::String(v.to_owned()))
            }
            fn visit_string<E: serde::de::Error>(self, v: String) -> Result<IdRepr, E> {
                Ok(IdRepr::String(v))
            }
        }
        d.deserialize_any(V)
    }
}

impl<'de> serde::Deserialize<'de> for RequestId {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        IdRepr::deserialize(d).map(RequestId)
    }
}

// For the struct types we deserialize via serde_json::Value as an intermediate
// so we can do field-by-field extraction without a full MapAccess visitor.
fn missing<E: serde::de::Error>(field: &'static str) -> E {
    E::missing_field(field)
}

fn bad_type<E: serde::de::Error>(field: &str, expected: &str, got: &serde_json::Value) -> E {
    E::custom(format!("field `{field}` expected {expected}, got {got}"))
}

impl<'de> serde::Deserialize<'de> for ResponseError {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let mut map = serde_json::Map::<String, serde_json::Value>::deserialize(d)?;
        let code = match map.remove("code").ok_or_else(|| missing("code"))? {
            serde_json::Value::Number(n) => {
                n.as_i64().ok_or_else(|| serde::de::Error::custom("code is not an integer"))?
                    as i32
            }
            v => return Err(bad_type("code", "number", &v)),
        };
        let message = match map.remove("message").ok_or_else(|| missing("message"))? {
            serde_json::Value::String(s) => s,
            v => return Err(bad_type("message", "string", &v)),
        };
        let data = map.remove("data");
        Ok(ResponseError { code, message, data })
    }
}

// Untagged Message: inspect which top-level keys are present to pick the variant.
impl<'de> serde::Deserialize<'de> for Message {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let mut map = serde_json::Map::<String, serde_json::Value>::deserialize(d)?;
        let has_method = map.contains_key("method");
        let has_id = map.contains_key("id");

        if has_id && has_method {
            let id = RequestId::deserialize(map.remove("id").unwrap())
                .map_err(serde::de::Error::custom)?;
            let method = match map.remove("method").unwrap() {
                serde_json::Value::String(s) => s,
                v => return Err(bad_type("method", "string", &v)),
            };
            let params = map.remove("params").unwrap_or(serde_json::Value::Null);
            Ok(Message::Request(Request { id, method, params }))
        } else if has_method {
            let method = match map.remove("method").unwrap() {
                serde_json::Value::String(s) => s,
                v => return Err(bad_type("method", "string", &v)),
            };
            let params = map.remove("params").unwrap_or(serde_json::Value::Null);
            Ok(Message::Notification(Notification { method, params }))
        } else if has_id {
            let id = RequestId::deserialize(map.remove("id").unwrap())
                .map_err(serde::de::Error::custom)?;
            let result = map.remove("result");
            let error = match map.remove("error") {
                Some(v) => Some(
                    ResponseError::deserialize(v).map_err(serde::de::Error::custom)?,
                ),
                None => None,
            };
            Ok(Message::Response(Response { id, result, error }))
        } else {
            Err(serde::de::Error::custom("LSP message has neither `id` nor `method`"))
        }
    }
}

fn invalid_data(error: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}

macro_rules! invalid_data {
    ($($tt:tt)*) => (invalid_data(format!($($tt)*)))
}

impl Message {
    pub fn read(r: &mut impl BufRead) -> io::Result<Option<Message>> {
        Message::_read(r)
    }
    fn _read(r: &mut dyn BufRead) -> io::Result<Option<Message>> {
        let text = match read_msg_text(r)? {
            None => return Ok(None),
            Some(text) => text,
        };

        let msg = match serde_json::from_str(&text) {
            Ok(msg) => msg,
            Err(e) => {
                return Err(invalid_data!("malformed LSP payload `{e:?}`: {text:?}"));
            }
        };

        Ok(Some(msg))
    }
    pub fn write(&self, w: &mut impl Write) -> io::Result<()> {
        self._write(w)
    }
    fn _write(&self, w: &mut dyn Write) -> io::Result<()> {
        // Serialize the inner fields then prepend jsonrpc:"2.0".
        let inner = serde_json::to_string(self)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        // inner is always a non-empty JSON object starting with '{'.
        let text = format!("{{\"jsonrpc\":\"2.0\",{}", &inner[1..]);
        write_msg_text(w, &text)
    }
}

impl Response {
    pub fn new_ok<R: serde::Serialize>(id: RequestId, result: R) -> Response {
        Response { id, result: Some(serde_json::to_value(result).unwrap()), error: None }
    }
    pub fn new_err(id: RequestId, code: i32, message: String) -> Response {
        let error = ResponseError { code, message, data: None };
        Response { id, result: None, error: Some(error) }
    }
}

impl Request {
    pub fn new<P: serde::Serialize>(id: RequestId, method: String, params: P) -> Request {
        Request { id, method, params: serde_json::to_value(params).unwrap() }
    }
    pub fn extract<P: DeserializeOwned>(
        self,
        method: &str,
    ) -> Result<(RequestId, P), ExtractError<Request>> {
        if self.method != method {
            return Err(ExtractError::MethodMismatch(self));
        }
        match serde_json::from_value(self.params) {
            Ok(params) => Ok((self.id, params)),
            Err(error) => Err(ExtractError::JsonError { method: self.method, error }),
        }
    }

    pub(crate) fn is_shutdown(&self) -> bool {
        self.method == "shutdown"
    }
    pub(crate) fn is_initialize(&self) -> bool {
        self.method == "initialize"
    }
}

impl Notification {
    pub fn new(method: String, params: impl serde::Serialize) -> Notification {
        Notification { method, params: serde_json::to_value(params).unwrap() }
    }
    pub fn extract<P: DeserializeOwned>(
        self,
        method: &str,
    ) -> Result<P, ExtractError<Notification>> {
        if self.method != method {
            return Err(ExtractError::MethodMismatch(self));
        }
        match serde_json::from_value(self.params) {
            Ok(params) => Ok(params),
            Err(error) => Err(ExtractError::JsonError { method: self.method, error }),
        }
    }
    pub(crate) fn is_exit(&self) -> bool {
        self.method == "exit"
    }
    pub(crate) fn is_initialized(&self) -> bool {
        self.method == "initialized"
    }
}

fn read_msg_text(inp: &mut dyn BufRead) -> io::Result<Option<String>> {
    let mut size = None;
    let mut buf = String::new();
    loop {
        buf.clear();
        if inp.read_line(&mut buf)? == 0 {
            return Ok(None);
        }
        if !buf.ends_with("\r\n") {
            return Err(invalid_data!("malformed header: {:?}", buf));
        }
        let buf = &buf[..buf.len() - 2];
        if buf.is_empty() {
            break;
        }
        let mut parts = buf.splitn(2, ": ");
        let header_name = parts.next().unwrap();
        let header_value =
            parts.next().ok_or_else(|| invalid_data!("malformed header: {:?}", buf))?;
        if header_name.eq_ignore_ascii_case("Content-Length") {
            size = Some(header_value.parse::<usize>().map_err(invalid_data)?);
        }
    }
    let size: usize = size.ok_or_else(|| invalid_data!("no Content-Length"))?;
    let mut buf = buf.into_bytes();
    buf.resize(size, 0);
    inp.read_exact(&mut buf)?;
    let buf = String::from_utf8(buf).map_err(invalid_data)?;
    log::debug!("< {buf}");
    Ok(Some(buf))
}

fn write_msg_text(out: &mut dyn Write, msg: &str) -> io::Result<()> {
    log::debug!("> {msg}");
    write!(out, "Content-Length: {}\r\n\r\n", msg.len())?;
    out.write_all(msg.as_bytes())?;
    out.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Message, Notification, Request, RequestId};

    #[test]
    fn shutdown_with_explicit_null() {
        let text = "{\"jsonrpc\": \"2.0\",\"id\": 3,\"method\": \"shutdown\", \"params\": null }";
        let msg: Message = serde_json::from_str(text).unwrap();

        assert!(
            matches!(msg, Message::Request(req) if req.id == 3.into() && req.method == "shutdown")
        );
    }

    #[test]
    fn shutdown_with_no_params() {
        let text = "{\"jsonrpc\": \"2.0\",\"id\": 3,\"method\": \"shutdown\"}";
        let msg: Message = serde_json::from_str(text).unwrap();

        assert!(
            matches!(msg, Message::Request(req) if req.id == 3.into() && req.method == "shutdown")
        );
    }

    #[test]
    fn notification_with_explicit_null() {
        let text = "{\"jsonrpc\": \"2.0\",\"method\": \"exit\", \"params\": null }";
        let msg: Message = serde_json::from_str(text).unwrap();

        assert!(matches!(msg, Message::Notification(not) if not.method == "exit"));
    }

    #[test]
    fn notification_with_no_params() {
        let text = "{\"jsonrpc\": \"2.0\",\"method\": \"exit\"}";
        let msg: Message = serde_json::from_str(text).unwrap();

        assert!(matches!(msg, Message::Notification(not) if not.method == "exit"));
    }

    #[test]
    fn serialize_request_with_null_params() {
        let msg = Message::Request(Request {
            id: RequestId::from(3),
            method: "shutdown".into(),
            params: serde_json::Value::Null,
        });
        let serialized = serde_json::to_string(&msg).unwrap();

        assert_eq!("{\"id\":3,\"method\":\"shutdown\"}", serialized);
    }

    #[test]
    fn serialize_notification_with_null_params() {
        let msg = Message::Notification(Notification {
            method: "exit".into(),
            params: serde_json::Value::Null,
        });
        let serialized = serde_json::to_string(&msg).unwrap();

        assert_eq!("{\"method\":\"exit\"}", serialized);
    }
}
