// Copyright 2026 manko Extension Contributors; SPDX-License-Identifier: Apache-2.0
use base64::{engine::general_purpose::STANDARD, Engine};
use futures::{future::join_all, task::noop_waker};
use serde_json::{json, Value};
use std::{
    cell::RefCell,
    collections::HashMap,
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context as TaskContext, Poll},
};

pub type Result<T> = std::result::Result<T, Value>;
pub type Operation = Pin<Box<dyn Future<Output = Result<Value>>>>;
pub type Handler = fn(Context, String, Value) -> Operation;
pub mod content;
pub mod connector;

pub fn error(message: impl Into<String>, kind: &str) -> Value {
    let mut chars = kind.chars();
    let name = format!(
        "{}{}{}",
        chars.next().unwrap_or('E').to_uppercase(),
        chars.as_str(),
        if kind.ends_with("Error") { "" } else { "Error" }
    );
    json!({"name":name, "message":message.into(), "type":kind})
}
pub fn text(value: &Value) -> &str {
    value.as_str().unwrap_or("")
}
pub fn id(value: &Value) -> Result<String> {
    let string = value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| value.to_string());
    let number = string
        .parse::<u64>()
        .map_err(|_| error("Invalid identifier", "invalidIdentifier"))?;
    if number == 0 || number > 9_007_199_254_740_991 || string.starts_with('0') {
        return Err(error("Invalid identifier", "invalidIdentifier"));
    }
    Ok(string)
}
pub fn bytes(response: &Value) -> Result<Vec<u8>> {
    STANDARD
        .decode(text(&response["dataBase64"]))
        .map_err(|_| error("Invalid response encoding", "invalidResponse"))
}
pub fn decode_json(response: &Value) -> Result<Value> {
    let data = bytes(response)?;
    if data.is_empty() {
        return Ok(json!({}));
    }
    serde_json::from_slice(&data)
        .map_err(|_| error("Service returned malformed JSON", "invalidResponse"))
}
pub fn header<'a>(response: &'a Value, name: &str) -> &'a str {
    response["headers"]
        .as_object()
        .and_then(|h| h.iter().find(|(key, _)| key.eq_ignore_ascii_case(name)))
        .map(|(_, value)| text(value))
        .unwrap_or("")
}
pub fn status(response: &Value) -> u64 {
    response["status"].as_u64().unwrap_or(0)
}
pub fn successful(response: &Value) -> Result<()> {
    match status(response) {
        200..=299 => Ok(()),
        401 | 403 => Err(error("Authentication required", "authenticationRequired")),
        404 => Err(error("The requested item was not found", "notFound")),
        value => Err(error(
            format!("Service returned HTTP {value}"),
            "serviceError",
        )),
    }
}
pub fn https(value: &str, hosts: &[&str]) -> Result<url::Url> {
    let url = url::Url::parse(value).map_err(|_| error("Invalid URL", "invalidResponse"))?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || !hosts.contains(&url.host_str().unwrap_or(""))
    {
        return Err(error(
            "The service supplied an unexpected URL",
            "hostNotAllowed",
        ));
    }
    Ok(url)
}
pub fn page(arguments: &Value, maximum: u64) -> Result<u64> {
    let value = arguments
        .get("metadata")
        .or_else(|| arguments.get("cursor"))
        .and_then(|cursor| cursor.get("page"))
        .filter(|value| !value.is_null());
    let page = value.map(|value| value.as_u64().unwrap_or(0)).unwrap_or(1);
    if page == 0 || page > maximum {
        return Err(error("Invalid page cursor", "invalidCursor"));
    }
    Ok(page)
}
pub fn timestamp(seconds: i64) -> Value {
    chrono::DateTime::from_timestamp(seconds, 0)
        .map(|date| json!(date.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)))
        .unwrap_or(Value::Null)
}

#[derive(Default)]
struct IO {
    sequence: u64,
    pending: Vec<Value>,
    responses: HashMap<String, Value>,
    completed: Vec<Value>,
}

#[derive(Clone, Default)]
pub struct Context {
    io: Rc<RefCell<IO>>,
}

struct HostCall {
    context: Context,
    id: String,
}
impl Future for HostCall {
    type Output = Result<Value>;
    fn poll(self: Pin<&mut Self>, _: &mut TaskContext<'_>) -> Poll<Self::Output> {
        let Some(response) = self.context.io.borrow_mut().responses.remove(&self.id) else {
            return Poll::Pending;
        };
        if !response["error"].is_null() {
            Poll::Ready(Err(response["error"].clone()))
        } else {
            Poll::Ready(Ok(response["payload"].clone()))
        }
    }
}

impl Context {
    pub async fn call(&self, service: &str, arguments: Value) -> Result<Value> {
        let id = {
            let mut io = self.io.borrow_mut();
            io.sequence += 1;
            let id = io.sequence.to_string();
            io.pending
                .push(json!({"id":id, "service":service, "arguments":arguments}));
            id
        };
        HostCall {
            context: self.clone(),
            id,
        }
        .await
    }
    pub async fn request(
        &self,
        url: &str,
        method: &str,
        headers: Value,
        body: Option<Vec<u8>>,
        image: bool,
    ) -> Result<Value> {
        self.call(
            "http",
            json!({"url":url,"method":method,"headers":headers,"cookies":{},
            "bodyBase64":body.map(|bytes| STANDARD.encode(bytes)),"imageResource":image}),
        )
        .await
    }
    /// Requests account-scoped access through Manko. Tokens never enter WASM.
    pub async fn authenticated_request(
        &self, url: &str, method: &str, headers: Value, body: Option<Vec<u8>>,
    ) -> Result<Value> {
        self.call("http", json!({"url":url,"method":method,"headers":headers,
            "bodyBase64":body.map(|bytes| STANDARD.encode(bytes)),"authenticated":true})).await
    }
    pub async fn secure(&self, key: &str) -> Result<Value> {
        self.call("secureState", json!({"operation":"get","key":key}))
            .await
    }
    pub async fn now(&self) -> Result<Value> {
        self.call("clock", json!({})).await
    }
    pub async fn cache_read(&self, key: &str, ttl: Option<f64>) -> Result<Value> {
        self.call(
            "cache",
            json!({"operation":"read","key":key,"ttlSeconds":ttl}),
        )
        .await
    }
    pub async fn cache_write(&self, key: &str, ticket: &Value, value: &Value) -> Result<()> {
        self.call(
            "cache",
            json!({"operation":"write","key":key,"permit":ticket["permit"],"value":value}),
        )
        .await?;
        Ok(())
    }
    pub async fn remember<F: Future<Output = Result<Value>>>(
        &self,
        key: &str,
        ttl: Option<f64>,
        loader: F,
        validate: impl Fn(&Value) -> Result<()>,
    ) -> Result<Value> {
        let ticket = self.cache_read(key, ttl).await?;
        if ticket["hit"] == true && validate(&ticket["value"]).is_ok() {
            return Ok(ticket["value"].clone());
        }
        self.call("cache", json!({"operation":"acquire","key":key}))
            .await?;
        let result = async {
            let ticket = self.cache_read(key, ttl).await?;
            if ticket["hit"] == true {
                if validate(&ticket["value"]).is_ok() {
                    return Ok(ticket["value"].clone());
                }
                self.call("cache", json!({"operation":"remove","key":key}))
                    .await?;
            }
            let ticket = self.cache_read(key, ttl).await?;
            let value = loader.await?;
            validate(&value)?;
            self.cache_write(key, &ticket, &value).await?;
            Ok(value)
        }
        .await;
        self.call("cache", json!({"operation":"release","key":key}))
            .await?;
        result
    }
    pub async fn json(&self, url: &str, headers: Value) -> Result<Value> {
        let response = self.request(url, "GET", headers, None, false).await?;
        successful(&response)?;
        decode_json(&response)
    }
}

struct Invocation {
    context: Context,
    future: Operation,
}
pub struct Runtime {
    id: &'static str,
    kind: &'static str,
    handler: Handler,
    operations: HashMap<String, Invocation>,
}

impl Runtime {
    pub fn new(id: &'static str, kind: &'static str, handler: Handler) -> Self {
        Self {
            id,
            kind,
            handler,
            operations: HashMap::new(),
        }
    }
    pub fn dispatch(&mut self, input: Value) -> Value {
        let owner = text(&input["invocationID"]).to_owned();
        match text(&input["action"]) {
            "metadata" => {
                return json!({"id":self.id,"kind":self.kind,"apiVersion":"3.0",
                                       "imageRequestMode":"independent","cachePolicy":"metadata"})
            }
            "cancel" => {
                self.operations.remove(&owner);
                return json!({"ok":true,"value":null});
            }
            "invoke" => {
                if owner.is_empty()
                    || owner.len() > 128
                    || self.operations.len() >= 64
                    || self.operations.contains_key(&owner)
                {
                    return json!({"ok":false,"error":error("Invalid invocation", "invalidInvocation")});
                }
                let context = Context::default();
                let method = text(&input["method"]).to_owned();
                let mut args = input["arguments"].clone();
                if self.kind == "content" {
                    args = match method.as_str() {
                        "details" => args.get("workID").cloned().unwrap_or(args),
                        "installments" => args.get("sourceWork").cloned().unwrap_or(args),
                        "imagePages" | "publicationContent" => {
                            args.get("extensionInstallment").cloned().unwrap_or(args)
                        }
                        "search" => {
                            if args.get("query").is_none() {
                                args["query"] = args["title"].clone();
                            }
                            args
                        }
                        _ => args,
                    };
                }
                let handler = self.handler;
                let future: Operation = if method == "imagePageContents" {
                    let context = context.clone();
                    Box::pin(async move {
                        let urls = args["urls"]
                            .as_array()
                            .ok_or_else(|| error("Invalid image batch", "invalidResponse"))?;
                        let ids = args["requestIDs"]
                            .as_array()
                            .ok_or_else(|| error("Invalid image batch", "invalidResponse"))?;
                        if urls.len() > 4 || urls.len() != ids.len() {
                            return Err(error("Invalid image batch", "invalidResponse"));
                        }
                        let calls = urls.iter().zip(ids).map(|(url, id)| {
                            let id = id.clone();
                            let future = handler(
                                context.clone(),
                                "imagePageContent".into(),
                                json!({"url":url}),
                            );
                            let context = context.clone();
                            async move {
                                let envelope = match future.await {
                                    Ok(value) => json!({"ok":true,"value":value}),
                                    Err(error) => json!({"ok":false,"error":error}),
                                };
                                context
                                    .io
                                    .borrow_mut()
                                    .completed
                                    .push(json!({"requestID":id,"envelope":envelope}));
                            }
                        });
                        join_all(calls).await;
                        Ok(Value::Null)
                    })
                } else {
                    handler(context.clone(), method, args)
                };
                self.operations
                    .insert(owner.clone(), Invocation { context, future });
            }
            "resume" => {
                let Some(operation) = self.operations.get(&owner) else {
                    return json!({"ok":false,"error":error("Unknown invocation", "invalidInvocation")});
                };
                for response in input["responses"].as_array().into_iter().flatten() {
                    operation
                        .context
                        .io
                        .borrow_mut()
                        .responses
                        .insert(text(&response["id"]).to_owned(), response.clone());
                }
            }
            _ => {
                return json!({"ok":false,"error":error("Unsupported dispatch action", "unsupportedOperation")})
            }
        }
        let Some(mut operation) = self.operations.remove(&owner) else {
            return json!({"ok":false,"error":error("Unknown invocation", "invalidInvocation")});
        };
        let waker = noop_waker();
        match operation
            .future
            .as_mut()
            .poll(&mut TaskContext::from_waker(&waker))
        {
            Poll::Ready(Ok(value)) => {
                let completed = std::mem::take(&mut operation.context.io.borrow_mut().completed);
                json!({"ok":true,"value":value,"completed":completed})
            }
            Poll::Ready(Err(error)) => json!({"ok":false,"error":error}),
            Poll::Pending => {
                let pending = std::mem::take(&mut operation.context.io.borrow_mut().pending);
                let completed = std::mem::take(&mut operation.context.io.borrow_mut().completed);
                self.operations.insert(owner, operation);
                json!({"pending":pending,"completed":completed})
            }
        }
    }
}

#[macro_export]
macro_rules! export_extension {
    ($id:expr, $kind:expr, $handler:path) => {
        std::thread_local! {
            static RUNTIME: std::cell::RefCell<$crate::Runtime> = std::cell::RefCell::new($crate::Runtime::new($id, $kind, $handler));
        }
        #[no_mangle]
        pub extern "C" fn manko_alloc(length: u32) -> u32 {
            let memory = vec![0u8; length as usize].into_boxed_slice();
            Box::into_raw(memory) as *mut u8 as u32
        }
        #[no_mangle]
        pub unsafe extern "C" fn manko_dealloc(pointer: u32, length: u32) {
            if length > 0 { drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(pointer as *mut u8, length as usize))); }
        }
        #[no_mangle]
        pub unsafe extern "C" fn manko_dispatch(pointer: u32, length: u32) -> u64 {
            let input = std::slice::from_raw_parts(pointer as *const u8, length as usize);
            let input = $crate::serde_json::from_slice(input).unwrap_or($crate::serde_json::Value::Null);
            let value = RUNTIME.with(|runtime| runtime.borrow_mut().dispatch(input));
            let bytes = $crate::serde_json::to_vec(&value).unwrap().into_boxed_slice();
            let length = bytes.len() as u64;
            let pointer = Box::into_raw(bytes) as *mut u8 as u64;
            (pointer << 32) | length
        }
    };
}
pub use serde_json;

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(context: Context, method: String, _: Value) -> Operation {
        Box::pin(async move {
            if method == "request" {
                context
                    .call("http", json!({"url":"https://example.test"}))
                    .await
            } else {
                Ok(json!(method))
            }
        })
    }
    #[test]
    fn suspend_resume_and_cancel() {
        let mut runtime = Runtime::new("fixture", "content", fixture);
        let pending = runtime.dispatch(
            json!({"action":"invoke","invocationID":"one","method":"request","arguments":{}}),
        );
        assert_eq!(pending["pending"][0]["service"], "http");
        let other = runtime.dispatch(
            json!({"action":"invoke","invocationID":"two","method":"settings","arguments":{}}),
        );
        assert_eq!(other["value"], "settings");
        let result = runtime.dispatch(json!({"action":"resume","invocationID":"one","responses":[{"id":"1","payload":{"status":200},"error":null}]}));
        assert_eq!(result["value"]["status"], 200);
        runtime.dispatch(
            json!({"action":"invoke","invocationID":"three","method":"request","arguments":{}}),
        );
        runtime.dispatch(json!({"action":"cancel","invocationID":"three"}));
        assert_eq!(
            runtime.dispatch(json!({"action":"resume","invocationID":"three","responses":[]}))
                ["ok"],
            false
        );
    }
    #[test]
    fn identifiers_and_urls_are_validated() {
        assert!(id(&json!("0")).is_err());
        assert!(id(&json!("01")).is_err());
        assert!(id(&json!("9007199254740992")).is_err());
        assert_eq!(id(&json!(123)).unwrap(), "123");
        assert!(https("https://user:secret@example.test", &["example.test"]).is_err());
    }
}
