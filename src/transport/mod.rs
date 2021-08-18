/* pub(crate) mod sealed {
    use futures::{Sink, Stream};
    use std::io;

    pub trait Transport<SinkItem, Item>:
        Stream<Item = io::Result<Item>> + Sink<SinkItem, Error = io::Error>
    {
    }

    impl<T, SinkItem, Item> Transport<SinkItem, Item> for T where
        T: Stream<Item = io::Result<Item>> + Sink<SinkItem, Error = io::Error> + ?Sized
    {
    }
} */

mod serde_transport;
pub use serde_transport::*;

mod safe_codec;
pub use safe_codec::*;
