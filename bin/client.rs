use bytes::{BufMut, BytesMut};
use futures::{SinkExt, StreamExt};
use std::{io, option::Option, result::Result};
use tokio::{
    net::{TcpListener, TcpStream},
    select,
    sync::oneshot,
};
use tokio_util::codec::{Decoder, Encoder, Framed};
use yew::client::Channel;

#[tokio::main]
async fn main() {
    // TODO 重连机制
    let server_addr = "127.0.0.1:11999";

    let conn = TcpStream::connect(server_addr).await.unwrap();
    let mut client = yew::client::new::<TcpStream, Request, Response>(conn);

    // 断网即使重连后, 监听也失效
    let lst = TcpListener::bind("0.0.0.0:1080").await.unwrap();
    loop {
        let (conn, _) = lst.accept().await.unwrap();

        let mut result = client.connect();
        if result.is_err() {
            let conn = TcpStream::connect(server_addr).await.unwrap();
            client = yew::client::new::<TcpStream, Request, Response>(conn);
            result = client.connect();
        }

        if let Ok(channel) = result {
            process(conn, channel);
        }
    }
}

fn process(mut conn: TcpStream, mut channel: Channel<Request, Response>) {
    tokio::spawn(async move {
        if let Ok((host, port)) = yew::socks::handshake(&mut conn).await {
            let addr = format!("{}:{}", host, port);

            channel.send(Request::Connect(addr)).await.unwrap();
            let transport = Framed::new(conn, SimpleClientCodec::new());
            let (sink, stream) = transport.split();
            let (sink2, strem2) = channel.split();

            let (mut close_tx, mut close_rx) = oneshot::channel::<bool>();

            let j = tokio::spawn(async move {
                select! {
                    _ = strem2.forward(sink) => {}
                    _ = close_tx.closed() => {}
                }
            });

            let _ = stream.forward(sink2).await;
            close_rx.close();
            let _ = j.await;

            // println!("[client] complete request");
        }
    });
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub enum Request {
    Connect(String),
    Data(Vec<u8>),
}
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Response {
    data: Vec<u8>,
}

pub struct SimpleServerCodec(());

impl SimpleServerCodec {
    pub fn new() -> Self {
        Self(())
    }
}

impl Decoder for SimpleServerCodec {
    type Item = Response;
    type Error = io::Error;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Response>, io::Error> {
        if !buf.is_empty() {
            Ok(Some(Response {
                data: buf.split().to_vec(),
            }))
        } else {
            Ok(None)
        }
    }
}

impl Encoder<Request> for SimpleServerCodec {
    type Error = io::Error;

    fn encode(&mut self, data: Request, buf: &mut BytesMut) -> Result<(), io::Error> {
        if let Request::Data(data) = data {
            buf.reserve(data.len());
            buf.put_slice(data.as_slice());
            return Ok(());
        }
        Err(io::Error::new(io::ErrorKind::Other, "err"))
    }
}

pub struct SimpleClientCodec(());

impl SimpleClientCodec {
    pub fn new() -> Self {
        Self(())
    }
}

impl Decoder for SimpleClientCodec {
    type Item = Request;
    type Error = io::Error;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Request>, io::Error> {
        if !buf.is_empty() {
            Ok(Some(Request::Data(buf.split().to_vec())))
        } else {
            Ok(None)
        }
    }
}

impl Encoder<Response> for SimpleClientCodec {
    type Error = io::Error;

    fn encode(&mut self, data: Response, buf: &mut BytesMut) -> Result<(), io::Error> {
        let v = data.data;
        buf.reserve(v.len());
        buf.put_slice(v.as_slice());
        Ok(())
    }
}
