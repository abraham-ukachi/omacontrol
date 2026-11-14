//! A small blocking WebSocket helper shared by the Samsung and LG remotes.

use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Connector, Message, WebSocket};

use super::tls::local_tv_config;
use crate::{Error, Result};

pub(crate) type Socket = WebSocket<MaybeTlsStream<TcpStream>>;

/// Opens `ws://` or `wss://` (any certificate) to a TV, with a read timeout.
pub(crate) fn open(url: &str, host_port: &str, read_timeout: Duration) -> Result<Socket> {
    let addr = host_port
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| Error::Protocol(format!("no address for {host_port}")))?;
    let stream = TcpStream::connect_timeout(&addr, Duration::from_secs(3))?;
    stream.set_read_timeout(Some(read_timeout))?;
    stream.set_nodelay(true)?;
    let connector = Connector::Rustls(local_tv_config());
    let (socket, _) = tungstenite::client_tls_with_config(url, stream, None, Some(connector))
        .map_err(|e| Error::WebSocket(e.to_string()))?;
    Ok(socket)
}

/// Reads the next text message (skipping pings), as JSON.
pub(crate) fn read_json(socket: &mut Socket) -> Result<serde_json::Value> {
    loop {
        match socket.read()? {
            Message::Text(text) => return Ok(serde_json::from_str(&text)?),
            Message::Close(_) => return Err(Error::Protocol("the TV closed the connection".into())),
            _ => continue,
        }
    }
}

/// Sends one JSON message.
pub(crate) fn send_json(socket: &mut Socket, value: &serde_json::Value) -> Result<()> {
    socket.send(Message::Text(value.to_string()))?;
    Ok(())
}
