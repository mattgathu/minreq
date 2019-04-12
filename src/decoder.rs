use std::io::{BufRead,  Read};

use flate2::bufread::ZlibDecoder;
use flate2::bufread::GzDecoder;

use crate::http::Headers;

type Body = Box<Read>;

/// A response decompressor over a BufRead stream.
pub enum Decoder {
    /// A `Identity` decoder just returns the response content as is.
    Identity(Body),
    /// A `Gzip` decoder will uncompress the gzipped response content before returning it.
    Gzip(Body),
    /// A `Deflate` decoder will uncompress response content before returning it
    Deflate(Body),
}

impl Decoder {
    /// An identity decoder.
    ///
    /// This decoder will emit the underlying chunks as-is.
    #[inline]
    fn identity<B: BufRead + 'static>(b: B) -> Decoder {
        Decoder::Identity(Box::new(b))
    }

    /// A gzip decoder.
    ///
    /// This decoder will buffer and decompress chunks that are gzipped.
    #[inline]
    fn gzip<B: BufRead + 'static>(b: B) -> Decoder {
        Decoder::Gzip(Box::new(GzDecoder::new(b)))
    }

    /// A deflate decoder.
    ///
    /// This decoder will decompress its underlying chunks
    #[inline]
    fn deflate<B: BufRead + 'static>(b: B) -> Decoder {
        Decoder::Deflate(Box::new(ZlibDecoder::new(b)))
    }

    /// Constructs a Decoder from a Response.
    ///
    ///
    /// Uses the correct variant by inspecting the Content-Encoding header.
    pub(crate) fn detect<B: BufRead + 'static>(headers: &mut Headers, b: B) -> Decoder {
        match detect_encoding(&headers).as_str() {
            "gzip" => {
                headers.remove("Content-Encoding");
                headers.remove("Content-Length");
                Decoder::gzip(b)
            }
            "deflate" => {
                headers.remove("Content-Encoding");
                headers.remove("Content-Length");
                Decoder::deflate(b)
            }
            _ => Decoder::identity(b),
        }

    }
}

impl Read for Decoder {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Decoder::Gzip(body) => body.read(buf),
            Decoder::Deflate(body) => body.read(buf),
            Decoder::Identity(body) => body.read(buf),
        }
    }
}

fn detect_encoding(headers: &Headers) -> String {
    if let Some(val) = headers.get("Content-Encoding") {
        val.trim().to_string()
    } else if let Some(tval) = headers.get("Transfer-Encoding") {
        tval.trim().to_string()
    } else {
        "".to_string()
    }
}
