use error::Error;
use message::{ Message, Method };
use std::collections::HashMap;
use std::sync::{ Arc, Condvar, Mutex };
use std::thread;
use time::now_utc;
use transport::Transport;

struct CacheEntry {
    msg: Message,
    desc: String
}

#[derive(Clone)]
pub struct Manager {
    transport: Arc<Transport>,
    cache: Arc<Mutex<HashMap<String, CacheEntry>>>,
    read: Arc<(Mutex<Vec<String>>, Condvar)>,
}

impl Manager {
    pub fn new() -> Self {
        Manager {
            transport: Arc::new(Transport::new()),
            cache: Arc::new(Mutex::new(HashMap::new())),
            read: Arc::new((Mutex::new(Vec::new()), Condvar::new())),
        }
    }

    fn start_transport(&self) {
        let mgr = self.clone();
        thread::spawn(move || {
            loop {
                match mgr.transport.recv() {
                    Ok(msg) => {
                        match msg.method {
                            Method::Notify | Method::Response => {
                                mgr.fetch_description(msg);
                            },
                            Method::Msearch => { }
                        }
                    },
                    Err(e) => {
                        warn!("transport error: {:?}", e);
                        match e {
                            // FIXME: notify listener
                            Error::IOError(_) => { return; }
                            _ => { }
                        }
                    },
                };
            }
        });
    }

    fn has_cache_expired(&self, msg: &Message) -> bool {
        let usn = msg.ext.usn.as_ref().unwrap();
        match self.cache.lock().unwrap().get(usn) {
            Some(ce) => ce.msg.ext.expires < msg.ext.expires ||
                        ce.msg.ext.location != msg.ext.location,
            None => true,
        }
    }

    fn update_cache(&self, msg: Message, desc: String) {
        let usn = msg.ext.usn.as_ref().unwrap().clone();
        self.cache.lock().unwrap().insert(usn, CacheEntry {
            msg: msg,
            desc: desc,
        });
    }

    fn notify_read(&self, usn: String) {
        let &(ref lock, ref cvar) = &*self.read;
        lock.lock().unwrap().push(usn);
        cvar.notify_one();
    }

    fn clear_cache_expired(&self) {
        let mut cache_lock = self.cache.lock().unwrap();
        let now = now_utc();
        let mut expired = Vec::new();
        for (usn, entry) in cache_lock.iter() {
            if entry.msg.ext.expires < now {
                expired.push(usn.clone());
            }
        }
        for usn in expired {
            cache_lock.remove(&usn);
        }
    }

    pub fn search(&self, target: Option<String>, attempts: Option<u16>) {
        let mut remaining = attempts.unwrap_or(3);
        let st = target.unwrap_or("ssdp:all".to_owned());
        let mgr = self.clone();
        thread::spawn(move || {
            while remaining > 0 {
                remaining -= 1;
                let _ = mgr.transport.send_msearch(&st);
            }
        });
    }

    pub fn read(&self) -> Option<(Message, String)> {
        let &(ref lock, ref cvar) = &*self.read;
        let usn;
        {
            let mut devices = lock.lock().unwrap();
            while devices.is_empty() {
                devices = cvar.wait(devices).unwrap();
            }
            usn = devices.remove(0);
        }
        let ret = match self.cache.lock().unwrap().get(&usn) {
            Some(ce) => Some((ce.msg.clone(), ce.desc.clone())),
            None => None,
        };
        self.clear_cache_expired();
        ret
    }

    fn fetch_description(&self, msg: Message) {
        if !self.has_cache_expired(&msg) {
            debug!("drop {:?}, cache still valid", msg);
            return;
        }

        let mgr = self.clone();
        thread::spawn(move || {
            match Transport::fetch_description(&msg) {
                Ok(desc) => {
                    let usn = msg.ext.usn.as_ref().unwrap().clone();
                    mgr.update_cache(msg, desc);
                    mgr.notify_read(usn);
                },
                Err(e) => {
                    warn!("fetch description error: {:?}", e);
                }
            }
        });
    }

    pub fn start(&self) {
        self.start_transport();
    }

    pub fn stop(&self) {
        self.notify_read("eof".to_owned());
    }
}

