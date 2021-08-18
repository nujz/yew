
build:
	RUSTFLAGS='-C target-feature=+crt-static' cargo build --release --bin server --target x86_64-unknown-linux-musl
	cargo build --release --bin client --target x86_64-pc-windows-gnu

	# cp target/x86_64-unknown-linux-musl/release/server ~/udisk/proxy-rs/server
	# cp target/x86_64-pc-windows-gnu/release/client.exe ~/udisk/proxy-rs/proxy.exe
