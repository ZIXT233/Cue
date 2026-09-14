use std::io::{Read, Write};
use std::net::TcpStream;

pub fn run() -> Result<(), String> {
    let prompt = std::env::args().nth(2).unwrap_or_default();
    let addr = std::env::var("CUE_ASK_SOCKET").map_err(|_| "CUE_ASK_SOCKET missing".to_string())?;
    let mut stream = TcpStream::connect(addr).map_err(|e| e.to_string())?;
    stream.write_all(prompt.as_bytes()).map_err(|e| e.to_string())?;
    stream
        .shutdown(std::net::Shutdown::Write)
        .map_err(|e| e.to_string())?;
    let mut reply = String::new();
    stream.read_to_string(&mut reply).map_err(|e| e.to_string())?;
    print!("{reply}");
    Ok(())
}
