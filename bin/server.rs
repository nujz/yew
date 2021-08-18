use bytes::{BufMut, BytesMut};
use futures::StreamExt;
use std::{io, option::Option, result::Result};
use tokio::{
    net::{TcpListener, TcpStream},
    select,
    sync::oneshot,
};
use tokio_util::codec::{Decoder, Encoder, Framed};
use yew::server::Channel;

#[tokio::main]
async fn main() {
    // 断网后, 重启前 失效
    let lst = TcpListener::bind("0.0.0.0:11999").await.unwrap();
    loop {
        let (conn, _) = lst.accept().await.unwrap();

        tokio::spawn(async move {
            let mut server = yew::server::new::<TcpStream, Request, Response>(conn);
            loop {
                match server.accept().await {
                    Ok(channel) => {
                        process(channel);
                    }
                    Err(_) => {
                        // println!("[server] close!! {:?}", err);
                        break;
                    }
                }
            }

            // println!("[server] connection close");
        });
    }
}

fn process(mut channel: Channel<Request, Response>) {
    tokio::spawn(async move {
        // let id = channel.get_id();
        // println!("[server] channel[{}] open", id);

        if let Some(Ok(Request::Connect(addr))) = channel.next().await {
            if let Ok(conn) = TcpStream::connect(addr).await {
                let transport = Framed::new(conn, SimpleServerCodec::new());
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
            }
        }

        // println!("[server] channel[{}] close", id);
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
