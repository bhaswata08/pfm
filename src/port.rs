use std::net::TcpListener;
const MAX_PORT: u16 = 65535;

pub fn is_port_available(port: u16) -> bool {
    TcpListener::bind(("127.0.0.1", port)).is_ok()
}

pub fn find_available_port(start_port: u16) -> Option<u16> {
    (start_port..=MAX_PORT).find(|&port| is_port_available(port))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_available_port_near_65535() {
        // Test port 65535 can be found when available
        let available = find_available_port(65535);
        if is_port_available(65535) {
            assert_eq!(available, Some(65535));
        }

        // Test with occupied port 65535
        if let Ok(_listener) = TcpListener::bind(("127.0.0.1", 65535)) {
            assert_eq!(find_available_port(65535), None);
        }
    }
}
