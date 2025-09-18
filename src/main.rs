use std::io::{self, Write};
use std::net::{UdpSocket};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool};
use std::thread;

use chat::{handle_input, heartbeat_and_cleanup, receive_messages};
use network::{get_broadcast_info, get_local_ip};
use peer::{new_active_peers};

mod cli;
mod chat;
mod network;
mod peer;

fn main() -> io::Result<()> {
    let args = cli::parse_args();
    let port = args.port;

    let local_ip = get_local_ip()?;
    let (netmask, broadcast) = get_broadcast_info(&local_ip)?;

    println!("=== P2P UDP Chat ===");
    println!("Local IP: {}", local_ip);
    println!("Subnet mask: {}", netmask);
    println!("Broadcast-address: {}", broadcast);
    println!("Port: {}", port);
    println!("Commands: '/exit' - выход, '/peers' - список участников");
    println!("Write a message...\n");
    print!("> ");
    io::stdout().flush()?;

    let socket = UdpSocket::bind(format!("0.0.0.0:{}", port))?;
    socket.set_broadcast(true)?;

    let active_peers = Arc::new(Mutex::new(new_active_peers(local_ip)));
    let should_exit = Arc::new(AtomicBool::new(false));

    let socket_clone = socket.try_clone()?;
    let active_peers_clone = Arc::clone(&active_peers);
    let should_exit_clone = Arc::clone(&should_exit);
    let local_ip_clone = local_ip;

    let receive_handle = thread::spawn(move || {
        let _ = receive_messages(socket_clone, active_peers_clone, should_exit_clone, local_ip_clone);
    });

    let socket_heartbeat = socket.try_clone()?;
    let active_peers_heartbeat = Arc::clone(&active_peers);
    let should_exit_heartbeat = Arc::clone(&should_exit);
    let heartbeat_handle = thread::spawn(move || {
        heartbeat_and_cleanup(socket_heartbeat, active_peers_heartbeat, should_exit_heartbeat, broadcast, port, local_ip)
    });

    handle_input(socket, active_peers, should_exit, broadcast, port, local_ip)?;

    let _ = receive_handle.join();
    let _ = heartbeat_handle.join();
    println!("Program completed.");

    Ok(())
}