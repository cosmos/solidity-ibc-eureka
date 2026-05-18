use std::sync::Arc;

use alloy::providers::{Provider, RootProvider};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::TlsAcceptor;

const TEST_CERT: &str = r#"-----BEGIN CERTIFICATE-----
MIIDHzCCAgegAwIBAgIUISS88j94jMu7PR25D7BgciB3J4EwDQYJKoZIhvcNAQEL
BQAwFDESMBAGA1UEAwwJbG9jYWxob3N0MB4XDTI2MDUxODEzMDMzM1oXDTM2MDUx
NTEzMDMzM1owFDESMBAGA1UEAwwJbG9jYWxob3N0MIIBIjANBgkqhkiG9w0BAQEF
AAOCAQ8AMIIBCgKCAQEArVeS42r4l30ICCCVqeba4QFGrBuBidNr2qGNWU7oBlO1
lRxkrPyRywNOBioD7miURA6zVPEptnS6hpFg47LwF5VPPGRKc+wQ4Jxog1aDEFHB
oJsldQnotHiuqwzkBfX0gdkb8WcVTDHqwTNYS3qIPTKhYbOvQ1USh53Tt0dc5wxN
yDLUWscJIwES/9ed+fYny0HeiF8NDauKyvKIS3ZIv6cJRl2vo2zdq//cJc9S5s1m
vD6MhUYYzhIC1afmxIQPZjcLrGov5QQpPpL2XrCn6b1iAeQAK4/rOhzewjAvFUJz
m0gCMqYtlg0uv6p9ozsXVpIyiAbOOwsLOaoz/HCLJQIDAQABo2kwZzAdBgNVHQ4E
FgQUCEKs9WR5r9KIyGAAZYMzIhQR920wHwYDVR0jBBgwFoAUCEKs9WR5r9KIyGAA
ZYMzIhQR920wDwYDVR0TAQH/BAUwAwEB/zAUBgNVHREEDTALgglsb2NhbGhvc3Qw
DQYJKoZIhvcNAQELBQADggEBAFaTM8BzKTdjFo6pKHYjvon9pGgiI6il81q4cozj
n/cnRH/BTJcXTInP33uCT66QLGVjemwOn9dIhn0g5rCpCGdrxJk98yWB610stzct
FAdfuIPVL6JbuzQqJ9TeX1kYcxNqUY5G5/9vyEK2ncLQtdQ7IIwOkvPIplaO0kyR
av6169DDb0yph2Xh6tgEeu/8lSo8zHHYjX2vyjt/+kLx04JMyQGe/iS0S3kL9+VB
PhKiZD2xubcscmAScA4isRe+o86L3qnFze/NkTBZ33eq6BE0fXJmhLw4X+ADCidm
USSTFwrZadixOEzepGbzn5/iSXqUW+8YdSe96lSEfYUP24A=
-----END CERTIFICATE-----"#;

const TEST_KEY: &str = r#"-----BEGIN PRIVATE KEY-----
MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQCtV5LjaviXfQgI
IJWp5trhAUasG4GJ02vaoY1ZTugGU7WVHGSs/JHLA04GKgPuaJREDrNU8Sm2dLqG
kWDjsvAXlU88ZEpz7BDgnGiDVoMQUcGgmyV1Cei0eK6rDOQF9fSB2RvxZxVMMerB
M1hLeog9MqFhs69DVRKHndO3R1znDE3IMtRaxwkjARL/15359ifLQd6IXw0Nq4rK
8ohLdki/pwlGXa+jbN2r/9wlz1LmzWa8PoyFRhjOEgLVp+bEhA9mNwusai/lBCk+
kvZesKfpvWIB5AArj+s6HN7CMC8VQnObSAIypi2WDS6/qn2jOxdWkjKIBs47Cws5
qjP8cIslAgMBAAECggEATQL9zwzc8gzKDzZO1+OpNdMHz3oO9KlaHxGsR1PPsNPq
9hrdvZ8etNe8h++NvJN43726PdBBLH8yyYt4ROFgWtHqmJWkIFlubCQBKOy8IPl5
sX2MSDHFUbzWOUdqXR2XakMHb5pRM39v4TMLFMmVEr9WRJ58jMkUiOz9PU22wC0S
Zw9+6QRi0jqda8BXWS2ChUTLjmHgchfbSopgKqcuXy5J2wkfZd+VjNwA2YE+4o49
f6IZpNT7QTgSBvWYDzPBd4MbfrFQsq1zDglw8gHrIGoG1dOb65NXtG0UuwEmWhv7
ZspbkC1/AJt9JRvTI2+dH85U5iVzi/0Nz9qsF+GJ9wKBgQDrJrc5p0UZcvj+c9OY
FlZ9Y+9rjNdjTLc2Ii/4P2EakSln1W6HVdYCD0p+6jMugPiuAUBdTPON+b7RDDlc
ZxsUE1NN+ldnRQ6sETcyMJCYvWOYNkbL3Gq3CdoQqnp+3U/571uIzG970F1yycqw
NFVu6H3jB9yBZivT2Oyj+sXlWwKBgQC8tfX+LmGaqTfWqeuY4wpOJxd/oO9db+eX
ezkwxQHk87jKAFzVLlib+oaeRyhhOXNtCNfZBW+juRILyDIHDd9a27JVMmcN2u6I
QWwlf3ETDVbNFKAjhwozShKRveOUskwiYkuGN9RcMjkAUcexIOuxe3qo1avceud5
WNl8up+5fwKBgCocDMOuSJl+FUi7xTB/D430z3MbDZcircxr4ts5vlHbnOaTOe/S
VziXNGf12KSDRAUlBDmxBgxupNUas0rwy9CUhV55bh14KGB31MKQH9S6VZN0ZMks
948/nGVPohAF4cSIMIQW1//8GD/uEvXq6UGrMJ/vUHV7rm2JcxcKWVXxAoGAOZDo
cZmEfMGSHxHGpOTdh+m06WIWotI4O6WDVLVEVqtie3yQ1RdGQI6z8iSS1NXJuEEy
VympXLBgKOfxGuXIdNcAF61AcqGOklIiw22U+NLg376i+zN+kRbPg1YBzqJ1Ryfl
EhSF23oWmVpZKFd6jrG0QTytKJz1b7cC4WGsx9ECgYEAmX0ZCvvfOSLqcbvTtC/m
F3BD4PBKEkjNtOqo0GX8cvXxZ/0o+M8vAYscfhP1UMCnyq+oY+S/C6gA0ePZ085S
kqJxOwXLVbRSwX2vPfEhX/SRO4xhLBn0gOSAiJg1cWuaMuaYGi1Ua+Wz86u60Lf6
YtH4ydZmeVFjIz9bSTsLiqQ=
-----END PRIVATE KEY-----"#;

#[tokio::test]
async fn https_provider_can_fetch_chain_id() {
    let _ = tokio_rustls::rustls::crypto::ring::default_provider().install_default();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let acceptor = tls_acceptor();

    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut stream = acceptor.accept(stream).await.unwrap();
        read_http_request(&mut stream).await;

        let body = r#"{"jsonrpc":"2.0","id":1,"result":"0x1"}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let client = reqwest_0_13::Client::builder()
        .use_rustls_tls()
        .danger_accept_invalid_certs(true)
        .build()
        .unwrap();
    let provider: RootProvider = RootProvider::builder()
        .connect_reqwest(client, format!("https://localhost:{port}").parse().unwrap());

    assert_eq!(provider.get_chain_id().await.unwrap(), 1);
    server.await.unwrap();
}

fn tls_acceptor() -> TlsAcceptor {
    let mut cert_reader = TEST_CERT.as_bytes();
    let certs = rustls_pemfile::certs(&mut cert_reader)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    let mut key_reader = TEST_KEY.as_bytes();
    let key = rustls_pemfile::private_key(&mut key_reader)
        .unwrap()
        .unwrap();

    let config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .unwrap();

    TlsAcceptor::from(Arc::new(config))
}

async fn read_http_request<T>(stream: &mut T)
where
    T: AsyncReadExt + Unpin,
{
    let mut request = Vec::new();
    let mut buf = [0; 1024];

    loop {
        let n = stream.read(&mut buf).await.unwrap();
        assert_ne!(n, 0, "client closed connection before sending request");
        request.extend_from_slice(&buf[..n]);

        let Some(header_end) = request.windows(4).position(|w| w == b"\r\n\r\n") else {
            continue;
        };

        let headers = std::str::from_utf8(&request[..header_end]).unwrap();
        let content_length = headers
            .lines()
            .find_map(|line| line.strip_prefix("content-length: "))
            .or_else(|| {
                headers
                    .lines()
                    .find_map(|line| line.strip_prefix("Content-Length: "))
            })
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or_default();

        if request.len() >= header_end + 4 + content_length {
            assert!(request.windows(11).any(|w| w == b"eth_chainId"));
            return;
        }
    }
}
