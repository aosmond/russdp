use error::{ Error, Result };
use httparse;
use hyper::header::{ Headers, Location, CacheControl, CacheDirective };
use time::{ Tm, now_utc, Duration };

#[derive(Clone, Debug)]
pub enum Method {
    Msearch,
    Notify,
    Response
}

#[derive(Clone, Debug)]
pub struct ExtData {
    pub location: String,
    pub expires: Tm,
    pub usn: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Message {
    pub method: Method,
    pub code: Option<u16>,
    pub reason: Option<String>,
    pub headers: Headers,
    pub ext: ExtData,
}

fn extract_headers(raw_headers: &[httparse::Header]) -> Result<(Headers, ExtData)> {
    let headers = match Headers::from_raw(raw_headers) {
        Ok(h) => h,
        Err(_) => { return Err(Error::InvalidFormat); }
    };
    let location = match headers.get() {
        Some(h) => match h {
            &Location(ref url) => url.clone(),
        },
        None => { return Err(Error::InvalidHeader("Location".to_owned())); }
    };
    let expires = match headers.get() {
        Some(h) => match h {
            &CacheControl(ref directives) => {
                let mut age = None;
                for cd in directives.iter() {
                    match cd {
                        &CacheDirective::MaxAge(a) => {
                            age = Some(a);
                            break;
                        },
                        _ => { }
                    }
                }
                match age {
                    Some(a) => now_utc() + Duration::seconds(a as i64),
                    None => { return Err(Error::InvalidHeader("CacheControl".to_owned())); }
                }
            },
        },
        None => { return Err(Error::InvalidHeader("CacheControl".to_owned())); }
    };
    Ok((headers, ExtData {
        location: location,
        expires: expires,
        usn: None,
    }))
}

impl Message {
    fn parse_request(packet: &[u8]) -> Result<Self> {
        let mut headers = [httparse::EMPTY_HEADER; 16];
        let mut req = httparse::Request::new(&mut headers);
        match req.parse(packet) {
            Ok(status) => match status {
                httparse::Status::Complete(_) => {
                    let method = match req.method {
                        Some("MSEARCH") => Method::Msearch,
                        Some("NOTIFY") => Method::Notify,
                        Some(m) => { return Err(Error::InvalidMethod(m.to_owned())); }
                        None => { return Err(Error::InvalidMethod("".to_owned())); }
                    };
                    let (headers, ext) = try!(extract_headers(req.headers));
                    Ok(Message {
                        method: method,
                        headers: headers,
                        ext: ext,
                        code: None,
                        reason: None,
                    })
                },
                httparse::Status::Partial => Err(Error::InvalidFormat)
            },
            Err(_) => Err(Error::InvalidPacket),
        }
    }

    fn parse_response(packet: &[u8]) -> Result<Self> {
        let mut headers = [httparse::EMPTY_HEADER; 16];
        let mut res = httparse::Response::new(&mut headers);
        match res.parse(packet) {
            Ok(status) => match status {
                httparse::Status::Complete(_) => {
                    let (headers, ext) = try!(extract_headers(res.headers));
                    Ok(Message {
                        method: Method::Response,
                        headers: headers,
                        ext: ext,
                        code: res.code,
                        reason: Some(res.reason.unwrap().to_owned()),
                    })
                },
                httparse::Status::Partial => Err(Error::InvalidFormat)
            },
            Err(_) => Err(Error::InvalidPacket),
        }
    }

    pub fn new(packet: &[u8]) -> Result<Self> {
        let r = Self::parse_request(packet);
        if let Err(Error::InvalidPacket) = r {
            return Self::parse_response(packet);
        }
        r
    }
}

