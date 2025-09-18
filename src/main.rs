use std::io::{self, Write};
use std::net::{IpAddr, SocketAddr, UdpSocket};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool};
use std::thread;

use chat::{handle_input, heartbeat_and_cleanup, receive_messages};
use network::{get_broadcast_info, get_local_ip, parse_multicast_ip};
use peer::{new_active_peers};

mod cli;
mod chat;
mod network;
mod peer;

fn main() -> io::Result<()> {
    let args = cli::parse_args();
    let port = args.port;
    let multicast_ip_str = args.multicast_ip;

    let local_ip = get_local_ip()?;
    let local_ipv4 = if let IpAddr::V4(ipv4) = local_ip {
        ipv4
    } else {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "Local IP must be IPv4 for multicast"));
    };
    let (netmask, broadcast) = get_broadcast_info(&local_ip)?;

    let mut multicast_addr = if let Some(ip_str) = multicast_ip_str {
        let multi_ip = parse_multicast_ip(&ip_str)?;
        Some(SocketAddr::new(IpAddr::V4(multi_ip), port))
    } else {
        None
    };

    println!("=== P2P UDP Chat ===");
    println!("Local IP: {}", local_ip);
    println!("Subnet mask: {}", netmask);
    println!("Broadcast-address: {}", broadcast);
    if let Some(addr) = &multicast_addr {
        println!("Multicast-address: {}", addr);
    }
    println!("Port: {}", port);
    println!("Commands: '/exit' - выход, '/peers' - список участников, '/join_multicast <IP>' - присоединиться к multicast, '/leave_multicast' - выйти из multicast");
    println!("Write a message...\n");
    print!("> ");
    io::stdout().flush()?;

    let socket = UdpSocket::bind(format!("0.0.0.0:{}", port))?;
    socket.set_broadcast(true)?;
    socket.set_multicast_loop_v4(false)?; // Отключаем multicast loop для изоляции

    if let Some(multi_addr) = &multicast_addr {
        if let IpAddr::V4(multi_ip) = multi_addr.ip() {
            socket.join_multicast_v4(&multi_ip, &local_ipv4)?;
            println!("Joined multicast group: {}", multi_ip);
        }
    }

    let active_peers = Arc::new(Mutex::new(new_active_peers(local_ip)));
    let should_exit = Arc::new(AtomicBool::new(false));

    let socket_clone = socket.try_clone()?;
    let active_peers_clone = Arc::clone(&active_peers);
    let should_exit_clone = Arc::clone(&should_exit);
    let local_ip_clone = local_ip;
    let multicast_addr_clone = multicast_addr;

    let receive_handle = thread::spawn(move || {
        let _ = receive_messages(socket_clone, active_peers_clone, should_exit_clone, local_ip_clone, multicast_addr_clone);
    });

    let socket_heartbeat = socket.try_clone()?;
    let active_peers_heartbeat = Arc::clone(&active_peers);
    let should_exit_heartbeat = Arc::clone(&should_exit);
    let heartbeat_handle = thread::spawn(move || {
        heartbeat_and_cleanup(socket_heartbeat, active_peers_heartbeat, should_exit_heartbeat, broadcast, port, local_ip, multicast_addr)
    });

    let socket_input = socket.try_clone()?;
    handle_input(socket_input, active_peers, should_exit, broadcast, port, local_ip, &mut multicast_addr, local_ipv4)?;

    if let Some(multi_addr) = multicast_addr {
        if let IpAddr::V4(multi_ip) = multi_addr.ip() {
            let _ = socket.leave_multicast_v4(&multi_ip, &local_ipv4);
        }
    }

    let _ = receive_handle.join();
    let _ = heartbeat_handle.join();
    println!("Program completed.");

    Ok(())
}