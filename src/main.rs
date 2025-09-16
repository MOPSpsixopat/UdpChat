use std::collections::HashSet;
use std::io::{self, BufRead, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::thread;

use if_addrs::{get_if_addrs, IfAddr};

use clap::Parser;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value_t = 12345)]
    port: u16,
}

fn main() -> io::Result<()> {
    let args = Args::parse();
    let port = args.port;

    // Автоопределение сетевых параметров
    let local_ip = get_local_ip()?;
    let (netmask, broadcast) = get_broadcast_info(&local_ip)?;

    println!("Локальный IP: {}", local_ip);
    println!("Сетевая маска: {}", netmask);
    println!("Broadcast-адрес: {}", broadcast);
    println!("Порт: {}", port);
    println!("Запущенные приложения в сети будут отображаться по мере получения сообщений.");

    // Создание UDP сокета
    let socket = UdpSocket::bind(format!("0.0.0.0:{}", port))?;
    socket.set_broadcast(true)?;

    let active_peers = Arc::new(Mutex::new(HashSet::new()));
    active_peers.lock().unwrap().insert(local_ip);

    let socket_clone = socket.try_clone()?;
    let active_peers_clone = Arc::clone(&active_peers);
    let receive_handle = thread::spawn(move || receive_messages(socket_clone, active_peers_clone));

    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let message = line?;
        if message.trim().is_empty() {
            continue;
        }
        if message.to_lowercase() == "exit" {
            break;
        }

        let full_msg = format!("{}: {}", local_ip, message);

        // Отправка на broadcast-адрес
        let broadcast_addr = SocketAddr::new(IpAddr::from(broadcast), port);
        socket.send_to(full_msg.as_bytes(), broadcast_addr)?;

        print_active_peers(&active_peers.lock().unwrap(), &mut io::stdout());
    }

    receive_handle.join().unwrap();

    Ok(())
}

fn receive_messages(socket: UdpSocket, active_peers: Arc<Mutex<HashSet<IpAddr>>>) {
    let mut buf = [0; 1024];
    loop {
        match socket.recv_from(&mut buf) {
            Ok((len, src_addr)) => {
                let msg = String::from_utf8_lossy(&buf[..len]);
                if let Ok(src_ip) = IpAddr::from_str(src_addr.ip().to_string().as_str()) {
                    let mut peers = active_peers.lock().unwrap();
                    peers.insert(src_ip);
                }
                println!("\n[Получено от {}]: {}", src_addr.ip(), msg.trim());
                io::stdout().flush().unwrap();
            }
            Err(_) => {
                // Игнорируем ошибки (например, таймауты)
            }
        }
    }
}

fn print_active_peers(active_peers: &HashSet<IpAddr>, stdout: &mut dyn Write) {
    stdout.write_all(b"\nActive peers: ").unwrap();
    for ip in active_peers {
        stdout.write_fmt(format_args!("{} ", ip)).unwrap();
    }
    stdout.write_all(b"\n").unwrap();
    stdout.flush().unwrap();
}

fn get_local_ip() -> io::Result<IpAddr> {
    for iface in get_if_addrs()? {
        if let IfAddr::V4(addr) = iface.addr {
            if !addr.ip.is_loopback() && addr.ip.is_private() {
                return Ok(IpAddr::V4(addr.ip));
            }
        }
    }
    Ok(IpAddr::V4(Ipv4Addr::LOCALHOST)) // Fallback
}

fn get_broadcast_info(ip: &IpAddr) -> io::Result<(Ipv4Addr, Ipv4Addr)> {
    if let IpAddr::V4(ipv4) = ip {
        for iface in get_if_addrs()? {
            if let IfAddr::V4(addr) = iface.addr {
                if addr.ip == *ipv4 {
                    let mask = Ipv4Addr::new(255, 255, 255, 0); // Предполагаем /24
                    let broadcast = calculate_broadcast(&addr.ip, mask);
                    return Ok((mask, broadcast));
                }
            }
        }
        Ok((Ipv4Addr::new(255, 255, 255, 0), Ipv4Addr::new(192, 168, 1, 255)))
    } else {
        Ok((Ipv4Addr::new(255, 255, 255, 0), Ipv4Addr::new(192, 168, 1, 255)))
    }
}

fn calculate_broadcast(ip: &Ipv4Addr, netmask: Ipv4Addr) -> Ipv4Addr {
    let ip_bytes = ip.octets();
    let mask_bytes = netmask.octets();
    let broadcast_bytes = [
        ip_bytes[0] | (mask_bytes[0] ^ 255),
        ip_bytes[1] | (mask_bytes[1] ^ 255),
        ip_bytes[2] | (mask_bytes[2] ^ 255),
        ip_bytes[3] | (mask_bytes[3] ^ 255),
    ];
    Ipv4Addr::from(broadcast_bytes)
}