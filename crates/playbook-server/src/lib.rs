//! One-shot HTTP/1.0 status surface.
//!
//! A drive trace is already the audit log. This listener prints the live snapshot.

#![forbid(unsafe_code)]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use playbook_core::TacticalSnapshot;

pub fn demo_snapshot() -> TacticalSnapshot {
    TacticalSnapshot {
        drive_id: "drive-mvp".into(),
        current_down: playbook_core::Down::ThirdDownRush,
        presentation: playbook_core::Down::ThirdDownRush.presentation().into(),
        current_play: Some("integration validation".into()),
        current_personnel: Some("cargo-toolchain".into()),
        ytg_milli: 18,
        ytg_delta_milli: -13,
        budget_remaining_ppm: 640_000,
        budget_consumed: playbook_core::DriveBudget::zero(),
        mandatory_gates: vec!["integration_tests".into()],
        blocked_gates: Vec::new(),
        active_capability: Some("elci.cargo".into()),
        last_result: None,
        completion_probability_ppm: 900_000,
        failure_class: None,
        status: playbook_core::DriveStatus::Active,
        explanation: vec!["remaining budget is 64%".into()],
    }
}

pub fn handle_status(request: &str, snapshot: &TacticalSnapshot) -> String {
    let (status, body) = if request.starts_with("GET /status ") {
        match serde_json::to_string(snapshot) {
            Ok(body) => ("200 OK", body),
            Err(_) => (
                "500 Internal Server Error",
                "{\"error\":\"snapshot\"}".into(),
            ),
        }
    } else {
        ("404 Not Found", "{\"error\":\"not found\"}".into())
    };
    format!(
        "HTTP/1.0 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

pub fn serve_once(listener: TcpListener, snapshot: &TacticalSnapshot) -> std::io::Result<()> {
    let (mut stream, _) = listener.accept()?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    let request = read_headers(&mut stream)?;
    let response = handle_status(&request, snapshot);
    stream.write_all(response.as_bytes())?;
    stream.flush()
}

fn read_headers(stream: &mut TcpStream) -> std::io::Result<String> {
    let mut buf = [0u8; 1024];
    let mut collected = Vec::new();
    loop {
        let read = stream.read(&mut buf)?;
        if read == 0 {
            break;
        }
        collected.extend_from_slice(&buf[..read]);
        if collected.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
        if collected.len() > 8192 {
            break;
        }
    }
    Ok(String::from_utf8_lossy(&collected).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpStream;
    use std::thread;

    #[test]
    fn status_responds_with_the_snapshot() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let snapshot = demo_snapshot();
        let server = thread::spawn(move || serve_once(listener, &snapshot));
        let mut client = TcpStream::connect(addr).unwrap();
        client
            .write_all(b"GET /status HTTP/1.0\r\nHost: localhost\r\n\r\n")
            .unwrap();
        let mut body = String::new();
        client.read_to_string(&mut body).unwrap();
        server.join().unwrap().unwrap();
        assert!(body.contains("200 OK"));
        assert!(body.contains("drive-mvp"));
        assert!(body.contains("3rd Down"));
    }
}
