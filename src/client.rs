use super::{transport::Transport, Request, Response};

use futures::{ready, Future, Sink, Stream};
use pin_project_lite::pin_project;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fmt::Debug,
    io,
    option::Option,
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    task::{Context, Poll},
};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::mpsc::{self, UnboundedReceiver, UnboundedSender},
};

#[derive(Debug)]
enum Message<Req, Resp> {
    Open {
        id: usize,
        sender: UnboundedSender<Resp>,
    },
    Data {
        id: usize,
        message: Req,
    },
    Close {
        id: usize,
    },
}

pub fn new<S, Req, Resp>(io: S) -> Client<Req, Resp>
where
    S: AsyncWrite + AsyncRead + Send + 'static,
    Req: Serialize + Send + 'static,
    Resp: for<'a> Deserialize<'a> + Send + 'static,
{
    let inner = Transport::from(io);

    let (sender, receiver) = mpsc::unbounded_channel();

    let fut = Dispatchor {
        inner,
        receiver,
        senders: HashMap::new(),
    };

    tokio::spawn(async {
        let _ = fut.await;
        // println!("[client] connection closed: {:?}", result);
    });

    Client {
        next_id: Arc::new(AtomicUsize::new(1)),
        sender,
    }
}

//
// Dispatchor
//

pin_project! {
    struct Dispatchor<S, Req, Resp> {
        #[pin]
        inner: Transport<S, Response<Resp>, Request<Req>>,

        #[pin]
        receiver: UnboundedReceiver<Message<Req, Resp>>,

        senders: HashMap<usize, UnboundedSender<Resp>>,
    }
}

impl<S, Req, Resp> Dispatchor<S, Req, Resp>
where
    S: AsyncWrite + AsyncRead,
    Req: Serialize,
    Resp: for<'a> Deserialize<'a>,
{
    fn read_half(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<io::Result<()>>> {
        let p: Poll<Option<Response<Resp>>> = self.as_mut().project().inner.poll_next(cx)?;
        Poll::Ready(match ready!(p) {
            Some(response) => {
                let id = response.id;

                if let Some(tx) = self.as_mut().project().senders.get_mut(&id) {
                    // channel 可能关闭, 忽略错误
                    let _ = tx.send(response.message);
                }

                Some(Ok(()))
            }
            None => None,
        })
    }

    fn write_half(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<io::Result<()>>> {
        let mut inner: Pin<&mut Transport<S, Response<Resp>, Request<Req>>> =
            self.as_mut().project().inner;

        //      let a: Poll<()> = inner.as_mut().poll_ready(cx)?;
        while let Poll::Pending = inner.as_mut().poll_ready(cx)? {
            ready!(inner.as_mut().poll_flush(cx)?);
        }

        let result: Option<Message<Req, Resp>> =
            ready!(self.as_mut().project().receiver.poll_recv(cx));

        Poll::Ready(match result {
            Some(request) => match request {
                Message::Open { id, sender } => {
                    self.as_mut().project().senders.insert(id, sender);

                    // TODO
                    self.as_mut()
                        .project()
                        .inner
                        .start_send(Request::Open { id })?;

                    ready!(self.as_mut().project().inner.poll_flush(cx)?);

                    Some(Ok(()))
                }
                Message::Data { id, message } => {
                    // let result: () = self.as_mut().project().inner.start_send(Request::Data { id, message })?;
                    self.as_mut()
                        .project()
                        .inner
                        .start_send(Request::Data { id, message })?;

                    ready!(self.as_mut().project().inner.poll_flush(cx)?);

                    Some(Ok(()))
                }
                Message::Close { id } => {
                    self.as_mut().project().senders.remove(&id);

                    self.as_mut()
                        .project()
                        .inner
                        .start_send(Request::Cancel { id })?;

                    ready!(self.as_mut().project().inner.poll_flush(cx)?);

                    Some(Ok(()))
                }
            },
            None => None,
        })
    }
}

impl<S, Req, Resp> Future for Dispatchor<S, Req, Resp>
where
    S: AsyncWrite + AsyncRead,
    Req: Serialize,
    Resp: for<'a> Deserialize<'a>,
{
    type Output = anyhow::Result<()>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        loop {
            let result = (self.as_mut().read_half(cx), self.as_mut().write_half(cx));

            // println!("[client] {:?}", result);

            match result {
                (Poll::Ready(None), _) => {
                    // println!("[client] Read none");
                    return Poll::Ready(Ok(()));
                }
                (read, Poll::Ready(None)) => {
                    // println!("[client] Write none");
                    // println!("[client] is empty? {:?}", self.as_mut().senders.is_empty());

                    if self.as_mut().project().senders.is_empty() {
                        return Poll::Ready(Ok(()));
                    }

                    match read {
                        Poll::Ready(Some(Ok(()))) => {}
                        _ => return Poll::Pending,
                    }
                }
                (Poll::Ready(Some(Ok(()))), _) | (_, Poll::Ready(Some(Ok(())))) => {}
                (_, Poll::Ready(Some(Err(_)))) => {
                    // println!("[client] error!!! {}", e);

                    return Poll::Ready(Ok(()));
                }
                _ => {
                    return Poll::Pending;
                }
            }
        }
    }
}

///
///  Client
///

pub struct Client<Req, Resp> {
    next_id: Arc<AtomicUsize>,                   // new id
    sender: UnboundedSender<Message<Req, Resp>>, // clone on new channel
}

impl<Req, Resp> Client<Req, Resp> {
    fn next_id(&self) -> usize {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    pub fn connect(&mut self) -> io::Result<Channel<Req, Resp>> {
        let id = self.next_id();
        let (sender, receiver) = mpsc::unbounded_channel();

        // open
        match self.sender.send(Message::Open { id, sender }) {
            Ok(_) => {
                let sender = self.sender.clone();
                Ok(Channel {
                    id,
                    sender,
                    receiver,
                })
            }
            Err(e) => Err(io::Error::new(io::ErrorKind::Other, e.to_string())),
        }
    }
}

///
/// Channel
///

pub struct Channel<Req, Resp> {
    id: usize,
    sender: UnboundedSender<Message<Req, Resp>>, // send to BaseChannel
    receiver: UnboundedReceiver<Resp>,           // receive from BaseChannel
}

impl<Req, Resp> Drop for Channel<Req, Resp> {
    fn drop(&mut self) {
        // close
        let _ = self.sender.send(Message::Close { id: self.id });
    }
}

impl<Req, Resp> Stream for Channel<Req, Resp> {
    type Item = io::Result<Resp>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Ready(match ready!(self.as_mut().receiver.poll_recv(cx)) {
            Some(resp) => Some(Ok(resp)),
            None => None,
        })
    }
}

impl<Req, Resp> Sink<Req> for Channel<Req, Resp> {
    type Error = io::Error;

    fn poll_ready(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn start_send(mut self: Pin<&mut Self>, item: Req) -> Result<(), Self::Error> {
        let msg = Message::Data {
            id: self.id,
            message: item,
        };

        self.as_mut()
            .sender
            .send(msg)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
}
