use std::io;
use clap_complete::{generate, Shell};
use clap::{CommandFactory, Parser, Subcommand};
use colored::Colorize;
use anyhow::{Context, Result};

mod config;
use config::{Config, PortForward};

mod port;
mod ssh;

use ssh::SshTunnel;

const EXAMPLES: &str = "\
Examples:
  pfm add user@server.com 8080:80   Forward localhost:8080 to port 80 on server.com
  pfm add server.com 3000           Forward localhost:3000 to port 3000 on server.com
  pfm list                          Show all forwards with their index and status
  pfm stop 0                        Stop forward 0 but keep it saved
  pfm start all                     Bring back every stopped forward (e.g. after a reboot)
  pfm delete 0 1                    Stop and remove forwards 0 and 1
  pfm cleanup                       Remove forwards whose ssh process has died

Run 'pfm <COMMAND> --help' for more on a command.";

#[derive(Parser)]
#[command(name = "pfm")]
#[command(about = "Port forward manager")]
#[command(version)]
#[command(after_help = EXAMPLES)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Add a new SSH port forward
    #[command(after_help = "\
Examples:
  pfm add user@server.com 8080:80   Forward localhost:8080 to port 80 on server.com
  pfm add server.com 3000           Forward localhost:3000 to port 3000 on server.com

If the local port is in use, pfm picks the next free port.
The host must accept key-based login; pfm cannot answer a password prompt.")]
    Add {
        /// SSH host (user@hostname)
        host: String,
        /// Port mapping (local:remote or just local for same port)
        ports: String,
    },
    /// List all configured port forwards
    List,
    /// Stop running port forward(s) but keep them saved
    #[command(after_help = "\
Examples:
  pfm stop 0 1 2    Stop forwards at index 0, 1 and 2
  pfm stop all      Stop all forwards")]
    Stop {
        /// Forward indices or 'all'
        #[arg(required = true)]
        ids: Vec<String>,
    },
    /// Start stopped port forward(s)
    #[command(after_help = "\
Examples:
  pfm start 0 1 2   Start forwards at index 0, 1 and 2
  pfm start all     Start all stopped forwards, e.g. after a reboot")]
    Start {
        /// Forward indices or 'all'
        #[arg(required = true)]
        ids: Vec<String>,
    },
    /// Delete port forward(s), stopping them first
    #[command(after_help = "\
Examples:
  pfm delete 0 1 2  Delete forwards at index 0, 1 and 2
  pfm delete all    Delete all forwards")]
    Delete {
        /// Forward indices or 'all'
        #[arg(required = true)]
        ids: Vec<String>,
    },
    /// Remove forwards whose SSH processes have died
    Cleanup,
    /// Generate shell completions
    #[command(after_help = "\
Examples:
  pfm completions fish > ~/.config/fish/completions/pfm.fish
  pfm completions zsh > ~/.zfunc/_pfm")]
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: Shell,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Completions { shell } => {
            generate_completions(*shell);
        }
        _ => {
            // Load config for all other commands
            let mut config = Config::load()?;
            
            match &cli.command {
                Commands::Add { host, ports } => {
                    add_forward(&mut config, host, ports)?;
                }
                Commands::List => {
                    list_forwards(&config);
                }
                Commands::Stop { ids } => {
                    stop_forwards(&mut config, ids)?;
                }
                Commands::Start { ids } => {
                    start_forwards(&mut config, ids)?;
                }
                Commands::Delete { ids } => {
                    delete_forwards(&mut config, ids)?;
                }
                Commands::Cleanup => {
                    cleanup_dead_forwards(&mut config)?;
                }
                Commands::Completions { .. } => unreachable!(),
            }
        }
    }

    Ok(())
}

fn generate_completions(shell: Shell) {
    let mut cmd = Cli::command();
    generate(
        shell,
        &mut cmd,
        "pfm",
        &mut io::stdout(),
    );
}

fn parse_ports(ports: &str) -> Result<(u16, u16)> {
    if ports.contains(':') {
        let parts: Vec<&str> = ports.split(':').collect();
        if parts.len() != 2 {
            anyhow::bail!("Invalid format '{}'. Use LOCAL:REMOTE or just PORT", ports);
        }

        let local = parts[0].parse::<u16>()
            .context("Invalid local port")?;
        let remote = parts[1].parse::<u16>()
            .context("Invalid remote port")?;
        Ok((local, remote))
    } else {
        let port = ports.parse::<u16>()
            .context("Invalid port number")?;
        Ok((port, port))
    }
}

fn is_forward_running(forward: &PortForward) -> bool {
    let Some(pid) = forward.pid else {
        return false;
    };
    ssh::is_tunnel_running(pid, &forward.host, forward.local_port, forward.remote_port)
}

fn make_forward_id(host: &str, local_port: u16, remote_port: u16) -> String {
    format!("{}_{}_{}", host.replace('@', "_at_"), local_port, remote_port)
}

fn launch_tunnel(host: &str, requested_local: u16, remote: u16) -> Result<(u16, u32)> {
    let mut local = requested_local;
    let original_port = local;
    if !port::is_port_available(local) {
        println!("{}", format!("Port {} is already in use", local).yellow());

        if let Some(new_port) = local.checked_add(1).and_then(port::find_available_port) {
            local = new_port;
            println!("{}", format!("Using port {} instead", local).green());
        } else {
            anyhow::bail!("No available ports found!");
        }
    }
    let tunnel = SshTunnel::start(host, local, remote)?;
    let pid = tunnel.pid();

    std::mem::forget(tunnel);

    if original_port != local {
        println!("{}", format!("\n⚠ Port remapped from {} to {}", original_port, local).yellow());
    }

    Ok((local, pid))
}

fn add_forward(config: &mut Config, host: &str, ports: &str) -> Result<()> {
    let (local, remote) = parse_ports(ports)?;
    let (actual_local, pid) = launch_tunnel(host, local, remote)?;

    let id = make_forward_id(host, actual_local, remote);
    let forward = PortForward {
        id: id.clone(),
        host: host.to_string(),
        local_port: actual_local,
        remote_port: remote,
        pid: Some(pid),
    };
    config.add_forward(forward);
    config.save()?;

    println!("\n{}", "✓ Port forward created!".green().bold());
    println!("{}", format!("  ID: {}", id).cyan());
    println!("  {}:{} → {}:{}", 
             "localhost".dimmed(), 
             actual_local.to_string().cyan(), 
             host.cyan(), 
             remote.to_string().cyan());
    println!("  {}: {}", "PID".cyan(), pid);

    Ok(())
}

fn list_forwards(config: &Config) {
    if config.forwards.is_empty() {
        println!("{}", "No port forwards configured.".yellow());
        println!("\n{}", "Add one with: pfm add <host> <ports>".dimmed());
        return;
    }

    let total = config.forwards.len();
    let running = config.forwards.values()
        .filter(|f| is_forward_running(f))
        .count();

    println!("\n{} ({} running, {} total)\n", 
             "Port forwards:".bold().underline(),
             running.to_string().green(),
             total);

    for (index, forward) in config.get_sorted_forwards().iter().enumerate() {
        println!("  {}: {}", "ID".cyan(), index.to_string().bold());
        println!("  {}:  {}", "Host".cyan(), forward.host);
        println!("  {}: {} → {}", 
                 "Ports".cyan(), 
                 forward.local_port, 
                 forward.remote_port);

        if let Some(pid) = forward.pid {
            let status = if is_forward_running(forward) {
                "● Running".green()
            } else {
                "○ Stopped".yellow()
            };
            println!("  {}:   {} ({})", "PID".cyan(), pid, status);
        } else {
            println!("  {}:   - ({})", "PID".cyan(), "○ Stopped".yellow());
        }
        
        println!();
    }
}

/// Resolves CLI arguments (indices, raw IDs, or `all`) into forward IDs.
/// Invalid indices are reported and recorded in `errors`.
fn resolve_ids(config: &Config, ids: &[String], verb: &str, errors: &mut Vec<String>) -> Vec<String> {
    if ids.len() == 1 && ids[0] == "all" {
        println!("{}", format!("{} all {} forward(s)...\n", verb, config.forwards.len()).yellow());
        return config.get_sorted_forwards().iter().map(|f| f.id.clone()).collect();
    }

    let mut result = Vec::new();
    for id_str in ids {
        if let Ok(index) = id_str.parse::<usize>() {
            if let Some(forward) = config.get_forward_by_index(index) {
                result.push(forward.id.clone());
            } else {
                let error = format!("✗ Invalid index: {}", index);
                eprintln!("{}", error.red());
                errors.push(error);
            }
        } else {
            result.push(id_str.to_string());
        }
    }
    result
}

fn stop_forwards(config: &mut Config, ids: &[String]) -> Result<()> {
    let mut stopped_count = 0;
    let mut errors = Vec::new();

    let ids_to_stop = resolve_ids(config, ids, "Stopping", &mut errors);

    let mut modified = false;
    for id in ids_to_stop {
        if let Some(forward) = config.forwards.get_mut(&id) {
            if is_forward_running(forward) {
                if let Some(pid) = forward.pid
                    && let Err(e) = kill_process(pid)
                {
                    eprintln!("{}", format!("  ⚠ Warning: {}", e).yellow());
                }
                forward.pid = None;
                println!("{} {} (localhost:{} → {}:{})", 
                         "✓ Stopped:".green(),
                         forward.id.dimmed(),
                         forward.local_port,
                         forward.host,
                         forward.remote_port);
                stopped_count += 1;
                modified = true;
            } else {
                if forward.pid.is_some() {
                    forward.pid = None;
                    modified = true;
                }
                println!("  Process was not running for {}", forward.id);
            }
        } else {
            let error = format!("✗ Not found: {}", id);
            eprintln!("{}", error.red());
            errors.push(error);
        }
    }

    if modified {
        config.save()?;
    }

    if stopped_count > 0 {
        println!("\n{}", format!("✓ Stopped {} forward(s)", stopped_count).green());
    }

    if !errors.is_empty() {
        anyhow::bail!("Some stops failed");
    }

    Ok(())
}

fn start_forwards(config: &mut Config, ids: &[String]) -> Result<()> {
    let mut started_count = 0;
    let mut errors = Vec::new();

    let ids_to_start = resolve_ids(config, ids, "Starting", &mut errors);

    for id in ids_to_start {
        let forward_opt = config.forwards.get(&id).cloned();
        let Some(mut forward) = forward_opt else {
            let error = format!("✗ Not found: {}", id);
            eprintln!("{}", error.red());
            errors.push(error);
            continue;
        };

        if is_forward_running(&forward) {
            println!("  Forward {} is already running (PID: {})", forward.id, forward.pid.unwrap_or(0));
            continue;
        }

        println!("Starting forward: {}", forward.id);
        match launch_tunnel(&forward.host, forward.local_port, forward.remote_port) {
            Ok((actual_local, pid)) => {
                let original_local = forward.local_port;
                forward.local_port = actual_local;
                forward.pid = Some(pid);

                if actual_local != original_local {
                    config.remove_forward(&id);
                    let new_id = make_forward_id(&forward.host, actual_local, forward.remote_port);
                    forward.id = new_id.clone();
                    config.add_forward(forward.clone());
                } else {
                    config.add_forward(forward.clone());
                }

                println!("{} {} (localhost:{} → {}:{})", 
                         "✓ Started:".green(),
                         forward.id.dimmed(),
                         actual_local,
                         forward.host,
                         forward.remote_port);
                println!("  PID: {}", pid);
                started_count += 1;
            }
            Err(e) => {
                let error = format!("✗ Failed to start {}: {:#}", forward.id, e);
                eprintln!("{}", error.red());
                errors.push(error);
            }
        }
    }

    if started_count > 0 {
        config.save()?;
        println!("\n{}", format!("✓ Started {} forward(s)", started_count).green());
    }

    if !errors.is_empty() {
        anyhow::bail!("Some forwards failed to start");
    }

    Ok(())
}

fn delete_forwards(config: &mut Config, ids: &[String]) -> Result<()> {
    let mut deleted_count = 0;
    let mut errors = Vec::new();
    
    let ids_to_delete = resolve_ids(config, ids, "Deleting", &mut errors);
    
    // Delete all collected IDs
    for id in ids_to_delete {
        if let Some(forward) = config.remove_forward(&id) {
            if is_forward_running(&forward) {
                if let Some(pid) = forward.pid
                    && let Err(e) = kill_process(pid)
                {
                    eprintln!("{}", format!("  ⚠ Warning: {}", e).yellow());
                }
            } else {
                println!("  Process was not running");
            }
            println!("{} {} (localhost:{} → {}:{})", 
                     "✓ Deleted:".green(),
                     forward.id.dimmed(),
                     forward.local_port,
                     forward.host,
                     forward.remote_port);
            deleted_count += 1;
        } else {
            let error = format!("✗ Not found: {}", id);
            eprintln!("{}", error.red());
            errors.push(error);
        }
    }
    
    if deleted_count > 0 {
        config.save()?;
        println!("\n{}", format!("✓ Deleted {} forward(s)", deleted_count).green());
    }
    
    if !errors.is_empty() {
        anyhow::bail!("Some deletions failed");
    }
    
    Ok(())
}

fn kill_process(pid: u32) -> Result<()> {
    let output = std::process::Command::new("kill")
        .arg(pid.to_string())
        .output()
        .context("Failed to execute kill command")?;

    if output.status.success() {
        println!("  Stopped process: {}", pid);
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("No such process") {
            println!("  Process was already stopped");
            Ok(())
        } else {
            anyhow::bail!("Failed to kill process {}:{}", pid, stderr)
        }
    }
}

fn cleanup_dead_forwards(config: &mut Config) -> Result<()> {
    let mut removed_count = 0;
    let dead_ids: Vec<String> = config
        .forwards
        .values()
        .filter(|f| {
            if f.pid.is_some() {
                !is_forward_running(f)
            } else {
                false
            }
        })
        .map(|f| f.id.clone())
        .collect();
    
    for id in dead_ids {
        if let Some(forward) = config.remove_forward(&id) {
            println!("{} {} (PID: {})", 
                     "✓ Removed dead forward:".yellow(),
                     forward.id.dimmed(), 
                     forward.pid.unwrap());
            removed_count += 1;
        }
    }
    
    if removed_count > 0 {
        config.save()?;
        println!("\n{}", format!("✓ Cleaned up {} dead forward(s)", removed_count).green());
    } else {
        println!("{}", "No dead forwards found".dimmed());
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ports_valid() {
        let (local, remote) = parse_ports("8080:80").unwrap();
        assert_eq!(local, 8080);
        assert_eq!(remote, 80);

        let (local, remote) = parse_ports("1:65535").unwrap();
        assert_eq!(local, 1);
        assert_eq!(remote, 65535);
    }

    #[test]
    fn test_parse_ports_same_port() {
        let (local, remote) = parse_ports("3000").unwrap();
        assert_eq!(local, 3000);
        assert_eq!(remote, 3000);

        let (local, remote) = parse_ports("80").unwrap();
        assert_eq!(local, 80);
        assert_eq!(remote, 80);
    }

    #[test]
    fn test_parse_ports_bad_formats() {
        assert!(parse_ports("80:80:80").is_err());
        assert!(parse_ports("").is_err());
        assert!(parse_ports("abc:def").is_err());
        assert!(parse_ports("8080:").is_err());
        assert!(parse_ports(":80").is_err());
        assert!(parse_ports("foo").is_err());
        assert!(parse_ports("8080:abc").is_err());
        assert!(parse_ports("abc:8080").is_err());
    }

    #[test]
    fn test_parse_ports_out_of_range() {
        assert!(parse_ports("70000").is_err());
        assert!(parse_ports("8080:70000").is_err());
        assert!(parse_ports("70000:80").is_err());
        assert!(parse_ports("0:70000").is_err());
    }
}
