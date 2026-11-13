//! A small blocking HTTPS client on WinHTTP (Windows' own stack and
//! certificate store; the WinRT `HttpClient` failed TLS with 0x80072F8F on
//! the user's PC). Used for the weather and for lyrics. Call it off the UI
//! thread: every phase can take up to the timeout.

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
    use std::ffi::c_void;
    use windows::core::{w, HSTRING, PCWSTR};
    use windows::Win32::Networking::WinHttp::{
        WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest, WinHttpQueryDataAvailable,
        WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest, WinHttpSetTimeouts,
        WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_FLAG_SECURE, WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE,
    };

    const TIMEOUT_MS: i32 = 10_000;
    const MAX_BYTES: usize = 1 << 20;

    struct Handle(*mut c_void);
    impl Drop for Handle {
        fn drop(&mut self) {
            if !self.0.is_null() {
                let _ = unsafe { WinHttpCloseHandle(self.0) };
            }
        }
    }
    let opened = |handle: *mut c_void, step: &str| {
        if handle.is_null() {
            Err(format!("{step}: {}", windows::core::Error::from_thread()))
        } else {
            Ok(Handle(handle))
        }
    };

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
        WinHttpSetTimeouts(session.0, TIMEOUT_MS, TIMEOUT_MS, TIMEOUT_MS, TIMEOUT_MS).map_err(|e| e.to_string())?;
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
        if status != 200 {
            return Ok((status, String::new()));
        }

        let mut body = Vec::new();
        loop {
            let mut available = 0u32;
            WinHttpQueryDataAvailable(request.0, &mut available).map_err(|e| format!("read: {e}"))?;
            if available == 0 {
                break;
            }
            let start = body.len();
            if start + available as usize > MAX_BYTES {
                return Err("response too large".into());
            }
            body.resize(start + available as usize, 0u8);
            let mut read = 0u32;
            WinHttpReadData(request.0, body[start..].as_mut_ptr().cast(), available, &mut read)
                .map_err(|e| format!("read: {e}"))?;
            body.truncate(start + read as usize);
            if read == 0 {
                break;
            }
        }
        Ok((status, String::from_utf8(body).map_err(|e| e.to_string())?))
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
