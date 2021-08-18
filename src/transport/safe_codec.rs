use bytes::{BufMut, Bytes, BytesMut};
use ring::{
    aead::{Aad, LessSafeKey, Nonce, UnboundKey, CHACHA20_POLY1305},
    rand::{SecureRandom, SystemRandom},
};
use std::io;
use tokio_util::codec::{Decoder, Encoder, LengthDelimitedCodec};

pub struct SafeCodec {
    inner: LengthDelimitedCodec,
    key: LessSafeKey,
}

impl SafeCodec {
    pub fn new() -> Self {
        let unbound_key = UnboundKey::new(&CHACHA20_POLY1305, &PRIVATE_KEY).unwrap();
        let key = LessSafeKey::new(unbound_key);
        Self {
            inner: LengthDelimitedCodec::new(),
            key,
        }
    }
}

impl Decoder for SafeCodec {
    type Item = BytesMut;
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        // 'encrypted data' + 'nonce'
        // return self.inner.decode(src);

        // codec
        let data = self.inner.decode(src)?;

        // 解密
        match data {
            Some(mut data) => {
                let nonce = data.split_off(data.len() - 12);

                if let Ok(nonce) = Nonce::try_assume_unique_for_key(nonce.as_ref()) {
                    if let Ok(ret) = self.key.open_in_place(nonce, Aad::empty(), &mut data) {
                        let len = ret.len();
                        let data = data.split_to(len);

                        return Ok(Some(data));
                    }
                }

                return Err(io::Error::new(io::ErrorKind::Other, "nonce error"));
            }
            None => {}
        }
        Ok(None)
    }
}

impl Encoder<Bytes> for SafeCodec {
    type Error = io::Error;

    fn encode(&mut self, item: Bytes, dst: &mut BytesMut) -> Result<(), Self::Error> {
        // 'encrypted data' + 'nonce'
        // return self.inner.encode(item, dst);

        let mut buf = BytesMut::from(item.as_ref());

        // nonce
        let rand = SystemRandom::new();
        let mut nonce = [0; 12];
        rand.fill(&mut nonce).unwrap();

        // encrypt
        if let Err(_) = self.key.seal_in_place_append_tag(
            Nonce::assume_unique_for_key(nonce),
            Aad::empty(),
            &mut buf,
        ) {
            return Err(io::Error::new(io::ErrorKind::Other, "encode error"));
        }

        buf.put_slice(&nonce);

        // codec
        self.inner.encode(buf.into(), dst)
    }
}

/* const PRIVATE_KEY: [u8; 32] = [
    220, 56, 180, 32, 105, 143, 199, 54, 209, 108, 65, 140, 67, 107, 220, 12, 222, 129, 71, 73,
    230, 198, 119, 11, 98, 216, 146, 46, 94, 139, 86, 230,
]; */
const PRIVATE_KEY: [u8; 32] = [
    178, 177, 42, 211, 51, 232, 172, 141, 190, 131, 170, 161, 104, 51, 165, 165, 56, 225, 12, 167,
    174, 228, 43, 55, 91, 129, 174, 173, 117, 32, 195, 252,
];
