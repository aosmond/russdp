use core::str::FromStr;
use error::{ Error, Result };
use hyper;
use net2::{ UdpBuilder, UdpSocketExt };
use message::Message;
use std::io::Read;
use std::net::{ UdpSocket, Ipv4Addr, SocketAddrV4 };
use std::sync::Arc;

pub struct Transport {
    socket: Arc<UdpSocket>,
}

impl Transport {
    pub fn new() -> Self {
        let builder = UdpBuilder::new_v6().unwrap();
        builder.reuse_address(true).unwrap();
        let socket = builder.bind("[::]:1900").unwrap();

        let multicast_addr4 = Ipv4Addr::from_str("239.255.255.255").unwrap();
        let any_addr4 = Ipv4Addr::from_str("0.0.0.0").unwrap();
        socket.join_multicast_v4(&multicast_addr4, &any_addr4).unwrap();

        // FIXME: add IPv6 support

        Transport {
            socket: Arc::new(socket),
        }
    }

    pub fn recv(&self) -> Result<Message> {
        let mut buf = Box::new(vec![0u8; 2048]);
        match self.socket.recv_from(&mut buf) {
            Ok((len, _)) => {
                buf.truncate(len);
                Message::new(buf.as_slice())
            },
            Err(e) => Err(Error::IOError(e))
        }
    }

    pub fn send_msearch(&self, target: &str) -> Result<()> {
        let packet = format!("M-SEARCH * HTTP/1.1\r\nHOST: {}:{}\r\nMAIN: \"ssdp:discover\"\r\nST: {}\r\nUSER-AGENT: Linux/2.2 UPnP/1.1 russdp/0.1.0\r\n\r\n",
                             "239.255.255.255", 1900, target);
        let dst_addr4 = SocketAddrV4::from_str("239.255.255.255:1900").unwrap();
        match self.socket.send_to(packet.as_bytes(), &dst_addr4) {
            Ok(_) => Ok(()),
            Err(e) => Err(Error::IOError(e)),
        }
    }

    pub fn fetch_description(msg: &Message) -> Result<String> {
        let client = hyper::Client::new();
        let mut res = match client.get(&msg.ext.location).header(hyper::header::Connection::close()).send() {
            Ok(x) => x,
            Err(e) => { return Err(Error::HyperError(e)); }
        };

        let mut body = String::new();
        match res.read_to_string(&mut body) {
            Ok(_) => { },
            Err(e) => { return Err(Error::IOError(e)); }
        };

        Ok(body)
    }
}
