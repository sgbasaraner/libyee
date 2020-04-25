use std::collections::{HashMap, HashSet};
use std::net::UdpSocket;
use std::{thread, time};
use std::sync::mpsc::{Sender, Receiver, channel};
use std::str;
use crate::BulbMap;
use crate::bulb::Bulb;

fn to_map(response: &str) -> BulbMap {
    let mut map = HashMap::new();

    let lines = response.split("\r\n");

    for line in lines {
        let mut splitter = line.splitn(2, ": ");
        splitter.next()
            .map(|key| splitter.next()
            .map(|value| map.insert(key.to_string(), value.to_string())));
    }

    map
}

fn start_search() -> UdpSocket {
    let socket = UdpSocket::bind("0.0.0.0:34254").unwrap();
    let message = b"M-SEARCH * HTTP/1.1\r\n
                HOST: 239.255.255.250:1982\r\n
                MAN: \"ssdp:discover\"\r\n
                ST: wifi_bulb";
    socket.send_to(message, "239.255.255.250:1982").ok();
    socket
}

pub enum SearchUntil<'a> {
    Duration(time::Duration),
    AtLeastN(usize),
    SpecificBulb(&'a str),
    SpecificBulbs(&'a HashSet<&'a str>)
}

fn listen(socket: UdpSocket, until: SearchUntil) -> Vec<Bulb> {
    let receiver = spawn_searcher_thread(socket);
    loop {
        let sleep_duration = if let SearchUntil::Duration(d) = until { d } else { time::Duration::from_millis(500) };
        thread::sleep(sleep_duration);
        let bulbs = read_bulbs(&receiver);
        match until {
            SearchUntil::Duration(_) => return bulbs,
            SearchUntil::AtLeastN(n) => if bulbs.len() == n { return bulbs; },
            SearchUntil::SpecificBulb(id) => if !bulbs.iter().find(|b| b.id == id).is_some() { return bulbs; },
            SearchUntil::SpecificBulbs(set) => {
                let found_id_set = bulbs.iter().map(|b| &b.id[..]).collect();
                if set.is_subset(&found_id_set) { return bulbs; }
            }
        }
    }
}

fn spawn_searcher_thread(socket: UdpSocket) -> Receiver<BulbMap> {
    let (sender, receiver): (Sender<BulbMap>, Receiver<BulbMap>) = channel();
    thread::spawn(move || {
        let mut buf = [0; 2048];
        loop {
            socket.recv_from(&mut buf).map(|_| str::from_utf8(&buf).map(|s| sender.send(to_map(s)))).ok();
            thread::sleep(time::Duration::from_millis(200));
        }
    });
    receiver
}

fn read_bulbs(receiver: &Receiver<BulbMap>) -> Vec<Bulb> {
    let mut id_set: HashSet<String> = HashSet::new();
    receiver.try_iter().flat_map(|map|
        map.get("id").and_then(|id| {
            if id_set.contains(id) { return None; }
            id_set.insert(id.to_string());
            Bulb::new(&map)
        })
    ).map(|b| b.clone()).collect()
}

pub fn search_until(until: SearchUntil) -> Vec<Bulb> {
    listen(start_search(), until)
}