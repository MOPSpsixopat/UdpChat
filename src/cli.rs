use clap::Parser;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    #[arg(short, long, default_value_t = 12345)]
    pub port: u16,
}

pub fn parse_args() -> Args {
    Args::parse()
}