use std::fs;
use std::net::TcpStream;
use std::path::Path;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use anyhow::{Context, Result};

const STARTUP_TIMEOUT: Duration = Duration::from_secs(10);
const POLL_INTERVAL: Duration = Duration::from_millis(50);

pub struct SshTunnel {
    process: Child,
}

impl SshTunnel {
    pub fn start(
        host: &str,
        local_port: u16,
        remote_port: u16,
    ) -> Result<Self> {
        let forward_arg = format!("{}:localhost:{}", local_port, remote_port);
        println!("Starting SSH Tunnel: ssh -N -L {} {}", forward_arg, host);

        let config_dir = dirs::config_dir()
            .context("Could not determine config directory")?
            .join("pfm");
        fs::create_dir_all(&config_dir)
            .context("Failed to create config directory")?;

        let log_path = config_dir.join(format!("ssh_{}_{}.log", local_port, std::process::id()));
        let log_file = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&log_path)
            .context("Failed to create SSH log file")?;

        let mut process = Command::new("ssh")
            .arg("-N")
            .arg("-L")
            .arg(&forward_arg)
            .arg("-o")
            .arg("ExitOnForwardFailure=yes")
            .arg("-o")
            .arg("BatchMode=yes")
            .arg(host)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::from(log_file))
            .process_group(0)
            .spawn()
            .context("Failed to start ssh process")?;

        let start = Instant::now();
        loop {
            if let Some(status) = process.try_wait()? {
                return Err(ssh_failure(&log_path, &format!("SSH exited with {}", status)));
            }

            if TcpStream::connect(("127.0.0.1", local_port)).is_ok() {
                // ssh writes to the unlinked file from here on, so nothing reaches the terminal
                let _ = fs::remove_file(&log_path);
                return Ok(SshTunnel { process });
            }

            if start.elapsed() >= STARTUP_TIMEOUT {
                let _ = process.kill();
                let _ = process.wait();
                return Err(ssh_failure(
                    &log_path,
                    &format!("Timed out waiting for SSH tunnel on port {}", local_port),
                ));
            }

            std::thread::sleep(POLL_INTERVAL);
        }
    }

    pub fn pid(&self) -> u32 {
        self.process.id()
    }
}

impl Drop for SshTunnel {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}

/// Builds the startup error from what ssh wrote to stderr, and removes the log file.
fn ssh_failure(log_path: &Path, fallback: &str) -> anyhow::Error {
    let stderr = fs::read_to_string(log_path).unwrap_or_default();
    let _ = fs::remove_file(log_path);
    match stderr.trim() {
        "" => anyhow::anyhow!("{}", fallback),
        msg => anyhow::anyhow!("SSH tunnel failed: {}", msg),
    }
}

pub fn matches_ssh_tunnel(args: &str, host: &str, local_port: u16, remote_port: u16) -> bool {
    let forward_arg = format!("{}:localhost:{}", local_port, remote_port);
    let words: Vec<&str> = args.split_whitespace().collect();

    let has_ssh = words.iter().any(|w| *w == "ssh" || w.ends_with("/ssh"));
    let joined_forward = format!("-L{}", forward_arg);
    let has_forward = words.windows(2).any(|w| w[0] == "-L" && w[1] == forward_arg)
        || words.contains(&joined_forward.as_str());
    let has_host = words.contains(&host);

    has_ssh && has_forward && has_host
}

pub fn is_tunnel_running(pid: u32, host: &str, local_port: u16, remote_port: u16) -> bool {
    let output = std::process::Command::new("ps")
        .arg("-o")
        .arg("args=")
        .arg("-p")
        .arg(pid.to_string())
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let cmdline = String::from_utf8_lossy(&out.stdout);
            matches_ssh_tunnel(&cmdline, host, local_port, remote_port)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matches_ssh_tunnel_valid() {
        assert!(matches_ssh_tunnel(
            "ssh -N -L 8080:localhost:80 user@example.com",
            "user@example.com",
            8080,
            80
        ));
        assert!(matches_ssh_tunnel(
            "/usr/bin/ssh -N -L 8080:localhost:80 user@example.com",
            "user@example.com",
            8080,
            80
        ));
        assert!(matches_ssh_tunnel(
            "ssh -N -L 8080:localhost:80 -o ExitOnForwardFailure=yes -o BatchMode=yes server",
            "server",
            8080,
            80
        ));
        assert!(matches_ssh_tunnel(
            "ssh -L 8080:localhost:80 server",
            "server",
            8080,
            80
        ));
        assert!(matches_ssh_tunnel(
            "ssh -N -L8080:localhost:80 server",
            "server",
            8080,
            80
        ));
    }

    #[test]
    fn test_matches_ssh_tunnel_mismatches() {
        // Wrong host
        assert!(!matches_ssh_tunnel(
            "ssh -N -L 8080:localhost:80 otherhost",
            "server",
            8080,
            80
        ));

        // Wrong local port
        assert!(!matches_ssh_tunnel(
            "ssh -N -L 8081:localhost:80 server",
            "server",
            8080,
            80
        ));

        // Wrong remote port
        assert!(!matches_ssh_tunnel(
            "ssh -N -L 8080:localhost:81 server",
            "server",
            8080,
            80
        ));

        // Forward arg that only ends with the expected one
        assert!(!matches_ssh_tunnel(
            "ssh -N -L 18080:localhost:80 server",
            "server",
            8080,
            80
        ));

        // Missing -L
        assert!(!matches_ssh_tunnel(
            "ssh 8080:localhost:80 server",
            "server",
            8080,
            80
        ));

        // Not ssh process
        assert!(!matches_ssh_tunnel(
            "sleep 120",
            "server",
            8080,
            80
        ));
        assert!(!matches_ssh_tunnel(
            "python server.py -L 8080:localhost:80 server",
            "server",
            8080,
            80
        ));
    }
}
