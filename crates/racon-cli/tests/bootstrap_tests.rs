use std::collections::BTreeSet;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::Value;

const PROCESS_TIMEOUT: Duration = Duration::from_secs(5);

struct ChildGuard {
    child: Child,
    stdout_remainder: Option<mpsc::Receiver<io::Result<String>>>,
}

impl ChildGuard {
    fn spawn(args: &[&str]) -> Self {
        let child = Command::new(env!("CARGO_BIN_EXE_racon"))
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("racon CLI should spawn");
        Self {
            child,
            stdout_remainder: None,
        }
    }

    fn read_stdout_line(&mut self) -> String {
        let stdout = self
            .child
            .stdout
            .take()
            .expect("child stdout should be piped");
        let (first_sender, first_receiver) = mpsc::channel();
        let (remainder_sender, remainder_receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();
            let result = reader.read_line(&mut line).map(|_| line);
            if first_sender.send(result).is_err() {
                return;
            }
            let mut remainder = String::new();
            let result = reader.read_to_string(&mut remainder).map(|_| remainder);
            let _ = remainder_sender.send(result);
        });
        self.stdout_remainder = Some(remainder_receiver);

        first_receiver
            .recv_timeout(PROCESS_TIMEOUT)
            .expect("CLI should write its bootstrap line promptly")
            .expect("CLI bootstrap line should be readable")
    }

    fn read_stdout_remainder(&mut self) -> String {
        self.stdout_remainder
            .take()
            .expect("stdout line should be read first")
            .recv_timeout(PROCESS_TIMEOUT)
            .expect("CLI stdout should close after process exit")
            .expect("remaining CLI stdout should be readable")
    }

    fn signal_interrupt(&self) {
        let status = Command::new("kill")
            .args(["-INT", &self.child.id().to_string()])
            .status()
            .expect("SIGINT command should run");
        assert!(status.success(), "SIGINT should be delivered to the CLI");
    }

    fn wait(&mut self) -> ExitStatus {
        let deadline = Instant::now() + PROCESS_TIMEOUT;
        loop {
            if let Some(status) = self
                .child
                .try_wait()
                .expect("CLI status should be readable")
            {
                return status;
            }
            assert!(Instant::now() < deadline, "CLI did not exit before timeout");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn read_stderr(&mut self) -> String {
        let mut stderr = String::new();
        self.child
            .stderr
            .as_mut()
            .expect("child stderr should be piped")
            .read_to_string(&mut stderr)
            .expect("child stderr should be readable");
        stderr
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

fn http_get(port: u16, path: &str) -> String {
    let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port))
        .expect("reported server address should accept HTTP connections");
    let request = format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n");
    stream
        .write_all(request.as_bytes())
        .expect("HTTP request should be written");

    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("HTTP response should be read");
    response
}

#[test]
fn start_server_json_bootstraps_the_api_and_stays_resident_until_sigint() {
    let mut process = ChildGuard::spawn(&["start-server", "--json", "--port", "0"]);
    let line = process.read_stdout_line();
    let bootstrap: Value = serde_json::from_str(line.trim_end())
        .expect("stdout bootstrap line should contain valid JSON");
    let keys = bootstrap
        .as_object()
        .expect("bootstrap JSON should be an object")
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();

    assert_eq!(
        keys,
        BTreeSet::from(["host", "pid", "port", "protocol_version", "status", "token",])
    );
    assert_eq!(bootstrap["status"], "ready");
    assert_eq!(bootstrap["host"], "127.0.0.1");
    assert_eq!(bootstrap["protocol_version"], "1.0");
    assert_eq!(bootstrap["pid"], process.child.id());
    assert!(bootstrap["port"].as_u64().is_some_and(|port| port > 0));
    assert!(bootstrap["token"]
        .as_str()
        .is_some_and(|token| !token.is_empty()));
    assert_eq!(
        line.lines().count(),
        1,
        "stdout should contain one JSON line"
    );
    assert!(
        process
            .child
            .try_wait()
            .expect("CLI status should be readable")
            .is_none(),
        "CLI should remain resident while the server is running"
    );

    let health = http_get(
        bootstrap["port"]
            .as_u64()
            .expect("bootstrap port should be numeric") as u16,
        "/v1/health",
    );
    assert!(health.starts_with("HTTP/1.1 200 OK"), "{health}");
    assert!(health.contains("\"status\":\"ok\""), "{health}");
    assert!(health.contains("\"protocol_version\":\"1.0\""), "{health}");

    process.signal_interrupt();
    assert!(
        process.wait().success(),
        "graceful SIGINT shutdown should exit successfully"
    );
    assert_eq!(
        process.read_stdout_remainder(),
        "",
        "stdout should contain no output after the bootstrap JSON"
    );
    assert!(
        process.read_stderr().contains("server stopped"),
        "shutdown log should be written to stderr"
    );
}

#[test]
fn occupied_explicit_port_returns_structured_json_error_and_nonzero_exit() {
    let reservation = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .expect("an available loopback port should be reservable");
    let port = reservation
        .local_addr()
        .expect("reservation should have a local address")
        .port();
    let port_argument = port.to_string();
    let mut process = ChildGuard::spawn(&["start-server", "--json", "--port", &port_argument]);

    let line = process.read_stdout_line();
    let error: Value =
        serde_json::from_str(line.trim_end()).expect("stdout error line should contain valid JSON");
    let status = process.wait();

    assert!(!status.success(), "occupied port should return nonzero");
    assert_eq!(error["error"]["code"], "PORT_UNAVAILABLE");
    assert!(
        error["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains(&format!("127.0.0.1:{port}"))),
        "structured error should identify the occupied address: {error}"
    );
    assert_eq!(
        line.lines().count(),
        1,
        "stdout should contain one JSON line"
    );
    assert_eq!(
        process.read_stdout_remainder(),
        "",
        "stdout should contain no output after the structured error"
    );
    assert!(
        process.read_stderr().contains("failed to start server"),
        "human-readable failure log should be written to stderr"
    );
}
