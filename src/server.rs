use super::transport::Transport;
use super::Request;
use super::Response;
use futures::{ready, Future, Sink, Stream};
use pin_project_lite::pin_project;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    io,
    option::Option,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::mpsc::{self, UnboundedReceiver, UnboundedSender},
};

#[derive(Debug)]
enum Message<Resp> {
    Data { id: usize, message: Resp },
    Close { id: usize },
}

pub fn new<S, Req, Resp>(io: S) -> Server<Req, Resp>
where
    S: AsyncWrite + AsyncRead + Send + 'static,
    Req: for<'a> Deserialize<'a> + Send + 'static,
    Resp: Serialize + Send + 'static,
{
    let (accept_sender, accept_receiver) = mpsc::unbounded_channel();

    let inner = Transport::from(io);

    let (sender, receiver) = mpsc::unbounded_channel();

    let fut = Dispatchor {
        inner,
        receiver,
        senders: HashMap::new(),
        accept_sender,
    };

    tokio::spawn(async move {
        let _ = fut.await;
        // println!("[server] dispatcher closed");
    });

    Server {
        sender,
        accept_receiver,
    }
}

pin_project! {
    struct Dispatchor<S, Req, Resp> {
        #[pin]
        inner: Transport<S, Request<Req>, Response<Resp>>,

        #[pin]
        receiver: UnboundedReceiver<Message<Resp>>,

        senders: HashMap<usize, UnboundedSender<Request<Req>>>,

        accept_sender: UnboundedSender<(usize, UnboundedReceiver<Request<Req>>)>
    }
}

impl<S, Req, Resp> Dispatchor<S, Req, Resp>
where
    S: AsyncWrite + AsyncRead,
    Req: for<'a> Deserialize<'a>,
    Resp: Serialize,
{
    fn write_half(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<io::Result<()>>> {
        while let Poll::Pending = self.as_mut().project().inner.poll_ready(cx)? {
            ready!(self.as_mut().project().inner.poll_flush(cx)?);
        }

        let result: Option<Message<Resp>> = ready!(self.as_mut().project().receiver.poll_recv(cx));

        Poll::Ready(match result {
            Some(msg) => match msg {
                Message::Data { id, message } => {
                    self.as_mut()
                        .project()
                        .inner
                        .start_send(Response { id, message })?;

                    ready!(self.as_mut().project().inner.poll_flush(cx)?);

                    Some(Ok(()))
                }
                Message::Close { id } => {
                    self.as_mut().project().senders.remove(&id);

                    Some(Ok(()))
                }
            },
            None => None,
        })
    }

    fn read_half(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<io::Result<()>>> {
        let result: Option<Request<Req>> = ready!(self.as_mut().project().inner.poll_next(cx)?);

        Poll::Ready(match result {
            Some(request) => {
                match request {
                    Request::Open { id } => {
                        let (sender, receiver) = mpsc::unbounded_channel();

                        self.as_mut().project().senders.insert(id, sender);

                        self.as_mut()
                            .project()
                            .accept_sender
                            .send((id, receiver))
                            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
                    }
                    Request::Data { id, message } => {
                        if let Some(tx) = self.as_mut().project().senders.get_mut(&id) {
                            tx.send(Request::Data { id, message })
                                .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
                        }
                    }
                    Request::Cancel { id } => {
                        let sender: Option<UnboundedSender<Request<Req>>> =
                            self.as_mut().project().senders.remove(&id);
                        if let Some(tx) = sender {
                            tx.send(Request::Cancel { id })
                                .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
                        }
                    }
                };
                Some(Ok(()))
            }
            None => {
                let senders: &mut HashMap<usize, UnboundedSender<Request<Req>>> =
                    self.as_mut().project().senders;

                let drain = senders.drain();
                for (id, sender) in drain {
                    let _ = sender.send(Request::Cancel { id });
                }

                None
            }
        })
    }
}

impl<S, Req, Resp> Future for Dispatchor<S, Req, Resp>
where
    S: AsyncWrite + AsyncRead,
    Req: for<'a> Deserialize<'a>,
    Resp: Serialize,
{
    type Output = anyhow::Result<()>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> std::task::Poll<Self::Output> {
        loop {
            let result = (self.as_mut().read_half(cx), self.as_mut().write_half(cx));

            // println!("[server] {:?}", result);

            match result {
                (_, Poll::Ready(None)) => {
                    // println!("[server] Write none");

                    return Poll::Ready(Ok(()));
                }
                (Poll::Ready(None), _) => {
                    // println!("[server] Read none");
                    return Poll::Ready(Ok(()));

                    /* println!(
                        "[server] is empty? {:?} write: {:?}",
                        self.as_mut().senders.is_empty(),
                        write
                    );

                    if self.as_mut().senders.is_empty() {
                        return Poll::Ready(Ok(()));
                    }

                    match write {
                        Poll::Ready(Some(Ok(()))) => {}
                        Poll::Ready(Some(Err(e))) => {
                           // println!("err!! {}", e);
                        }
                        _ => return Poll::Pending,
                    }; */
                }
                (Poll::Ready(Some(Ok(()))), _) | (_, Poll::Ready(Some(Ok(())))) => {}
                (Poll::Ready(Some(Err(_))), _) => {
                    // TODO decode error ?
                    // println!("[server] error 1 : {}", e);
                    return Poll::Ready(Ok(()));
                }
                (_, Poll::Ready(Some(Err(_)))) => {
                    // println!("[server] error 2 : {}", e);
                    return Poll::Ready(Ok(()));
                }
                _ => return Poll::Pending,
            }
        }
    }
}

pub struct Server<Req, Resp> {
    sender: UnboundedSender<Message<Resp>>,
    accept_receiver: UnboundedReceiver<(usize, UnboundedReceiver<Request<Req>>)>,
}

impl<Req, Resp> Server<Req, Resp> {
    pub async fn accept(&mut self) -> io::Result<Channel<Req, Resp>> {
        if let Some((id, receiver)) = self.accept_receiver.recv().await {
            let ch = Channel {
                id,
                sender: self.sender.clone(),
                receiver,
            };

            return Ok(ch);
        }

        Err(io::Error::new(io::ErrorKind::Other, "closed"))
    }
}

pub struct Channel<Req, Resp> {
    id: usize,
    sender: UnboundedSender<Message<Resp>>,
    receiver: UnboundedReceiver<Request<Req>>,
}

impl<Req, Resp> Channel<Req, Resp> {
    pub fn get_id(&self) -> usize {
        self.id
    }
}

impl<Req, Resp> Drop for Channel<Req, Resp> {
    fn drop(&mut self) {
        // cancel
        let _ = self.sender.send(Message::Close { id: self.id });
    }
}

impl<Req, Resp> Stream for Channel<Req, Resp> {
    type Item = io::Result<Req>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Ready(match ready!(self.as_mut().receiver.poll_recv(cx)) {
            Some(request) => match request {
                Request::Open { id: _ } => unreachable!(),
                Request::Data { id: _, message } => Some(Ok(message)),
                Request::Cancel { id: _ } => None,
            },
            None => None,
        })
    }
}

impl<Req, Resp> Sink<Resp> for Channel<Req, Resp> {
    type Error = io::Error;

    fn poll_ready(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn start_send(mut self: Pin<&mut Self>, item: Resp) -> Result<(), Self::Error> {
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
