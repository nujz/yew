
    use std::net::{Ipv4Addr, Ipv6Addr};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpStream,
    };

    pub async fn handshake(socket: &mut TcpStream) -> std::io::Result<(String, u16)> {
        let mut buf = [0];

        // version
        socket.read(&mut buf).await.unwrap();
        if buf[0] != v5::VERSION {
            return Err(std::io::Error::new(std::io::ErrorKind::Other, ""));
        }

        // methods
        socket.read(&mut buf).await.unwrap();
        let mut methods = vec![0; buf[0] as usize];
        socket.read_exact(&mut methods).await.unwrap();
        if !methods.contains(&v5::METH_NO_AUTH) {
            return Err(std::io::Error::new(std::io::ErrorKind::Other, ""));
        }

        // [ varify username/password result ]
        socket
            .write(&[v5::VERSION, v5::METH_NO_AUTH])
            .await
            .unwrap();

        // ack
        socket.read(&mut buf).await.unwrap();
        if buf[0] != v5::VERSION {
            return Err(std::io::Error::new(std::io::ErrorKind::Other, ""));
        }

        // cmd
        socket.read(&mut buf).await.unwrap();
        if buf[0] != v5::CMD_CONNECT {
            return Err(std::io::Error::new(std::io::ErrorKind::Other, ""));
        }

        // ignore
        socket.read(&mut buf).await.unwrap();

        // host port
        socket.read(&mut buf).await.unwrap();

        let ret;
        match buf[0] {
            v5::ATYP_IPV4 => {
                let mut buf = [0; 6];
                socket.read_exact(&mut buf).await.unwrap();

                let host = Ipv4Addr::new(buf[0], buf[1], buf[2], buf[3]).to_string();
                let port = ((buf[4] as u16) << 8) | (buf[5] as u16);

                ret = (host, port);
            }
            v5::ATYP_IPV6 => {
                let mut buf = [0; 18];
                socket.read_exact(&mut buf).await.unwrap();
                let a = ((buf[0] as u16) << 8) | (buf[1] as u16);
                let b = ((buf[2] as u16) << 8) | (buf[3] as u16);
                let c = ((buf[4] as u16) << 8) | (buf[5] as u16);
                let td = ((buf[6] as u16) << 8) | (buf[7] as u16);
                let e = ((buf[8] as u16) << 8) | (buf[9] as u16);
                let f = ((buf[10] as u16) << 8) | (buf[11] as u16);
                let g = ((buf[12] as u16) << 8) | (buf[13] as u16);
                let h = ((buf[14] as u16) << 8) | (buf[15] as u16);
                let host = Ipv6Addr::new(a, b, c, td, e, f, g, h).to_string();
                let port = ((buf[16] as u16) << 8) | (buf[17] as u16);

                ret = (host, port);
            }
            v5::ATYP_DOMAIN => {
                socket.read(&mut buf).await.unwrap();
                let mut bytes = vec![0; buf[0] as usize];
                socket.read_exact(&mut bytes).await.unwrap();
                let host = String::from_utf8(bytes).unwrap();

                let mut port = [0; 2];
                socket.read_exact(&mut port).await.unwrap();
                let port = ((port[0] as u16) << 8) | (port[1] as u16);

                ret = (host, port);
            }
            _ => return Err(std::io::Error::new(std::io::ErrorKind::Other, "")),
        }
        socket
            .write_all(&mut [5, 0, 0, 1, 0, 0, 0, 0, 0, 0])
            .await
            .unwrap();

        Ok(ret)
    }

    mod v5 {
        pub const VERSION: u8 = 5;

        pub const METH_NO_AUTH: u8 = 0;
        // pub const METH_GSSAPI: u8 = 1;
        // pub const METH_USER_PASS: u8 = 2;

        pub const CMD_CONNECT: u8 = 1;
        // pub const CMD_BIND: u8 = 2;
        // pub const CMD_UDP_ASSOCIATE: u8 = 3;

        pub const ATYP_IPV4: u8 = 1;
        pub const ATYP_IPV6: u8 = 4;
        pub const ATYP_DOMAIN: u8 = 3;
    }
