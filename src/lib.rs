pub mod transport;

pub mod client;
pub mod server;
pub mod socks;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
enum Request<T> {
    Open { id: usize },
    Data { id: usize, message: T },
    Cancel { id: usize },
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct Response<T> {
    id: usize,
    message: T,
}
