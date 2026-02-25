use std::collections::VecDeque;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;

const LISTEN_ADDR: &str = "0.0.0.0:80";
const BACKEND_ADDR: &str = "backend:8080";
const MAX_HEADER_SIZE: usize = 8192;
const MAX_CHUNK_SIZE: usize = 16 * 1024 * 1024;
const MAX_BODY_SIZE: usize = 64 * 1024 * 1024;
const POOL_SIZE: usize = 8;

struct ConnPool {
    idle: Mutex<VecDeque<TcpStream>>,
}

impl ConnPool {
    fn new() -> Self {
        Self {
            idle: Mutex::new(VecDeque::with_capacity(POOL_SIZE)),
        }
    }

    async fn acquire(&self) -> tokio::io::Result<TcpStream> {
        if let Some(stream) = self.idle.lock().await.pop_front() {
            return Ok(stream);
        }
        TcpStream::connect(BACKEND_ADDR).await
    }

    async fn release(&self, stream: TcpStream) {
        let mut pool = self.idle.lock().await;
        if pool.len() < POOL_SIZE {
            pool.push_back(stream);
        }
    }
}

struct RequestMeta {
    method: String,
    path: String,
    version: String,
    headers: Vec<(String, String)>,
    is_chunked: bool,
    content_length: Option<usize>,
    client_wants_close: bool,
}


fn is_valid_token(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| matches!(b, b'!' | b'#' | b'$' | b'%' | b'&' | b'\'' | b'*'
                | b'+' | b'-' | b'.' | b'^' | b'_' | b'`' | b'|' | b'~'
                | b'0'..=b'9' | b'A'..=b'Z' | b'a'..=b'z'))
}

fn is_valid_http_version(v: &str) -> bool {
    v == "HTTP/1.0" || v == "HTTP/1.1"
}

async fn parse_request(
    reader: &mut BufReader<tokio::io::ReadHalf<TcpStream>>,
) -> Result<Option<RequestMeta>, &'static str> {
    let mut request_line = String::new();
    let n = reader.read_line(&mut request_line).await.map_err(|_| "read error")?;
    if n == 0 {
        return Ok(None); 
    }

    if !request_line.ends_with("\r\n") {
        return Err("bare LF in request-line");
    }

    let trimmed_line = request_line.trim_end().to_string();
    let parts: Vec<&str> = trimmed_line.splitn(3, ' ').collect();
    if parts.len() != 3 {
        return Err("malformed request-line");
    }

    let method = parts[0].to_string();
    let path = parts[1].to_string();
    let version = parts[2].to_string();

    if !is_valid_token(&method) {
        return Err("invalid method token");
    }
    if !is_valid_http_version(&version) {
        return Err("unsupported HTTP version");
    }

    let mut raw_headers: Vec<(String, String)> = Vec::new();
    let mut total_header_bytes = request_line.len();

    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line).await.map_err(|_| "read error")?;
        if n == 0 {
            return Err("unexpected EOF in headers");
        }
        total_header_bytes += n;
        if total_header_bytes > MAX_HEADER_SIZE {
            return Err("header section too large");
        }

        if !line.ends_with("\r\n") {
            return Err("bare LF in header line");
        }

        if line.starts_with(' ') || line.starts_with('\t') {
            return Err("obs-fold in header");
        }

        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            break;
        }

        let colon_pos = match trimmed.find(':') {
            Some(p) => p,
            None => return Err("header line missing colon"),
        };
        let field_name = &trimmed[..colon_pos];
        if field_name.ends_with(' ') || field_name.ends_with('\t') {
            return Err("whitespace before colon in header");
        }
        if !is_valid_token(field_name) {
            return Err("invalid header field-name");
        }

        let name = field_name.to_string();
        let value = trimmed[colon_pos + 1..].trim().to_string();
        raw_headers.push((name, value));
    }

    let mut is_chunked = false;
    let mut content_length: Option<usize> = None;
    let mut cl_count = 0usize;
    let mut client_wants_close = false;
    let mut hop_by_hop_names: Vec<String> = Vec::new();

    for (name, value) in &raw_headers {
        let lower = name.to_lowercase();
        if lower == "transfer-encoding" {
            let codings: Vec<&str> = value.split(',').map(|s| s.trim()).collect();
            match codings.last() {
                Some(last) if last.eq_ignore_ascii_case("chunked") => {
                    is_chunked = true;
                }
                _ => return Err("unsupported transfer-coding"),
            }
        }
        if lower == "content-length" {
            let val: usize = value.parse().map_err(|_| "invalid Content-Length")?;
            if val > MAX_BODY_SIZE {
                return Err("Content-Length exceeds limit");
            }
            if cl_count > 0 && content_length != Some(val) {
                return Err("conflicting Content-Length values");
            }
            content_length = Some(val);
            cl_count += 1;
        }
        if lower == "connection" {
            for tok in value.split(',') {
                let tok = tok.trim().to_lowercase();
                if tok == "close" {
                    client_wants_close = true;
                }
                hop_by_hop_names.push(tok);
            }
        }
    }

    if is_chunked && content_length.is_some() {
        content_length = None;
    }

    let mut headers: Vec<(String, String)> = Vec::new();
    let mut cl_forwarded = false;

    for (name, value) in &raw_headers {
        let lower = name.to_lowercase();

        if lower == "connection" {
            continue;
        }
        if hop_by_hop_names.contains(&lower) {
            continue;
        }

        if lower == "content-length" {
            if is_chunked {
                continue;
            }
            if cl_forwarded {
                continue;
            }
            cl_forwarded = true;
        }

        headers.push((name.clone(), value.clone()));
    }

    Ok(Some(RequestMeta {
        method,
        path,
        version,
        headers,
        is_chunked,
        content_length,
        client_wants_close,
    }))
}
fn is_path_allowed(path: &str) -> bool {
    let normalized = path.to_lowercase();
    if normalized.starts_with("/admin") {
        return false;
    }
    true
}

async fn send_error(
    writer: &mut tokio::io::WriteHalf<TcpStream>,
    status: &str,
    body: &str,
) -> tokio::io::Result<()> {
    let resp = format!(
        "HTTP/1.1 {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        status,
        body.len(),
        body,
    );
    writer.write_all(resp.as_bytes()).await?;
    writer.flush().await
}


async fn forward_body(
    client_reader: &mut BufReader<tokio::io::ReadHalf<TcpStream>>,
    backend: &mut TcpStream,
    meta: &RequestMeta,
) -> tokio::io::Result<()> {
    if meta.is_chunked {
        let mut total_body: usize = 0;

        loop {
            
            let mut size_line = String::new();
            let n = client_reader.read_line(&mut size_line).await?;
            if n == 0 {
                break;
            }

            
            if !size_line.ends_with("\r\n") {
                return Err(tokio::io::Error::new(
                    tokio::io::ErrorKind::InvalidData,
                    "bare LF in chunk-size line",
                ));
            }

            
            
            let trimmed = size_line.trim_end();
            let size_str = trimmed.split(';').next().unwrap_or("0").trim();

            
            if size_str.is_empty() || size_str.len() > 16 {
                return Err(tokio::io::Error::new(
                    tokio::io::ErrorKind::InvalidData,
                    "invalid chunk-size value",
                ));
            }

            let chunk_size = usize::from_str_radix(size_str, 16).map_err(|_| {
                tokio::io::Error::new(
                    tokio::io::ErrorKind::InvalidData,
                    "non-hex digit in chunk-size",
                )
            })?;

            if chunk_size > MAX_CHUNK_SIZE {
                return Err(tokio::io::Error::new(
                    tokio::io::ErrorKind::InvalidData,
                    "chunk-size exceeds limit",
                ));
            }

            total_body += chunk_size;
            if total_body > MAX_BODY_SIZE {
                return Err(tokio::io::Error::new(
                    tokio::io::ErrorKind::InvalidData,
                    "chunked body exceeds limit",
                ));
            }

            
            let normalised = format!("{:x}\r\n", chunk_size);
            backend.write_all(normalised.as_bytes()).await?;

            
            if chunk_size == 0 {
                
                
                
                
                loop {
                    let mut trailer = String::new();
                    let tn = client_reader.read_line(&mut trailer).await?;
                    if tn == 0 {
                        break;
                    }
                    
                    if trailer.trim_end().is_empty() {
                        backend.write_all(b"\r\n").await?;
                        break;
                    }
                    
                    
                    if let Some(cp) = trailer.find(':') {
                        let tname = &trailer[..cp];
                        if !is_valid_token(tname) {
                            return Err(tokio::io::Error::new(
                                tokio::io::ErrorKind::InvalidData,
                                "invalid trailer field-name",
                            ));
                        }
                    }
                    backend.write_all(trailer.as_bytes()).await?;
                }
                backend.flush().await?;
                break;
            }

            
            let mut buf = vec![0u8; chunk_size + 2];
            client_reader.read_exact(&mut buf).await?;

            
            if buf[chunk_size] != b'\r' || buf[chunk_size + 1] != b'\n' {
                return Err(tokio::io::Error::new(
                    tokio::io::ErrorKind::InvalidData,
                    "missing CRLF after chunk-data",
                ));
            }

            backend.write_all(&buf).await?;
        }
    } else if let Some(cl) = meta.content_length {
        if cl > 0 {
            let mut remaining = cl;
            let mut buf = [0u8; 8192];
            while remaining > 0 {
                let to_read = std::cmp::min(remaining, buf.len());
                let n = client_reader.read(&mut buf[..to_read]).await?;
                if n == 0 {
                    break;
                }
                backend.write_all(&buf[..n]).await?;
                remaining -= n;
            }
        }
    }
    
    backend.flush().await?;
    Ok(())
}












async fn forward_response(
    backend: &mut BufReader<tokio::net::tcp::OwnedReadHalf>,
    client_writer: &mut tokio::io::WriteHalf<TcpStream>,
) -> tokio::io::Result<bool> {
    let mut keep_alive = true;
    let mut status_line = String::new();
    let n = backend.read_line(&mut status_line).await?;
    if n == 0 {
        return Ok(false);
    }
    client_writer.write_all(status_line.as_bytes()).await?;

    let mut content_length: Option<usize> = None;
    let mut is_chunked = false;

    
    loop {
        let mut line = String::new();
        let n = backend.read_line(&mut line).await?;
        if n == 0 {
            return Ok(false);
        }
        client_writer.write_all(line.as_bytes()).await?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            break; 
        }
        let lower = trimmed.to_lowercase();
        if lower.starts_with("content-length:") {
            if let Some(val) = trimmed.split(':').nth(1) {
                content_length = val.trim().parse().ok();
            }
        }
        if lower.starts_with("transfer-encoding:") && lower.contains("chunked") {
            is_chunked = true;
        }
        if lower.starts_with("connection:") && lower.contains("close") {
            keep_alive = false;
        }
    }

    
    if is_chunked {
        loop {
            let mut line = String::new();
            let n = backend.read_line(&mut line).await?;
            if n == 0 {
                break;
            }
            client_writer.write_all(line.as_bytes()).await?;
            let size_str = line.trim().split(';').next().unwrap_or("0");
            let chunk_size = usize::from_str_radix(size_str, 16).unwrap_or(0);
            if chunk_size == 0 {
                
                loop {
                    let mut tl = String::new();
                    let tn = backend.read_line(&mut tl).await?;
                    if tn == 0 {
                        break;
                    }
                    client_writer.write_all(tl.as_bytes()).await?;
                    if tl.trim().is_empty() {
                        break;
                    }
                }
                break;
            }
            let mut buf = vec![0u8; chunk_size + 2];
            backend.read_exact(&mut buf).await?;
            client_writer.write_all(&buf).await?;
        }
    } else if let Some(cl) = content_length {
        let mut remaining = cl;
        let mut buf = [0u8; 8192];
        while remaining > 0 {
            let to_read = std::cmp::min(remaining, buf.len());
            let n = backend.read(&mut buf[..to_read]).await?;
            if n == 0 {
                break;
            }
            client_writer.write_all(&buf[..n]).await?;
            remaining -= n;
        }
    }
    client_writer.flush().await?;
    Ok(keep_alive)
}









async fn handle_client(client: TcpStream, pool: Arc<ConnPool>) {
    let (read_half, mut write_half) = tokio::io::split(client);
    let mut reader = BufReader::new(read_half);

    loop {
        
        let meta = match parse_request(&mut reader).await {
            Ok(Some(m)) => m,
            Ok(None) => break, 
            Err(msg) => {
                
                let _ = send_error(&mut write_half, "400 Bad Request", msg).await;
                break;
            }
        };

        let close_after = meta.client_wants_close;

        
        if !is_path_allowed(&meta.path) {
            let _ = send_error(&mut write_half, "403 Forbidden", "Access denied.\n").await;
            break;
        }

        
        let mut backend_stream = match pool.acquire().await {
            Ok(s) => s,
            Err(e) => {
                
                let _ =
                    send_error(&mut write_half, "502 Bad Gateway", "Backend unavailable.\n").await;
                break;
            }
        };

        
        let mut head = format!("{} {} {}\r\n", meta.method, meta.path, meta.version);
        for (k, v) in &meta.headers {
            head.push_str(&format!("{}: {}\r\n", k, v));
        }
        head.push_str("\r\n");

        if backend_stream.write_all(head.as_bytes()).await.is_err() {
            
            backend_stream = match TcpStream::connect(BACKEND_ADDR).await {
                Ok(s) => s,
                Err(_) => break,
            };
            if backend_stream.write_all(head.as_bytes()).await.is_err() {
                break;
            }
        }

        
        if let Err(e) = forward_body(&mut reader, &mut backend_stream, &meta).await {
            
            break;
        }

        
        let (b_read, b_write) = backend_stream.into_split();
        let mut b_reader = BufReader::new(b_read);

        
        let backend_keep_alive = match forward_response(&mut b_reader, &mut write_half).await {
            Ok(ka) => ka,
            Err(e) => {
                
                break;
            }
        };

        
        let b_read = b_reader.into_inner();
        let reunited = b_read.reunite(b_write).expect("reunite backend");
        pool.release(reunited).await;

        
        if close_after || !backend_keep_alive {
            break;
        }
    }
}




#[tokio::main]
async fn main() -> tokio::io::Result<()> {
    let listener = TcpListener::bind(LISTEN_ADDR).await?;
    let pool = Arc::new(ConnPool::new());
    
    loop {
        let (client, addr) = listener.accept().await?;
        
        let pool = Arc::clone(&pool);
        tokio::spawn(async move {
            handle_client(client, pool).await;
        });
    }
}
