//! A minimal static file server, enough for Cargo's sparse index client.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::thread;

pub struct Server {
    pub base_url: String,
}

impl Server {
    /// Serves `root` on a free loopback port until the process exits.
    pub fn start(root: PathBuf) -> anyhow::Result<Server> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let base_url = format!("http://{}/", listener.local_addr()?);
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let root = root.clone();
                thread::spawn(move || {
                    let _ = handle(stream, &root);
                });
            }
        });
        Ok(Server { base_url })
    }
}

fn handle(mut stream: TcpStream, root: &Path) -> std::io::Result<()> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 1024];
    while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
        let n = stream.read(&mut chunk)?;
        if n == 0 {
            return Ok(());
        }
        buf.extend_from_slice(&chunk[..n]);
        if buf.len() > 64 * 1024 {
            return Ok(());
        }
    }
    let head = String::from_utf8_lossy(&buf);
    let mut parts = head.lines().next().unwrap_or("").split_whitespace();
    let method = parts.next().unwrap_or("");
    let target = parts.next().unwrap_or("/");
    let path = target
        .split('?')
        .next()
        .unwrap_or("/")
        .trim_start_matches('/');
    let safe = !path.split('/').any(|seg| seg == "..");
    let file = root.join(path);
    let response = if method == "GET" && safe && file.is_file() {
        let body = std::fs::read(&file)?;
        let mut r = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/octet-stream\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .into_bytes();
        r.extend_from_slice(&body);
        r
    } else {
        b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec()
    };
    stream.write_all(&response)?;
    stream.flush()
}
