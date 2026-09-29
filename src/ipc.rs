//! Protocolo mínimo: cada conexión TCP a 127.0.0.1:PORT manda líneas JSON y cierra.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::time::Duration;

use serde_json::Value;
use winit::event_loop::EventLoopProxy;

use crate::app::UserEvent;

fn addr() -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], crate::PORT))
}

/// Falla si ya hay otra instancia escuchando.
pub fn bind() -> Option<TcpListener> {
    TcpListener::bind(addr()).ok()
}

pub fn serve(listener: TcpListener, proxy: EventLoopProxy<UserEvent>) {
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
            let mut buf = String::new();
            if stream.take(1 << 20).read_to_string(&mut buf).is_err() {
                continue;
            }
            for line in buf.lines() {
                if let Ok(v) = serde_json::from_str::<Value>(line) {
                    if proxy.send_event(UserEvent::Hook(v)).is_err() {
                        return;
                    }
                }
            }
        }
    });
}

pub fn send(msg: &Value, timeout: Duration) -> bool {
    let Ok(mut s) = TcpStream::connect_timeout(&addr(), timeout) else {
        return false;
    };
    let _ = s.set_write_timeout(Some(timeout));
    let mut line = msg.to_string();
    line.push('\n');
    s.write_all(line.as_bytes()).is_ok()
}

pub fn send_quit() {
    send(&serde_json::json!({ "cmd": "quit" }), Duration::from_millis(500));
}
