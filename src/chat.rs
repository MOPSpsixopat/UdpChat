use std::io::{self, BufRead};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use std::io::Write;

use crate::peer::{ActivePeers, print_active_peers, update_peer_activity};

pub fn receive_messages(
    socket: UdpSocket,
    active_peers: Arc<Mutex<ActivePeers>>,
    should_exit: Arc<AtomicBool>,
    local_ip: IpAddr
) -> io::Result<()> {
    let mut buf = [0; 1024];

    socket.set_read_timeout(Some(Duration::from_millis(500))).ok();

    loop {
        if should_exit.load(Ordering::Relaxed) {
            break;
        }

        match socket.recv_from(&mut buf) {
            Ok((len, src_addr)) => {
                let msg = String::from_utf8_lossy(&buf[..len]);
                let src_ip = src_addr.ip();

                if src_ip == local_ip {
                    continue;
                }

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
                //ignore another errors
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
    local_ip: IpAddr
) {
    let broadcast_addr = SocketAddr::new(IpAddr::from(broadcast), port);

    loop {
        if should_exit.load(Ordering::Relaxed) {
            break;
        }

        std::thread::sleep(Duration::from_secs(5));

        let heartbeat_msg = format!("{}:HEARTBEAT", local_ip);
        let _ = socket.send_to(heartbeat_msg.as_bytes(), broadcast_addr);

        let mut peers = active_peers.lock().unwrap();
        let now = Instant::now();
        let initial_count = peers.len();

        peers.retain(|_, peer_info| {
            now.duration_since(peer_info.last_seen) < Duration::from_secs(60)
        });

        // Обновляем время последнего появления локального IP
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
    local_ip: IpAddr
) -> io::Result<()> {
    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let message = line?;
        let trimmed = message.trim();

        if trimmed.is_empty() {
            continue;
        }

        match trimmed {
            "/exit" => {
                let leave_msg = format!("{}:LEAVE", local_ip);
                let broadcast_addr = SocketAddr::new(IpAddr::from(broadcast), port);
                let _ = socket.send_to(leave_msg.as_bytes(), broadcast_addr);

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
            _ => {
                let full_msg = format!("{}:CHAT:{}", local_ip, trimmed);

                let broadcast_addr = SocketAddr::new(IpAddr::from(broadcast), port);
                if let Err(e) = socket.send_to(full_msg.as_bytes(), broadcast_addr) {
                    println!("Sending error: {}", e);
                }
                print!("> ");
                io::stdout().flush()?;
            }
        }
    }
    Ok(())
}