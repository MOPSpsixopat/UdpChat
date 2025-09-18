use clap::Parser;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    #[arg(short, long, default_value_t = 12345)]
    pub port: u16,

    #[arg(long, help = "Multicast IP address to join (e.g., 224.0.0.1), optional")]
    pub multicast_ip: Option<String>,
}

pub fn parse_args() -> Args {
    Args::parse()
}