use std::collections::HashSet;
use std::io::{self, BufRead, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::thread;
use std::sync::atomic::{AtomicBool, Ordering};

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

    println!("=== P2P UDP Chat ===");
    println!("Локальный IP: {}", local_ip);
    println!("Сетевая маска: {}", netmask);
    println!("Broadcast-адрес: {}", broadcast);
    println!("Порт: {}", port);
    println!("Команды: 'exit' - выход, '/peers' - список участников");
    println!("Начинайте печатать сообщения...\n");

    // Создание UDP сокета
    let socket = UdpSocket::bind(format!("0.0.0.0:{}", port))?;
    socket.set_broadcast(true)?;

    let active_peers = Arc::new(Mutex::new(HashSet::new()));
    let should_exit = Arc::new(AtomicBool::new(false));

    // Клонируем для потока получения сообщений
    let socket_clone = socket.try_clone()?;
    let active_peers_clone = Arc::clone(&active_peers);
    let should_exit_clone = Arc::clone(&should_exit);
    let local_ip_clone = local_ip;

    let receive_handle = thread::spawn(move || {
        receive_messages(socket_clone, active_peers_clone, should_exit_clone, local_ip_clone)
    });

    // Основной цикл ввода
    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let message = line?;
        let trimmed = message.trim();

        if trimmed.is_empty() {
            continue;
        }

        // Обработка команд
        match trimmed.to_lowercase().as_str() {
            "exit" => {
                println!("👋 Завершение работы...");
                should_exit.store(true, Ordering::Relaxed);
                break;
            }
            "/peers" => {
                print_active_peers(&active_peers.lock().unwrap());
                continue;
            }
            _ => {
                // Обычное сообщение
                let full_msg = format!("{}:{}", local_ip, trimmed);

                // Отправка на broadcast-адрес
                let broadcast_addr = SocketAddr::new(IpAddr::from(broadcast), port);
                if let Err(e) = socket.send_to(full_msg.as_bytes(), broadcast_addr) {
                    println!("Ошибка отправки: {}", e);
                }
            }
        }
    }

    // Ждем завершения потока получения сообщений
    let _ = receive_handle.join();
    println!("Программа завершена.");

    Ok(())
}

fn receive_messages(
    socket: UdpSocket,
    active_peers: Arc<Mutex<HashSet<IpAddr>>>,
    should_exit: Arc<AtomicBool>,
    local_ip: IpAddr
) {
    let mut buf = [0; 1024];

    // Устанавливаем таймаут для recv_from, чтобы проверять should_exit
    socket.set_read_timeout(Some(std::time::Duration::from_millis(500))).ok();

    loop {
        if should_exit.load(Ordering::Relaxed) {
            break;
        }

        match socket.recv_from(&mut buf) {
            Ok((len, src_addr)) => {
                let msg = String::from_utf8_lossy(&buf[..len]);
                let src_ip = src_addr.ip();

                // Добавляем IP отправителя в список активных участников
                if let Ok(parsed_ip) = IpAddr::from_str(&src_ip.to_string()) {
                    let mut peers = active_peers.lock().unwrap();
                    peers.insert(parsed_ip);
                }

                // Парсим сообщение (формат: IP:сообщение)
                let msg_str = msg.trim();
                if let Some(colon_pos) = msg_str.find(':') {
                    let (sender_ip_str, content) = msg_str.split_at(colon_pos);
                    let content = &content[1..]; // убираем двоеточие

                    // Показываем только сообщения от других участников
                    if src_ip != local_ip {
                        println!("💬 {}: {}", sender_ip_str, content);
                        print!("Введите сообщение: ");
                        io::stdout().flush().unwrap();
                    }
                } else {
                    // Если формат не распознан, показываем как есть (только от других)
                    if src_ip != local_ip {
                        println!("[{}]: {}", src_ip, msg_str);
                        print!("Введите сообщение: ");
                        io::stdout().flush().unwrap();
                    }
                }
            }
            Err(ref e) if e.kind() == io::ErrorKind::TimedOut => {
                // Таймаут - это нормально, продолжаем
                continue;
            }
            Err(_) => {
                // Другие ошибки игнорируем
            }
        }
    }
}

fn print_active_peers(active_peers: &HashSet<IpAddr>) {
    println!("\n=== Активные участники ({}) ===", active_peers.len());
    for (i, ip) in active_peers.iter().enumerate() {
        println!("   {}. {}", i + 1, ip);
    }
    println!("==================================\n");
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