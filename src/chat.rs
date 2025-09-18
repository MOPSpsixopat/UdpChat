use std::io::{self, BufRead};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use std::io::Write;
use std::thread;

use crate::network::parse_multicast_ip;
use crate::peer::{ActivePeers, print_active_peers, update_peer_activity};

pub fn receive_messages(
    socket: UdpSocket,
    active_peers: Arc<Mutex<ActivePeers>>,
    should_exit: Arc<AtomicBool>,
    local_ip: IpAddr,
    multicast_addr: Option<SocketAddr>
) -> io::Result<()> {
    let mut buf = [0; 1024];

    socket.set_read_timeout(Some(Duration::from_millis(500))).ok();

    loop {
        if should_exit.load(Ordering::Relaxed) {
            break;
        }

        match socket.recv_from(&mut buf) {
            Ok((len, src_addr)) => {
                let src_ip = src_addr.ip();
                
                let is_multicast_mode = multicast_addr.is_some();
                let is_multicast_src = match src_ip {
                    IpAddr::V4(ipv4) => ipv4.octets()[0] >= 224 && ipv4.octets()[0] <= 239,
                    _ => false,
                };
                
                if is_multicast_mode {
                    if let Some(multi_addr) = multicast_addr {
                        if src_ip != multi_addr.ip() {
                            continue;
                        }
                    }
                } else if is_multicast_src {
                    continue;
                }

                if src_ip == local_ip {
                    continue;
                }

                let msg = String::from_utf8_lossy(&buf[..len]);
                let msg_str = msg.trim();

                let parts: Vec<&str> = msg_str.splitn(3, ':').collect();
                if parts.len() >= 2 {
                    let sender_ip_str = parts[0];
                    let msg_type = parts[1];

                    match msg_type {
                        "CHAT" => {
                            if parts.len() >= 3 {
                                let content = parts[2];
                                update_peer_activity(&active_peers, src_ip);

                                println!("\r{}: {}", sender_ip_str, content);
                                print!("> ");
                                let _ = io::stdout().flush();
                            }
                        }
                        "LEAVE" => {
                            let mut peers = active_peers.lock().unwrap();
                            peers.remove(&src_ip);
                            println!("\r{} leave chat", sender_ip_str);
                            print_active_peers(&peers, local_ip);
                            print!("> ");
                            let _ = io::stdout().flush();
                        }
                        "HEARTBEAT" => {
                            update_peer_activity(&active_peers, src_ip);
                        }
                        _ => {
                            update_peer_activity(&active_peers, src_ip);
                            println!("\r[{}]: {}", src_ip, msg_str);
                            print!("> ");
                            let _ = io::stdout().flush();
                        }
                    }
                }
            }
            Err(ref e) if e.kind() == io::ErrorKind::TimedOut => {
                continue;
            }
            Err(_) => {
                // Игнорируем другие ошибки
            }
        }
    }
    Ok(())
}

pub fn heartbeat_and_cleanup(
    socket: UdpSocket,
    active_peers: Arc<Mutex<ActivePeers>>,
    should_exit: Arc<AtomicBool>,
    broadcast: Ipv4Addr,
    port: u16,
    local_ip: IpAddr,
    multicast_addr: Option<SocketAddr>
) {
    let broadcast_addr = SocketAddr::new(IpAddr::V4(broadcast), port);
    let target_addr = multicast_addr.unwrap_or(broadcast_addr);

    loop {
        if should_exit.load(Ordering::Relaxed) {
            break;
        }

        thread::sleep(Duration::from_secs(5));

        let heartbeat_msg = format!("{}:HEARTBEAT", local_ip);
        let _ = socket.send_to(heartbeat_msg.as_bytes(), &target_addr);

        let mut peers = active_peers.lock().unwrap();
        let now = Instant::now();
        let initial_count = peers.len();

        peers.retain(|_, peer_info| {
            now.duration_since(peer_info.last_seen) < Duration::from_secs(60)
        });

        peers.entry(local_ip).or_insert(crate::peer::PeerInfo {
            ip: local_ip,
            last_seen: Instant::now(),
        });

        if peers.len() < initial_count && !should_exit.load(Ordering::Relaxed) {
            drop(peers); // Освобождаем мьютекс
            let peers = active_peers.lock().unwrap();
            print!("\r");
            print_active_peers(&peers, local_ip);
            print!("> ");
            let _ = io::stdout().flush();
        }
    }
}

pub fn handle_input(
    socket: UdpSocket,
    active_peers: Arc<Mutex<ActivePeers>>,
    should_exit: Arc<AtomicBool>,
    broadcast: Ipv4Addr,
    port: u16,
    local_ip: IpAddr,
    multicast_addr: &mut Option<SocketAddr>,
    local_ipv4: Ipv4Addr
) -> io::Result<()> {
    let mut is_multicast = multicast_addr.is_some();
    let broadcast_addr = SocketAddr::new(IpAddr::V4(broadcast), port);

    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let message = line?;
        let trimmed = message.trim();

        if trimmed.is_empty() {
            continue;
        }

        match trimmed {
            "/exit" => {
                let target_addr = multicast_addr.as_ref().unwrap_or(&broadcast_addr);
                let leave_msg = format!("{}:LEAVE", local_ip);
                let _ = socket.send_to(leave_msg.as_bytes(), target_addr);

                println!("Shutdown...");
                should_exit.store(true, Ordering::Relaxed);
                break;
            }
            "/peers" => {
                print_active_peers(&active_peers.lock().unwrap(), local_ip);
                print!("> ");
                io::stdout().flush()?;
                continue;
            }
            "/join_multicast" => {
                if is_multicast {
                    println!("Already in multicast group.");
                } else {
                    println!("Usage: /join_multicast <multicast_ip> (e.g., 224.0.0.1)");
                }
                print!("> ");
                io::stdout().flush()?;
                continue;
            }
            cmd if cmd.starts_with("/join_multicast ") => {
                let ip_str = cmd.strip_prefix("/join_multicast ").unwrap().trim();
                match parse_multicast_ip(ip_str) {
                    Ok(multi_ip) => {
                        let multi_addr = SocketAddr::new(IpAddr::V4(multi_ip), port);
                        if let Err(e) = socket.join_multicast_v4(&multi_ip, &local_ipv4) {
                            println!("Failed to join multicast: {}", e);
                        } else {
                            *multicast_addr = Some(multi_addr);
                            is_multicast = true;
                            println!("Joined multicast group: {}", multi_ip);
                        }
                    }
                    Err(e) => {
                        println!("Invalid multicast IP: {}", e);
                    }
                }
                print!("> ");
                io::stdout().flush()?;
                continue;
            }
            "/leave_multicast" => {
                if let Some(multi_addr) = multicast_addr.take() {
                    if let IpAddr::V4(multi_ip) = multi_addr.ip() {
                        if let Err(e) = socket.leave_multicast_v4(&multi_ip, &local_ipv4) {
                            println!("Failed to leave multicast: {}", e);
                        } else {
                            is_multicast = false;
                            println!("Left multicast group: {}", multi_ip);
                        }
                    }
                } else {
                    println!("Not in multicast group.");
                }
                print!("> ");
                io::stdout().flush()?;
                continue;
            }
            _ => {
                let target_addr = multicast_addr.as_ref().unwrap_or(&broadcast_addr);
                let full_msg = format!("{}:CHAT:{}", local_ip, trimmed);

                if let Err(e) = socket.send_to(full_msg.as_bytes(), target_addr) {
                    println!("Sending error: {}", e);
                }
                print!("> ");
                io::stdout().flush()?;
            }
        }
    }
    Ok(())
}