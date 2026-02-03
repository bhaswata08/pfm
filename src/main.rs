use std::io;
use clap_complete::{generate, Shell};
use clap::{Parser, Subcommand, CommandFactory};
use colored::Colorize;
use anyhow::{Context, Result};

mod config;
use config::{Config, PortForward};

mod port;
mod ssh;

use ssh::SshTunnel;

#[derive(Parser)]
#[command(name = "pfm")]
#[command(about = "Port forward manager")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Add a new SSH port forward
    /// 
    /// Examples:
    ///   pfm add user@server.com 8080:80
    ///   pfm add server.com 3000
    Add {
        /// SSH host (user@hostname)
        host: String,
        /// Port mapping (local:remote or just local for same port)
        ports: String
    },
    /// List all configured port forwards
    List,
    /// Delete port forward(s)
    /// 
    /// Examples:
    ///   pfm delete 0 1 2    # Delete forwards at index 0, 1, 2
    ///   pfm delete all      # Delete all forwards
    Delete {
        /// Forward indices or 'all'
        ids: Vec<String>,
    },
    /// Remove forwards whose SSH processes have died
    Cleanup,
    /// Generate shell completions
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: Shell,
    }
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
        &mut io::stdout()
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

fn add_forward(config: &mut Config, host: &str, ports: &str) -> Result<()> {
    let (mut local, remote) = parse_ports(ports)?;

    let original_port = local;
    if !port::is_port_available(local) {
        println!("{}", format!("Port {} is already in use", local).yellow());

        if let Some(new_port) = port::find_available_port(local+1) {
            local = new_port;
            println!("{}", format!("Using port {} instead", local).green());
        } else {
            anyhow::bail!("No available ports found!");
        }
    }
    let tunnel = SshTunnel::start(host, local, remote)?;
    let pid = tunnel.pid();

    std::mem::forget(tunnel);

    let id  = format!("{}_{}_{}",
        host.replace("@", "_at_"),
        local,
        remote);
    let forward = PortForward {
        id: id.clone(),
        host: host.to_string(),
        local_port: local,
        remote_port: remote,
        pid: Some(pid)
    };
    config.add_forward(forward);
    config.save()?;

    println!("\n{}", "✓ Port forward created!".green().bold());
    println!("{}", format!("  ID: {}", id).cyan());
    println!("  {}:{} → {}:{}", 
             "localhost".dimmed(), 
             local.to_string().cyan(), 
             host.cyan(), 
             remote.to_string().cyan());
    println!("  {}: {}", "PID".cyan(), pid);

    if original_port != local {
            println!("{}", format!("\n⚠ Port remapped from {} to {}", original_port, local).yellow());
        }
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
        .filter(|f| f.pid.map(port::is_process_running).unwrap_or(false))
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
            let status = if port::is_process_running(pid) {
                "● Running".green()
            } else {
                "○ Stopped".yellow()
            };
            println!("  {}:   {} ({})", "PID".cyan(), pid, status);
        }
        
        println!();
    }
}

fn delete_forwards(config: &mut Config, ids: &[String]) -> Result<()> {
    let mut deleted_count = 0;
    let mut errors = Vec::new();
    
    // Check for "all" keyword
    let ids_to_delete: Vec<String> = if ids.len() == 1 && ids[0] == "all" {
        println!("{}", format!("Deleting all {} forward(s)...\n", config.forwards.len()).yellow());
        config.forwards.keys().cloned().collect()
    } else {
        // Resolve indices to IDs
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
    };
    
    // Delete all collected IDs
    for id in ids_to_delete {
        if let Some(forward) = config.remove_forward(&id) {
            if let Some(pid) = forward.pid {
                if let Err(e) = kill_process(pid) {
                    eprintln!("{}", format!("  ⚠ Warning: {}", e).yellow());
                }
            }
            println!("{} {} ({}:{} → {}:{})", 
                     "✓ Deleted:".green(),
                     forward.id.dimmed(),
                     forward.local_port,
                     forward.host,
                     forward.remote_port,
                     forward.host);
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
            println!("    Process {} was already stopped", pid);
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
            if let Some(pid) = f.pid {
                !port::is_process_running(pid)
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
            removed_count += 1
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
