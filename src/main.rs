use std::collections::{HashMap};
use std::io::{self, BufRead, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::{Arc, Mutex};
use std::thread;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use if_addrs::{get_if_addrs, IfAddr};
use clap::Parser;

#[derive(Debug, Clone)]
struct PeerInfo {
    ip: IpAddr,
    last_seen: Instant,
}

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
    println!("Команды: '/exit' - выход, '/peers' - список участников");
    println!("Начинайте печатать сообщения...\n");
    print!("> ");
    io::stdout().flush()?;

    // Создание UDP сокета
    let socket = UdpSocket::bind(format!("0.0.0.0:{}", port))?;
    socket.set_broadcast(true)?;

    // Используем HashMap для отслеживания времени последней активности
    let active_peers = Arc::new(Mutex::new(HashMap::new()));
    let should_exit = Arc::new(AtomicBool::new(false));

    // Клонируем для потока получения сообщений
    let socket_clone = socket.try_clone()?;
    let active_peers_clone = Arc::clone(&active_peers);
    let should_exit_clone = Arc::clone(&should_exit);
    let local_ip_clone = local_ip;

    let receive_handle = thread::spawn(move || {
        let _ = receive_messages(socket_clone, active_peers_clone, should_exit_clone, local_ip_clone);
    });

    // Запускаем поток для отправки heartbeat и очистки неактивных участников
    let socket_heartbeat = socket.try_clone()?;
    let active_peers_heartbeat = Arc::clone(&active_peers);
    let should_exit_heartbeat = Arc::clone(&should_exit);
    let heartbeat_handle = thread::spawn(move || {
        heartbeat_and_cleanup(socket_heartbeat, active_peers_heartbeat, should_exit_heartbeat, broadcast, port, local_ip)
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
        match trimmed {
            "/exit" => {
                // Отправляем уведомление о выходе
                let leave_msg = format!("{}:LEAVE", local_ip);
                let broadcast_addr = SocketAddr::new(IpAddr::from(broadcast), port);
                let _ = socket.send_to(leave_msg.as_bytes(), broadcast_addr);

                println!("Завершение работы...");
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
                // Обычное сообщение
                let full_msg = format!("{}:CHAT:{}", local_ip, trimmed);

                // Отправка на broadcast-адрес
                let broadcast_addr = SocketAddr::new(IpAddr::from(broadcast), port);
                if let Err(e) = socket.send_to(full_msg.as_bytes(), broadcast_addr) {
                    println!("Ошибка отправки: {}", e);
                }
                print!("> ");
                io::stdout().flush()?;
            }
        }
    }

    // Ждем завершения потоков
    let _ = receive_handle.join();
    let _ = heartbeat_handle.join();
    println!("Программа завершена.");

    Ok(())
}

fn receive_messages(
    socket: UdpSocket,
    active_peers: Arc<Mutex<HashMap<IpAddr, PeerInfo>>>,
    should_exit: Arc<AtomicBool>,
    local_ip: IpAddr
) -> io::Result<()> {
    let mut buf = [0; 1024];

    // Устанавливаем таймаут для recv_from, чтобы проверять should_exit
    socket.set_read_timeout(Some(Duration::from_millis(500))).ok();

    loop {
        if should_exit.load(Ordering::Relaxed) {
            break;
        }

        match socket.recv_from(&mut buf) {
            Ok((len, src_addr)) => {
                let msg = String::from_utf8_lossy(&buf[..len]);
                let src_ip = src_addr.ip();

                // Игнорируем свои сообщения
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
                                // Обновляем информацию об участнике
                                update_peer_activity(&active_peers, src_ip);

                                println!("\r{}: {}", sender_ip_str, content);
                                print!("> ");
                                let _ = io::stdout().flush();
                            }
                        }
                        "LEAVE" => {
                            // Удаляем участника из списка
                            let mut peers = active_peers.lock().unwrap();
                            peers.remove(&src_ip);
                            println!("\r{} покинул чат", sender_ip_str);
                            print_active_peers(&peers, local_ip);
                            print!("> ");
                            let _ = io::stdout().flush();
                        }
                        "HEARTBEAT" => {
                            // Обновляем информацию об участнике
                            update_peer_activity(&active_peers, src_ip);
                        }
                        _ => {
                            // Неизвестный тип сообщения, обрабатываем как обычное
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
                // Другие ошибки игнорируем
            }
        }
    }
    Ok(())
}

fn update_peer_activity(active_peers: &Arc<Mutex<HashMap<IpAddr, PeerInfo>>>, ip: IpAddr) {
    let mut peers = active_peers.lock().unwrap();
    peers.insert(ip, PeerInfo {
        ip,
        last_seen: Instant::now(),
    });
}

fn heartbeat_and_cleanup(
    socket: UdpSocket,
    active_peers: Arc<Mutex<HashMap<IpAddr, PeerInfo>>>,
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

        thread::sleep(Duration::from_secs(5));

        // Отправляем heartbeat
        let heartbeat_msg = format!("{}:HEARTBEAT", local_ip);
        let _ = socket.send_to(heartbeat_msg.as_bytes(), broadcast_addr);

        // Очищаем неактивных участников (не получали сообщений более 30 секунд)
        let mut peers = active_peers.lock().unwrap();
        let now = Instant::now();
        let initial_count = peers.len();

        peers.retain(|_, peer_info| {
            now.duration_since(peer_info.last_seen) < Duration::from_secs(30)
        });

        // Если кто-то исчез, показываем обновленный список
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

fn print_active_peers(active_peers: &HashMap<IpAddr, PeerInfo>, local_ip: IpAddr) {
    println!("\n=== Активные участники ({}) ===", active_peers.len());
    for (i, peer_info) in active_peers.values().enumerate() {
        if peer_info.ip == local_ip {
            println!("   {}. {} (You)", i + 1, peer_info.ip);
        } else {
            println!("   {}. {}", i + 1, peer_info.ip);
        }
    }
    println!("=================================\n");
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