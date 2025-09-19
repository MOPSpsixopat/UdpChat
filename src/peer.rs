use std::collections::HashMap;
use std::net::IpAddr;
use std::time::Instant;
use std::sync::{Arc, Mutex};
use std::collections::HashSet;

pub type ActivePeers = HashMap<IpAddr, PeerInfo>;

#[derive(Debug)]
pub struct PeerInfo {
    pub last_seen: Instant,
}

pub fn new_active_peers(local_ip: IpAddr) -> ActivePeers {
    let mut peers = HashMap::new();
    peers.insert(local_ip, PeerInfo {
        last_seen: Instant::now(),
    });
    peers
}

pub fn update_peer_activity(peers: &Arc<Mutex<ActivePeers>>, ip: IpAddr) {
    let mut peers = peers.lock().unwrap();
    peers.entry(ip).and_modify(|e| {
        e.last_seen = Instant::now();
    }).or_insert(PeerInfo {
        last_seen: Instant::now(),
    });
}

pub fn print_active_peers(peers: &ActivePeers, local_ip: IpAddr, ignored_peers: &Arc<Mutex<HashSet<IpAddr>>>) {
    let ignored = ignored_peers.lock().unwrap();
    println!("\n=== Активные участники ({}) ===", peers.len());
    for (i, (ip, _)) in peers.iter().enumerate() {
        let mut label = String::new();
        if *ip == local_ip {
            label.push_str(" (You)");
        }
        if ignored.contains(ip) {
            label.push_str(" [Muted]");
        }
        println!("   {}. {}{}", i + 1, ip, label);
    }
    println!("==============================");
}