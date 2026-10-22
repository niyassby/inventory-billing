//! Getting bytes to a printer: raw TCP (port 9100), a serial port, or the Windows print queue in
//! RAW mode (how USB receipt printers with the maker's driver are reached). Network printers must
//! be on the shop's own network: private, link-local or loopback addresses only.

use std::io::Write;
use std::net::{IpAddr, SocketAddr, TcpStream};
use std::time::Duration;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::error::{AppError, ErrorCode};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PrinterConnection {
    /// Raw TCP, usually port 9100.
    Network { host: String, port: u16 },
    /// "COM3" on Windows, "/dev/ttyUSB0" elsewhere.
    Serial { port: String, baud: u32 },
    /// A printer installed in Windows (its queue name), sent RAW.
    Windows { queue: String },
}

impl PrinterConnection {
    pub fn describe(&self) -> String {
        match self {
            PrinterConnection::Network { host, port } => format!("network {host}:{port}"),
            PrinterConnection::Serial { port, baud } => format!("serial {port} at {baud} baud"),
            PrinterConnection::Windows { queue } => format!("Windows printer \"{queue}\""),
        }
    }
}

/// Why a print failed, in words a shop can act on. Sent as `details.kind` with PRINTER_ERROR.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FailureKind {
    NotConfigured,
    BadAddress,
    Unreachable,
    Refused,
    Timeout,
    NotFound,
    Busy,
    Unsupported,
    Io,
}

impl FailureKind {
    pub fn advice(self) -> &'static str {
        match self {
            FailureKind::NotConfigured => "No printer is set up for this. Choose one in Settings → Printers.",
            FailureKind::BadAddress => "That printer address isn't allowed. Use the printer's address on the shop network (for example 192.168.1.50).",
            FailureKind::Unreachable => "The printer can't be reached on the network. Check that it is on, has paper and is on the same network as this PC.",
            FailureKind::Refused => "The printer refused the connection. Check the port (usually 9100) and that no other PC is printing to it.",
            FailureKind::Timeout => "The printer stopped answering. Check the cable or network, the paper and the cover.",
            FailureKind::NotFound => "That printer or port isn't on this PC. Check the cable and choose it again in Settings → Printers.",
            FailureKind::Busy => "The port is in use by another program. Close it, or unplug and plug in the printer.",
            FailureKind::Unsupported => "This kind of printer connection isn't available on this computer.",
            FailureKind::Io => "The printer reported a problem. Check the paper, the cover and the cable, then try again.",
        }
    }
}

pub fn printer_error(kind: FailureKind, printer: &str, cause: impl std::fmt::Display) -> AppError {
    tracing::warn!(printer, ?kind, "print failed: {cause}");
    AppError::new(ErrorCode::PrinterError, kind.advice()).with_details(json!({
        "kind": kind,
        "printer": printer,
    }))
}

/// Private (RFC 1918 / unique-local), link-local or loopback: the only places a printer may be.
pub fn is_allowed_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_private() || v4.is_link_local() || v4.is_loopback(),
        IpAddr::V6(v6) => {
            let first = v6.segments()[0];
            v6.is_loopback() || (first & 0xfe00) == 0xfc00 || (first & 0xffc0) == 0xfe80
        }
    }
}

const BAUD_RATES: [u32; 7] = [9_600, 19_200, 38_400, 57_600, 115_200, 4_800, 2_400];

/// Checks a connection before it is saved or used.
pub fn validate(connection: &PrinterConnection) -> Result<(), (String, String)> {
    match connection {
        PrinterConnection::Network { host, port } => {
            let ip: IpAddr = host.trim().parse().map_err(|_| {
                (
                    "connection.host".into(),
                    "Enter the printer's IP address, for example 192.168.1.50.".into(),
                )
            })?;
            if !is_allowed_ip(ip) {
                return Err((
                    "connection.host".into(),
                    "Only printers on the shop network are allowed (192.168.x.x, 10.x.x.x or 172.16–31.x.x).".into(),
                ));
            }
            if *port == 0 {
                return Err((
                    "connection.port".into(),
                    "Enter a port (usually 9100).".into(),
                ));
            }
        }
        PrinterConnection::Serial { port, baud } => {
            let name = port.trim();
            let ok = name.len() <= 64
                && ((name.len() > 3
                    && name[..3].eq_ignore_ascii_case("COM")
                    && name[3..].bytes().all(|b| b.is_ascii_digit()))
                    || (name.starts_with("/dev/")
                        && name[5..]
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b"._-/".contains(&b))));
            if !ok {
                return Err((
                    "connection.port".into(),
                    "Enter a serial port such as COM3.".into(),
                ));
            }
            if !BAUD_RATES.contains(baud) {
                return Err((
                    "connection.baud".into(),
                    "Choose a speed the printer supports (usually 9600 or 115200).".into(),
                ));
            }
        }
        PrinterConnection::Windows { queue } => {
            let name = queue.trim();
            if name.is_empty() || name.len() > 200 || name.contains(['\0', '\r', '\n']) {
                return Err((
                    "connection.queue".into(),
                    "Choose a printer installed in Windows.".into(),
                ));
            }
        }
    }
    Ok(())
}

const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const WRITE_TIMEOUT: Duration = Duration::from_secs(8);

fn io_kind(e: &std::io::Error) -> FailureKind {
    use std::io::ErrorKind as K;
    match e.kind() {
        K::ConnectionRefused => FailureKind::Refused,
        K::TimedOut | K::WouldBlock => FailureKind::Timeout,
        K::NotFound => FailureKind::NotFound,
        K::PermissionDenied | K::AddrInUse => FailureKind::Busy,
        K::HostUnreachable | K::NetworkUnreachable | K::AddrNotAvailable => {
            FailureKind::Unreachable
        }
        _ => FailureKind::Io,
    }
}

/// Sends a whole print job. Returns only once the bytes were handed over.
pub fn send(printer: &str, connection: &PrinterConnection, bytes: &[u8]) -> Result<(), AppError> {
    if let Err((_, message)) = validate(connection) {
        return Err(printer_error(FailureKind::BadAddress, printer, message));
    }
    match connection {
        PrinterConnection::Network { host, port } => {
            let ip: IpAddr = host
                .trim()
                .parse()
                .map_err(|e| printer_error(FailureKind::BadAddress, printer, e))?;
            let address = SocketAddr::new(ip, *port);
            let mut stream =
                TcpStream::connect_timeout(&address, CONNECT_TIMEOUT).map_err(|e| {
                    let kind = match io_kind(&e) {
                        FailureKind::Timeout | FailureKind::Io => FailureKind::Unreachable,
                        other => other,
                    };
                    printer_error(kind, printer, e)
                })?;
            stream
                .set_write_timeout(Some(WRITE_TIMEOUT))
                .map_err(|e| printer_error(FailureKind::Io, printer, e))?;
            stream
                .write_all(bytes)
                .and_then(|()| stream.flush())
                .map_err(|e| printer_error(io_kind(&e), printer, e))?;
            // Many printers start printing only when the connection closes.
            let _ = stream.shutdown(std::net::Shutdown::Write);
            Ok(())
        }
        PrinterConnection::Serial { port, baud } => {
            let mut serial = serialport::new(port.trim(), *baud)
                .timeout(WRITE_TIMEOUT)
                .open()
                .map_err(|e| {
                    let kind = match e.kind() {
                        serialport::ErrorKind::NoDevice => FailureKind::NotFound,
                        serialport::ErrorKind::Io(k) => io_kind(&std::io::Error::from(k)),
                        _ => FailureKind::Io,
                    };
                    printer_error(kind, printer, e)
                })?;
            serial
                .write_all(bytes)
                .and_then(|()| serial.flush())
                .map_err(|e| printer_error(io_kind(&e), printer, e))
        }
        PrinterConnection::Windows { queue } => windows_queue::send(printer, queue.trim(), bytes),
    }
}

/// Printers installed in Windows and serial ports present, for the printer settings screen.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Discovered {
    pub windows_queues: Vec<String>,
    pub serial_ports: Vec<String>,
    /// Whether this computer can print through Windows queues.
    pub windows_supported: bool,
}

pub fn discover() -> Discovered {
    let serial_ports = serialport::available_ports()
        .map(|ports| ports.into_iter().map(|p| p.port_name).collect())
        .unwrap_or_default();
    Discovered {
        windows_queues: windows_queue::list(),
        serial_ports,
        windows_supported: cfg!(windows),
    }
}

#[cfg(windows)]
mod windows_queue {
    use std::ptr::{null, null_mut};

    use windows_sys::Win32::Foundation::{GetLastError, HANDLE};
    use windows_sys::Win32::Graphics::Printing::{
        ClosePrinter, EndDocPrinter, EndPagePrinter, EnumPrintersW, OpenPrinterW, StartDocPrinterW,
        StartPagePrinter, WritePrinter, DOC_INFO_1W, PRINTER_ENUM_CONNECTIONS, PRINTER_ENUM_LOCAL,
        PRINTER_INFO_4W,
    };

    use super::{printer_error, FailureKind};
    use crate::error::AppError;

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// Closes the printer handle however the job ends.
    struct Printer(HANDLE);

    impl Drop for Printer {
        fn drop(&mut self) {
            // SAFETY: the handle came from a successful OpenPrinterW and is closed once.
            unsafe {
                ClosePrinter(self.0);
            }
        }
    }

    fn last_error(printer: &str, step: &str) -> AppError {
        // SAFETY: reads the calling thread's last error code.
        let code = unsafe { GetLastError() };
        // 1801 ERROR_INVALID_PRINTER_NAME, 2 ERROR_FILE_NOT_FOUND, 5 ERROR_ACCESS_DENIED.
        let kind = match code {
            1801 | 2 | 3 => FailureKind::NotFound,
            5 | 32 => FailureKind::Busy,
            _ => FailureKind::Io,
        };
        printer_error(
            kind,
            printer,
            format!("{step} failed (Windows error {code})"),
        )
    }

    pub fn send(printer: &str, queue: &str, bytes: &[u8]) -> Result<(), AppError> {
        let name = wide(queue);
        let mut handle: HANDLE = null_mut();
        // SAFETY: `name` is a NUL-terminated UTF-16 string that outlives the call.
        if unsafe { OpenPrinterW(name.as_ptr(), &mut handle, null()) } == 0 {
            return Err(last_error(printer, "OpenPrinter"));
        }
        let guard = Printer(handle);
        let mut doc_name = wide("Inventory and Billing");
        let mut datatype = wide("RAW");
        let info = DOC_INFO_1W {
            pDocName: doc_name.as_mut_ptr(),
            pOutputFile: null_mut(),
            pDatatype: datatype.as_mut_ptr(),
        };
        // SAFETY: `info` points at buffers that live until the end of this function.
        if unsafe { StartDocPrinterW(guard.0, 1, &info) } == 0 {
            return Err(last_error(printer, "StartDocPrinter"));
        }
        // SAFETY: a document was started on this handle.
        let result = unsafe {
            if StartPagePrinter(guard.0) == 0 {
                Err(last_error(printer, "StartPagePrinter"))
            } else {
                let mut written = 0u32;
                let len = u32::try_from(bytes.len()).unwrap_or(u32::MAX);
                let ok = WritePrinter(guard.0, bytes.as_ptr().cast(), len, &mut written);
                EndPagePrinter(guard.0);
                if ok == 0 {
                    Err(last_error(printer, "WritePrinter"))
                } else if written != len {
                    Err(printer_error(
                        FailureKind::Io,
                        printer,
                        "the queue took only part of the job",
                    ))
                } else {
                    Ok(())
                }
            }
        };
        // SAFETY: ends the document started above.
        unsafe {
            EndDocPrinter(guard.0);
        }
        result
    }

    pub fn list() -> Vec<String> {
        let flags = PRINTER_ENUM_LOCAL | PRINTER_ENUM_CONNECTIONS;
        let mut needed = 0u32;
        let mut count = 0u32;
        // SAFETY: a size query with a null buffer, as the API documents.
        unsafe {
            EnumPrintersW(flags, null(), 4, null_mut(), 0, &mut needed, &mut count);
        }
        if needed == 0 {
            return Vec::new();
        }
        // u64 storage keeps the buffer aligned for PRINTER_INFO_4W.
        let mut buffer = vec![0u64; (needed as usize).div_ceil(8)];
        // SAFETY: the buffer holds at least `needed` bytes.
        let ok = unsafe {
            EnumPrintersW(
                flags,
                null(),
                4,
                buffer.as_mut_ptr().cast(),
                needed,
                &mut needed,
                &mut count,
            )
        };
        if ok == 0 {
            return Vec::new();
        }
        let infos = buffer.as_ptr().cast::<PRINTER_INFO_4W>();
        let mut names = Vec::with_capacity(count as usize);
        for i in 0..count as usize {
            // SAFETY: EnumPrintersW wrote `count` PRINTER_INFO_4W records at the buffer's start;
            // their name pointers point into the same buffer.
            let name = unsafe {
                let info = &*infos.add(i);
                let mut len = 0;
                while *info.pPrinterName.add(len) != 0 {
                    len += 1;
                }
                String::from_utf16_lossy(std::slice::from_raw_parts(info.pPrinterName, len))
            };
            names.push(name);
        }
        names.sort();
        names
    }
}

#[cfg(not(windows))]
mod windows_queue {
    use super::{printer_error, FailureKind};
    use crate::error::AppError;

    pub fn send(printer: &str, _queue: &str, _bytes: &[u8]) -> Result<(), AppError> {
        Err(printer_error(
            FailureKind::Unsupported,
            printer,
            "Windows print queues only exist on Windows",
        ))
    }

    pub fn list() -> Vec<String> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::net::TcpListener;

    #[test]
    fn only_shop_network_addresses_are_allowed() {
        for ok in [
            "192.168.1.50",
            "10.0.0.7",
            "172.16.4.2",
            "169.254.10.1",
            "127.0.0.1",
            "fd00::5",
            "fe80::1",
        ] {
            assert!(is_allowed_ip(ok.parse().unwrap()), "{ok}");
        }
        for bad in ["8.8.8.8", "172.32.0.1", "100.64.0.1", "2001:db8::1"] {
            assert!(!is_allowed_ip(bad.parse().unwrap()), "{bad}");
        }
        assert!(validate(&PrinterConnection::Network {
            host: "printer.local".into(),
            port: 9100
        })
        .is_err());
        assert!(validate(&PrinterConnection::Network {
            host: "8.8.8.8".into(),
            port: 9100
        })
        .is_err());
        assert!(validate(&PrinterConnection::Serial {
            port: "COM3".into(),
            baud: 9600
        })
        .is_ok());
        assert!(validate(&PrinterConnection::Serial {
            port: "COM3; del".into(),
            baud: 9600
        })
        .is_err());
        assert!(validate(&PrinterConnection::Serial {
            port: "COM3".into(),
            baud: 1234
        })
        .is_err());
    }

    #[test]
    fn sends_to_a_fake_network_printer() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let reader = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut got = Vec::new();
            socket.read_to_end(&mut got).unwrap();
            got
        });
        let connection = PrinterConnection::Network {
            host: "127.0.0.1".into(),
            port,
        };
        send("Front", &connection, b"\x1b@hello\n").unwrap();
        assert_eq!(reader.join().unwrap(), b"\x1b@hello\n");
    }

    #[test]
    fn a_closed_port_is_reported_as_refused_or_unreachable() {
        // Bind and drop to get a port nothing listens on.
        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let connection = PrinterConnection::Network {
            host: "127.0.0.1".into(),
            port,
        };
        let err = send("Front", &connection, b"x").unwrap_err();
        assert_eq!(err.code, ErrorCode::PrinterError);
        let kind = err.details.unwrap()["kind"].as_str().unwrap().to_string();
        assert!(kind == "refused" || kind == "unreachable", "{kind}");
    }

    #[test]
    fn a_public_address_is_refused_before_connecting() {
        let connection = PrinterConnection::Network {
            host: "8.8.8.8".into(),
            port: 9100,
        };
        let err = send("Front", &connection, b"x").unwrap_err();
        assert_eq!(err.details.unwrap()["kind"], "bad_address");
    }
}
