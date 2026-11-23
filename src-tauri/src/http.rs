//! A small blocking HTTPS client on WinHTTP (Windows' own stack and
//! certificate store; the WinRT `HttpClient` failed TLS with 0x80072F8F on
//! the user's PC). Used for the weather, lyrics and the converter's ffmpeg
//! download. Call it off the UI thread: every phase can take up to the
//! timeout.

/// Sent with every request, so services can tell who is calling (LRCLIB asks
/// apps to identify themselves).
const USER_AGENT: &str = concat!("Duri/", env!("CARGO_PKG_VERSION"), " (https://github.com/yushadev0/duri)");

/// Encodes everything but unreserved characters, for query values.
pub fn percent_encode(text: &str) -> String {
    text.bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (byte as char).to_string(),
            other => format!("%{other:02X}"),
        })
        .collect()
}

/// GET that expects 200; any other status is an error ("HTTP 404").
pub fn get(url: &str) -> Result<String, String> {
    match request(url)? {
        (200, body) => Ok(body),
        (status, _) => Err(format!("HTTP {status}")),
    }
}

/// GET where 404 means "there is none" rather than a failure.
pub fn get_optional(url: &str) -> Result<Option<String>, String> {
    match request(url)? {
        (200, body) => Ok(Some(body)),
        (404, _) => Ok(None),
        (status, _) => Err(format!("HTTP {status}")),
    }
}

/// Status and body. The body is only read for 200.
#[cfg(windows)]
fn request(url: &str) -> Result<(u32, String), String> {
    const MAX_BYTES: usize = 1 << 20;
    let req = winhttp::Request::get(url, 10_000)?;
    if req.status != 200 {
        return Ok((req.status, String::new()));
    }
    let mut body = Vec::new();
    req.read(|chunk| {
        if body.len() + chunk.len() > MAX_BYTES {
            return Err("response too large".into());
        }
        body.extend_from_slice(chunk);
        Ok(())
    })?;
    Ok((req.status, String::from_utf8(body).map_err(|e| e.to_string())?))
}

/// Downloads a file (following redirects) to `dest`, calling `progress`
/// with (bytes so far, total if known). `progress` returns false to stop.
#[cfg(windows)]
pub fn download(
    url: &str,
    dest: &std::path::Path,
    mut progress: impl FnMut(u64, Option<u64>) -> bool,
) -> Result<(), String> {
    use std::io::Write;
    let req = winhttp::Request::get(url, 30_000)?;
    if req.status != 200 {
        return Err(format!("HTTP {}", req.status));
    }
    let total = req.content_length();
    let mut file = std::io::BufWriter::new(std::fs::File::create(dest).map_err(|e| e.to_string())?);
    let mut done = 0u64;
    req.read(|chunk| {
        file.write_all(chunk).map_err(|e| e.to_string())?;
        done += chunk.len() as u64;
        if progress(done, total) { Ok(()) } else { Err("cancelled".into()) }
    })?;
    file.flush().map_err(|e| e.to_string())?;
    if total.is_some_and(|t| t != done) {
        return Err("download ended early".into());
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn download(
    _url: &str,
    _dest: &std::path::Path,
    _progress: impl FnMut(u64, Option<u64>) -> bool,
) -> Result<(), String> {
    Err("unsupported".into())
}

#[cfg(windows)]
mod winhttp {
    use super::USER_AGENT;
    use std::ffi::c_void;
    use windows::core::{w, HSTRING, PCWSTR};
    use windows::Win32::Networking::WinHttp::{
        WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest, WinHttpQueryDataAvailable,
        WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest, WinHttpSetTimeouts,
        WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_FLAG_SECURE, WINHTTP_QUERY_CONTENT_LENGTH,
        WINHTTP_QUERY_FLAG_NUMBER64, WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE,
    };

    struct Handle(*mut c_void);
    impl Drop for Handle {
        fn drop(&mut self) {
            if !self.0.is_null() {
                let _ = unsafe { WinHttpCloseHandle(self.0) };
            }
        }
    }

    fn opened(handle: *mut c_void, step: &str) -> Result<Handle, String> {
        if handle.is_null() {
            Err(format!("{step}: {}", windows::core::Error::from_thread()))
        } else {
            Ok(Handle(handle))
        }
    }

    /// A GET whose response headers have arrived. WinHTTP follows
    /// redirects (GitHub release downloads use one) on its own.
    pub struct Request {
        // Dropped in field order: request, then connection, then session.
        request: Handle,
        _connection: Handle,
        _session: Handle,
        pub status: u32,
    }

    impl Request {
        /// `timeout_ms` applies to each phase (resolve, connect, send, each read).
        pub fn get(url: &str, timeout_ms: i32) -> Result<Request, String> {
            let rest = url.strip_prefix("https://").ok_or("only https is supported")?;
            let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
            let path = format!("/{path}");
            unsafe {
                let session = opened(
                    WinHttpOpen(
                        &HSTRING::from(USER_AGENT),
                        WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
                        PCWSTR::null(),
                        PCWSTR::null(),
                        0,
                    ),
                    "open",
                )?;
                WinHttpSetTimeouts(session.0, timeout_ms, timeout_ms, timeout_ms, timeout_ms)
                    .map_err(|e| e.to_string())?;
                let connection = opened(WinHttpConnect(session.0, &HSTRING::from(host), 443, 0), "connect")?;
                let request = opened(
                    WinHttpOpenRequest(
                        connection.0,
                        w!("GET"),
                        &HSTRING::from(path),
                        PCWSTR::null(),
                        PCWSTR::null(),
                        std::ptr::null(),
                        WINHTTP_FLAG_SECURE,
                    ),
                    "request",
                )?;
                WinHttpSendRequest(request.0, None, None, 0, 0, 0).map_err(|e| format!("send: {e}"))?;
                WinHttpReceiveResponse(request.0, std::ptr::null_mut()).map_err(|e| format!("receive: {e}"))?;
                let (mut status, mut size) = (0u32, std::mem::size_of::<u32>() as u32);
                WinHttpQueryHeaders(
                    request.0,
                    WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
                    PCWSTR::null(),
                    Some((&raw mut status).cast()),
                    &mut size,
                    std::ptr::null_mut(),
                )
                .map_err(|e| format!("status: {e}"))?;
                Ok(Request { request, _connection: connection, _session: session, status })
            }
        }

        pub fn content_length(&self) -> Option<u64> {
            let (mut len, mut size) = (0u64, std::mem::size_of::<u64>() as u32);
            unsafe {
                WinHttpQueryHeaders(
                    self.request.0,
                    WINHTTP_QUERY_CONTENT_LENGTH | WINHTTP_QUERY_FLAG_NUMBER64,
                    PCWSTR::null(),
                    Some((&raw mut len).cast()),
                    &mut size,
                    std::ptr::null_mut(),
                )
                .ok()?;
            }
            Some(len)
        }

        /// Reads the body chunk by chunk; `sink` can stop it with an error.
        pub fn read(&self, mut sink: impl FnMut(&[u8]) -> Result<(), String>) -> Result<(), String> {
            let mut buf = vec![0u8; 64 * 1024];
            loop {
                let mut available = 0u32;
                unsafe { WinHttpQueryDataAvailable(self.request.0, &mut available) }
                    .map_err(|e| format!("read: {e}"))?;
                if available == 0 {
                    return Ok(());
                }
                let want = (available as usize).min(buf.len());
                let mut read = 0u32;
                unsafe { WinHttpReadData(self.request.0, buf.as_mut_ptr().cast(), want as u32, &mut read) }
                    .map_err(|e| format!("read: {e}"))?;
                if read == 0 {
                    return Ok(());
                }
                sink(&buf[..read as usize])?;
            }
        }
    }
}

#[cfg(not(windows))]
fn request(_url: &str) -> Result<(u32, String), String> {
    Err("unsupported".into())
}

#[cfg(test)]
mod tests {
    use super::percent_encode;

    #[test]
    fn encodes_query_values() {
        assert_eq!(percent_encode("a b&c"), "a%20b%26c");
        assert_eq!(percent_encode("Şebnem"), "%C5%9Eebnem");
        assert_eq!(percent_encode("A-z_0.9~"), "A-z_0.9~");
    }
}
