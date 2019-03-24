use std::collections::HashMap;
use std::fmt;
use std::io::Error;
use std::io::Read;
use crate::connection::Connection;
use failure::{Fallible, bail};
use std::net::TcpStream;

/// A URL type for requests.
pub type URL = String;

/// An HTTP request method.
pub enum Method {
    /// The GET method
    Get,
    /// The HEAD method
    Head,
    /// The POST method
    Post,
    /// The PUT method
    Put,
    /// The DELETE method
    Delete,
    /// The CONNECT method
    Connect,
    /// The OPTIONS method
    Options,
    /// The TRACE method
    Trace,
    /// The PATCH method
    Patch,
    /// A custom method, use with care: the string will be embedded in
    /// your request as-is.
    Custom(String),
}

impl fmt::Display for Method {
    /// Formats the Method to the form in the HTTP request,
    /// ie. Method::Get -> "GET", Method::Post -> "POST", etc.
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            &Method::Get => write!(f, "GET"),
            &Method::Head => write!(f, "HEAD"),
            &Method::Post => write!(f, "POST"),
            &Method::Put => write!(f, "PUT"),
            &Method::Delete => write!(f, "DELETE"),
            &Method::Connect => write!(f, "CONNECT"),
            &Method::Options => write!(f, "OPTIONS"),
            &Method::Trace => write!(f, "TRACE"),
            &Method::Patch => write!(f, "PATCH"),
            &Method::Custom(ref s) => write!(f, "{}", s),
        }
    }
}

/// An HTTP request.
pub struct Request {
    method: Method,
    pub(crate) host: URL,
    resource: URL,
    headers: HashMap<String, String>,
    body: Option<String>,
    pub(crate) timeout: Option<u64>,
    https: bool,
}

impl Request {
    /// Creates a new HTTP `Request`.
    ///
    /// This is only the request's data, it is not sent yet. For
    /// sending the request, see [`send`](struct.Request.html#method.send).
    pub fn new<T: Into<URL>>(method: Method, url: T) -> Request {
        let (host, resource, https) = parse_url(url.into());
        Request {
            method,
            host,
            resource,
            headers: HashMap::new(),
            body: None,
            timeout: None,
            https,
        }
    }

    /// Adds a header to the request this is called on. Use this
    /// function to add headers to your requests.
    pub fn with_header<T: Into<String>, U: Into<String>>(mut self, key: T, value: U) -> Request {
        self.headers.insert(key.into(), value.into());
        self
    }

    /// Sets the request body.
    pub fn with_body<T: Into<String>>(mut self, body: T) -> Request {
        let body = body.into();
        let body_length = body.len();
        self.body = Some(body);
        self.with_header("Content-Length", format!("{}", body_length))
    }

    /// Sets the request timeout.
    pub fn with_timeout(mut self, timeout: u64) -> Request {
        self.timeout = Some(timeout);
        self
    }

    /// Sends this request to the host.
    #[cfg(feature = "https")]
    pub fn send(self) -> Result<Response, Error> {
        if self.https {
            Connection::new(self).send_https()
        } else {
            Connection::new(self).send()
        }
    }

    /// Sends this request to the host.
    #[cfg(not(feature = "https"))]
    pub fn send(self) -> Result<Response, Error> {
        if self.https {
            panic!("Can't send requests to urls that start with https:// when the `https` feature is not enabled!")
        } else {
            Connection::new(self).send()
        }
    }

    /// Returns the HTTP request as a `String`, ready to be sent to
    /// the server.
    pub(crate) fn into_string(self) -> String {
        let mut http = String::new();
        // Add the request line and the "Host" header
        http += &format!(
            "{} {} HTTP/1.1\r\nHost: {}\r\n",
            self.method, self.resource, self.host
        );
        // Add other headers
        for (k, v) in self.headers {
            http += &format!("{}: {}\r\n", k, v);
        }
        // Add the body
        http += "\r\n";
        if let Some(body) = self.body {
            http += &format!("{}", body);
        }
        http
    }
}

/// An HTTP response.
pub struct Response {
    /// The status code of the response, eg. 404.
    pub status_code: i32,
    /// The reason phrase of the response, eg. "Not Found".
    pub reason_phrase: String,
    /// The headers of the response.
    pub headers: HashMap<String, String>,
    /// The body of the response.
    pub body: TcpStream,
}

impl Response {

    pub (crate) fn from_stream(stream: TcpStream) -> Response {
        parse_response(stream).unwrap()
    }
}

fn parse_response(stream: TcpStream) -> Fallible<Response> {
        // get http status line
        let (status_code, reason_phrase) = parse_status_line(&read_http_status_line(&stream)?);
        // get http headers
        let headers = parse_headers(read_http_headers(&stream)?).unwrap();
        //
        // rest is body
        let resp = Response {
            status_code,
            reason_phrase,
            headers,
            body: stream
        };

        Ok(resp)

}

fn parse_headers(s: String) -> Fallible<HashMap<String, String>> {
    let headers = s.trim();

    if headers.lines().all(|e| e.contains(':')) {
        let headers: HashMap<String, String> = headers
            .lines()
            .map(|elem| {
                let idx = elem.find(": ").unwrap();
                let (key, value) = elem.split_at(idx);
                (key.to_string(), value[2..].to_string())
            })
            .collect();

        Ok(headers)
    } else {
        bail!("error parsing")
    }
}

fn read_http_status_line<T: Read>(stream: T) -> Result<String, Error> {
    let mut buf: Vec<u8> = Vec::new();

    for b in stream.bytes() {
        buf.push(b?);
        if buf.len() > 5 && &buf[(buf.len() - 2)..] == b"\r\n" {
            break;
        }
    }
    Ok(String::from_utf8(buf).unwrap())
}

fn read_http_headers<T: Read>(stream: T) -> Result<String, Error> {
    let mut buf: Vec<u8> = Vec::new();

    for b in stream.bytes() {
        buf.push(b?);
        if buf.len() > 5 && &buf[(buf.len() - 2)..] == b"\r\n\r\n" {
            break;
        }
    }
    Ok(String::from_utf8(buf).unwrap())
}


fn parse_url(url: URL) -> (URL, URL, bool) {
    let mut first = URL::new();
    let mut second = URL::new();
    let mut slashes = 0;
    for c in url.chars() {
        if c == '/' {
            slashes += 1;
        } else if slashes == 2 {
            first.push(c);
        }
        if slashes >= 3 {
            second.push(c);
        }
    }
    // Ensure the resource is *something*
    if second.len() == 0 {
        second += "/";
    }
    // Set appropriate port
    let https = url.starts_with("https://");
    if !first.contains(":") {
        first += if https { ":443" } else { ":80" };
    }
    (first, second, https)
}

pub (crate) fn parse_status_line(line: &str) -> (i32, String) {
    let mut split = line.split(" ");
    if let Some(code) = split.nth(1) {
        if let Ok(code) = code.parse::<i32>() {
            if let Some(reason) = split.next() {
                return (code, reason.to_string());
            }
        }
    }
    (503, "Server did not provide a status line".to_string())
}
