use std::net::TcpListener;
const MAX_PORT: u16 = 65535;

pub fn is_port_available(port:u16) -> bool {
    TcpListener::bind(("127.0.0.1", port)).is_ok()
}

pub fn find_available_port(start_port: u16) -> Option<u16> {
    for port in start_port..MAX_PORT {
        if is_port_available(port) {
            return Some(port)
        }
    }
    None
}

pub fn is_process_running(pid: u32) -> bool {
    std::process::Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}
