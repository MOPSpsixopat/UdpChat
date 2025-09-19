use std::io;
use std::net::{IpAddr, Ipv4Addr};

use if_addrs::{get_if_addrs, IfAddr};

pub fn get_local_ip() -> io::Result<IpAddr> {
    for iface in get_if_addrs()? {
        if let IfAddr::V4(addr) = iface.addr {
            if !addr.ip.is_loopback() && addr.ip.is_private() {
                return Ok(IpAddr::V4(addr.ip));
            }
        }
    }
    Ok(IpAddr::V4(Ipv4Addr::LOCALHOST)) // Fallback
}

pub fn get_broadcast_info(ip: &IpAddr) -> io::Result<(Ipv4Addr, Ipv4Addr)> {
    if let IpAddr::V4(ipv4) = ip {
        for iface in get_if_addrs()? {
            if let IfAddr::V4(addr) = iface.addr {
                if addr.ip == *ipv4 {
                    let mask = Ipv4Addr::new(255, 255, 255, 0);
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

pub fn calculate_broadcast(ip: &Ipv4Addr, netmask: Ipv4Addr) -> Ipv4Addr {
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

pub fn parse_multicast_ip(ip_str: &str) -> io::Result<Ipv4Addr> {
    let ip: IpAddr = ip_str
        .parse()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, format!("Failed to parse IP: {}", e)))?;
    if let IpAddr::V4(ipv4) = ip {
        if ipv4.octets()[0] >= 224 && ipv4.octets()[0] <= 239 {
            Ok(ipv4)
        } else {
            Err(io::Error::new(io::ErrorKind::InvalidInput, "Not a valid multicast IP"))
        }
    } else {
        Err(io::Error::new(io::ErrorKind::InvalidInput, "Multicast IP must be IPv4"))
    }
}