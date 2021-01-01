use std::net::{Ipv4Addr, IpAddr, SocketAddr, TcpListener, TcpStream};
use std::thread;
use std::time::Duration;
use std::sync::{Arc, Mutex};
use std::io::{Read, Write};
use std::collections::HashMap;
use std::io;
use std::env;
use std::str::FromStr;

#[cfg(debug_assertions)]
macro_rules! debug {
  ($( $args:expr ), *) => { println!( $( $args ), * ); }
}

#[cfg(not(debug_assertions))]
macro_rules! debug {
  ($( $args:expr ),*) => {}
}

#[derive(Clone)]
struct Conn {
  stream: Arc<Mutex<TcpStream>>,
  connections: Connections,
}

impl Conn {
  fn read(&self, mut buf: &mut [u8]) -> std::io::Result<usize> {
    self.stream.lock().unwrap().read(&mut buf)
  }
  fn write(&self, buf: &[u8]) -> std::io::Result<usize> {
    match self.stream.try_lock() {
      Ok(mut lock) => {lock.write(buf)},
      Err(_e) => {Ok(0)},
    }
  }
  fn take_error(&self) -> std::io::Result<Option<std::io::Error>> {
    self.stream.lock().unwrap().take_error()
  }
}

#[derive(Clone)]
struct Connections {
  counter: Arc<Mutex<u32>>,
  connections: Arc<Mutex<HashMap<u32, Conn>>>,
}

impl Connections {
  fn store(&self, conn: Conn) -> u32 {
    let mut counter = self.counter.lock().unwrap();
    *counter += 1;
    let id = *counter;
    self.connections.lock().unwrap().insert(id, conn);
    return id;
  }
  fn remove(&self, id: u32) {
    self.connections.lock().unwrap().remove(&id);
  }
  fn broadcast(&self, buf: &[u8]) {
    for (id, conn) in self.connections.lock().unwrap().iter() {
      match conn.write(&buf) {
        Ok(size) => { debug!("[{}] Wrote {} to connection...", id, size); },
        Err(e) => { debug!("[{}] Error writing to connection {}", id, e); },
      }
    }
  }
  pub fn new() -> Connections {
    Connections { 
      counter: Arc::new(Mutex::new(0)),
      connections: Arc::new(Mutex::new(HashMap::new())),
    }
  }
}

fn handle_stream(conn: Conn) -> std::io::Result<()> {
  let id = conn.connections.store(conn.clone());
  println!("[{}] Connected...", id);
  loop {
    match conn.take_error() {
      Ok(_) => {},
      Err(_e) => {
        break;
      },
    }
    let mut buf = vec![0; 1024];
    match conn.read(&mut buf) {
      Ok(read) if read == 0 => {break;},
      Ok(_read) => {
        let string = String::from_utf8_lossy(&buf);
        let mut message = format!("[{}] ", id);
        message.push_str(&string);
        conn.connections.broadcast(message.as_bytes());
      },
      Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
        thread::sleep(Duration::from_millis(10));
      },
      Err(_e) => {break},
    };
  }
  conn.connections.remove(id);
  println!("[{}] Disconnected...", id);

  Ok(())
}

fn main() -> std::io::Result<()> {
  let connections = Connections::new();

  let args: Vec<String> = env::args().collect();
  let port = if args.len() > 1 {
    args[1].parse::<u16>().expect("Port must be a number")
  } else {
    1300
  };
  let addr = if args.len() > 2 {
    IpAddr::from_str(&args[2]).expect("Address must be valid")
  } else {
    IpAddr::from(Ipv4Addr::new(127,0,0,1))
  };
  
  let socket_addr = SocketAddr::from((addr,port)); 
  let socket = TcpListener::bind(socket_addr)?;
  socket.set_nonblocking(true).expect("Unable to set non-blocking");
  println!("Listening on {}", socket_addr);

  for stream in socket.incoming() {
    match stream {
      Ok(stream) => {
        stream.set_nonblocking(true).expect("Unable to set non-blocking");
        let conn = Conn { 
          stream: Arc::new(Mutex::new(stream)),
          connections: connections.clone(),
        };
        thread::spawn(move || handle_stream(conn));
      },
      Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
        thread::sleep(Duration::from_millis(10));
        continue;
      },
      Err(e) => panic!("Encountered IO error: {}", e),
    }
  }

  Ok(())
}
