use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct PeerInfo {
    pub ip: IpAddr,
    pub last_seen: Instant,
}

pub type ActivePeers = HashMap<IpAddr, PeerInfo>;

pub fn update_peer_activity(active_peers: &Arc<Mutex<ActivePeers>>, ip: IpAddr) {
    let mut peers = active_peers.lock().unwrap();
    peers.insert(ip, PeerInfo {
        ip,
        last_seen: Instant::now(),
    });
}

pub fn print_active_peers(active_peers: &HashMap<IpAddr, PeerInfo>, local_ip: IpAddr) {
    println!("\n=== Активные участники ({}) ===", active_peers.len());
    let mut sorted_peers: Vec<_> = active_peers.values().collect();
    sorted_peers.sort_by_key(|peer| peer.ip);
    for (i, peer_info) in sorted_peers.iter().enumerate() {
        if peer_info.ip == local_ip {
            println!("   {}. {} (You)", i + 1, peer_info.ip);
        } else {
            println!("   {}. {}", i + 1, peer_info.ip);
        }
    }
    println!("=================================\n");
}

pub fn new_active_peers(local_ip: IpAddr) -> ActivePeers {
    let mut peers = HashMap::new();
    peers.insert(local_ip, PeerInfo {
        ip: local_ip,
        last_seen: Instant::now(),
    });
    peers
}