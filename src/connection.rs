use crate::http::{parse_url, Request, Response, Status};
#[cfg(feature = "https")]
use rustls::{self, ClientConfig, ClientSession};
use std::env;
use std::io::{BufReader, BufWriter, Error, Write};
use std::net::TcpStream;
#[cfg(feature = "https")]
use std::sync::Arc;
use std::time::Duration;
#[cfg(feature = "https")]
use webpki::DNSNameRef;
#[cfg(feature = "https")]
use webpki_roots::TLS_SERVER_ROOTS;

/// A connection to the server for sending
/// [`Request`](struct.Request.html)s.
pub struct Connection {
    request: Request,
    timeout: Option<u64>,
}

impl Connection {
    /// Creates a new `Connection`. See
    /// [`Request`](struct.Request.html) for specifics about *what* is
    /// being sent.
    pub(crate) fn new(request: Request) -> Connection {
        let timeout = request
            .timeout
            .or_else(|| match env::var("MINREQ_TIMEOUT") {
                Ok(t) => t.parse::<u64>().ok(),
                Err(_) => None,
            });
        Connection { request, timeout }
    }

    /// Sends the [`Request`](struct.Request.html), consumes this
    /// connection, and returns a [`Response`](struct.Response.html).
    #[cfg(feature = "https")]
    pub(crate) fn send_https(self) -> Result<Response, Error> {
        let req_copy = self.request.clone();
        let host = self.request.host.clone();
        let bytes = self.request.into_string().into_bytes();

        // Rustls setup
        let dns_name = host.clone();
        let dns_name = dns_name.split(':').next().unwrap();
        let dns_name = DNSNameRef::try_from_ascii_str(dns_name).unwrap();
        let mut config = ClientConfig::new();
        config
            .root_store
            .add_server_trust_anchors(&TLS_SERVER_ROOTS);
        let sess = ClientSession::new(&Arc::new(config), dns_name);

        // IO
        let stream = create_tcp_stream(host, self.timeout)?;
        let mut tls = rustls::StreamOwned::new(sess, stream);
        let _ = tls.write(&bytes)?;
        let resp = Response::from_stream(tls)?;
        match resp.status {
            Status::Redirect(_) => Self::handle_redirect(req_copy, resp),
            _ => Ok(resp),
        }
    }

    /// handle https redirect
    fn handle_redirect(mut req: Request, resp: Response) -> Result<Response, Error> {
        if let Some(loc) = resp.headers.get("Location") {
            let url = if loc.starts_with("https://") || loc.starts_with("http://") {
                loc.to_string()
            } else {
                let scheme = if req.https { "https://" } else { "http://" };
                format!("{}{}{}", scheme, req.host, loc)
            };
            let (host, resource, https) = parse_url(url);
            req.host = host;
            req.resource = resource;
            req.https = https;
            req.body = None;
            req.send()
        } else {
            Ok(resp)
        }
    }

    /// Sends the [`Request`](struct.Request.html), consumes this
    /// connection, and returns a [`Response`](struct.Response.html).
    pub(crate) fn send(self) -> Result<Response, Error> {
        let req_copy = self.request.clone();
        let host = self.request.host.clone();
        let bytes = self.request.into_string().into_bytes();

        let tcp = create_tcp_stream(host, self.timeout)?;

        // Send request
        let mut stream = BufWriter::new(tcp);
        stream.write_all(&bytes)?;
        let buf = BufReader::new(stream.into_inner()?);
        let resp = Response::from_stream(buf)?;
        match resp.status {
            Status::Redirect(_) => Self::handle_redirect(req_copy, resp),
            _ => Ok(resp),
        }
    }
}

fn create_tcp_stream(host: String, timeout: Option<u64>) -> Result<TcpStream, Error> {
    let stream = TcpStream::connect(host)?;
    if let Some(secs) = timeout {
        let dur = Some(Duration::from_secs(secs));
        stream.set_read_timeout(dur)?;
        stream.set_write_timeout(dur)?;
    }
    Ok(stream)
}
