use super::safe_codec::SafeCodec;
use futures::{Sink, Stream};
use pin_project_lite::pin_project;
use serde::{Deserialize, Serialize};
use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_serde::{formats::Bincode, Framed as SerdeFramed};
use tokio_util::codec::Framed;

// pub type Transport<S, Item, SinkItem> = SerdeFramed<Framed<S, SafeCodec>, Item, SinkItem, Bincode<Item, SinkItem>>;

pin_project! {
    pub struct Transport<S, Item, SinkItem> {
        #[pin]
        inner: SerdeFramed<Framed<S, SafeCodec>, Item, SinkItem, Bincode<Item, SinkItem>>,
    }
}

impl<S, Item, SinkItem> Stream for Transport<S, Item, SinkItem>
where
    S: AsyncRead,
    Item: for<'a> Deserialize<'a>,
{
    type Item = io::Result<Item>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.project().inner.poll_next(cx)
    }
}

impl<S, Item, SinkItem> Sink<SinkItem> for Transport<S, Item, SinkItem>
where
    S: AsyncWrite,
    SinkItem: Serialize,
{
    type Error = io::Error;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.project().inner.poll_ready(cx)
    }

    fn start_send(self: Pin<&mut Self>, item: SinkItem) -> io::Result<()> {
        self.project().inner.start_send(item)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.project().inner.poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.project().inner.poll_close(cx)
    }
}

impl<S, Item, SinkItem> From<S> for Transport<S, Item, SinkItem>
where
    S: AsyncWrite + AsyncRead,
{
    fn from(inner: S) -> Self {
        Transport {
            inner: SerdeFramed::new(Framed::new(inner, SafeCodec::new()), Bincode::default()),
        }
    }
}
