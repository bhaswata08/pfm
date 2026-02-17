use std::process::{Command, Child, Stdio};
use anyhow::{Context, Result};

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

        let mut process = Command::new("ssh")
            .arg("-N")
            .arg("-L")
            .arg(&forward_arg)
            .arg(host)
            .stdin(Stdio::null())   
            .stdout(Stdio::null()) 
            .stderr(Stdio::null())
            .spawn()
            .context("Failed to start ssh process")?;

        std::thread::sleep(std::time::Duration::from_millis(500));

        if let Some(status) = process.try_wait()? {
            anyhow::bail!("SSH process exited immediately: {:?}", status);
        }

        Ok(SshTunnel { process })
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
