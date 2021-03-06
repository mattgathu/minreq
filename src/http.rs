use crate::connection::Connection;
use std::collections::HashMap;
use std::fmt;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Error;

/// A URL type for requests.
pub type URL = String;


/// An HTTP Response Status
#[derive(Clone, Debug)]
pub enum Status {
    /// Informational: 1XX
    Info(i32),
    /// Success: 2XX
    Success(i32),
    /// Redirections: 3XX
    Redirect(i32),
    /// Client Errors: 4XX
    ClientError(i32),
    /// Server Errors: 5XX
    ServerError(i32),
}

impl Status {
    /// check is status is a success
    pub fn is_success(&self) -> bool {
        match self {
            Status::Success(_) => true,
            _ => false,
        }
    }
}

impl From<i32> for Status {
    fn from(i: i32) -> Self {
        if i >= 100 && i < 200 {
            Status::Info(i)
        } else if i >= 200 && i < 300 {
            Status::Success(i)
        } else if i >= 300 && i < 400 {
            Status::Redirect(i)
        } else if i >= 400 && i < 500 {
            Status::ClientError(i)
        } else {
            Status::ServerError(i)
        }
    }
}

impl From<&Status> for i32 {
    fn from(s: &Status) -> i32 {
        match *s {
            Status::Info(i) => i,
            Status::Success(i) => i,
            Status::Redirect(i) => i,
            Status::ClientError(i) => i,
            Status::ServerError(i) => i,
        }
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let code: i32 = i32::from(self);
        write!(f, "{}", code)
    }
}

/// An HTTP request method.
#[derive(Clone, Debug)]
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
        match *self {
            Method::Get => write!(f, "GET"),
            Method::Head => write!(f, "HEAD"),
            Method::Post => write!(f, "POST"),
            Method::Put => write!(f, "PUT"),
            Method::Delete => write!(f, "DELETE"),
            Method::Connect => write!(f, "CONNECT"),
            Method::Options => write!(f, "OPTIONS"),
            Method::Trace => write!(f, "TRACE"),
            Method::Patch => write!(f, "PATCH"),
            Method::Custom(ref s) => write!(f, "{}", s),
        }
    }
}

/// An HTTP request.
#[derive(Clone, Debug)]
pub struct Request {
    method: Method,
    pub(crate) host: URL,
    pub(crate) resource: URL,
    headers: HashMap<String, String>,
    pub(crate) body: Option<String>,
    pub(crate) timeout: Option<u64>,
    pub(crate) https: bool,
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

    /// Adds headers to the request.
    pub fn with_headers(mut self, headers: &HashMap<String, String>) -> Request {
        for (k, v) in headers.iter() {
            self.headers.insert(k.to_string(), v.to_string());
        }
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
            http += &body;
        }
        http
    }
}

/// An HTTP response.
pub struct Response {
    /// The status code of the response, eg. 404.
    pub status: Status,
    /// The reason phrase of the response, eg. "Not Found".
    pub reason_phrase: String,
    /// The headers of the response.
    pub headers: HashMap<String, String>,
    /// The body of the response.
    pub body: Box<BufRead>,
}

impl Response {
    pub(crate) fn from_stream<T: std::io::Read + 'static>(stream: T) -> std::io::Result<Response> {
        let mut stream = BufReader::new(stream);
        // get http status line
        let mut s = String::new();
        stream.read_line(&mut s)?;
        let (status, reason_phrase) = parse_status_line(&s);
        // get http headers
        let mut buf: Vec<String> = Vec::new();
        loop {
            let mut s = String::new();
            stream.read_line(&mut s)?;
            if s.trim().is_empty() {
                break;
            } else {
                buf.push(s.trim().to_string());
            }
        }

        let headers: HashMap<String, String> = buf
            .iter()
            .map(|elem| {
                let idx = elem.find(':').unwrap();
                let (key, value) = elem.split_at(idx);
                (key.to_string(), value[1..].trim().to_string())
            })
            .collect();

        let resp = Response {
            status,
            reason_phrase,
            headers,
            body: Box::new(stream),
        };

        Ok(resp)
    }
}

impl fmt::Debug for Response {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Response{{ status_code: {}, reason_phrase: {}, headers: {:#?}, body: <BufRead> }}",
            self.status, self.reason_phrase, self.headers
        )
    }
}

pub(crate) fn parse_url(url: URL) -> (URL, URL, bool) {
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
    if second.is_empty() {
        second += "/";
    }
    // Set appropriate port
    let https = url.starts_with("https://");
    if !first.contains(':') {
        first += if https { ":443" } else { ":80" };
    }
    (first, second, https)
}

pub(crate) fn parse_status_line(line: &str) -> (Status, String) {
    let mut split = line.split(' ');
    if let Some(code) = split.nth(1) {
        if let Ok(code) = code.parse::<i32>() {
            if let Some(reason) = split.next() {
                return (Status::from(code), reason.to_string());
            }
        }
    }
    (Status::from(503), "Server did not provide a status line".to_string())
}
