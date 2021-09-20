use crate::bulb::Bulb;
use std::net::UdpSocket;
use std::time::Duration;
use std::{str, time};

const MULTICAST_ADDR: &str = "239.255.255.250:1982";

pub enum BulbSearcher {
    UntilDuration(Duration),
    UntilBulbCount(usize),
}

impl BulbSearcher {
    pub fn search(&self) -> Option<Vec<Bulb>> {
        let socket = UdpSocket::bind("0.0.0.0:34254");
        if socket.is_err() {
            return None;
        }
        let socket = socket.unwrap();
        let message = b"M-SEARCH * HTTP/1.1\r\n
                    HOST: 239.255.255.250:1982\r\n
                    MAN: \"ssdp:discover\"\r\n
                    ST: wifi_bulb";

        let send_result = socket.send_to(message, MULTICAST_ADDR);
        if send_result.is_err() {
            return None;
        }

        let start = time::Instant::now();

        let mut buf = [0; 2048];
        let mut found_bulbs: Vec<Bulb> = Vec::new();
        loop {
            socket.recv_from(&mut buf).ok().map(|_| {
                str::from_utf8(&buf)
                    .ok()
                    .map(|s| Bulb::parse(s))
                    .flatten()
                    .map(|bulb| {
                        if !found_bulbs.iter().any(|b| b.id == bulb.id) {
                            found_bulbs.push(bulb)
                        }
                    });
            });

            match self {
                BulbSearcher::UntilDuration(duration_limit) => {
                    let duration = start.elapsed();
                    if duration > *duration_limit {
                        return Some(found_bulbs);
                    }
                }
                BulbSearcher::UntilBulbCount(count) => {
                    if found_bulbs.len() == *count {
                        return Some(found_bulbs);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::connection::{BulbConnection, SetCtMode};

    use super::BulbSearcher;

    #[test]
    fn bulb_search_test() {
        let mut bulbs = BulbSearcher::UntilBulbCount(1).search().unwrap();

        let bulb = bulbs.remove(0);

        let mut conn = BulbConnection::new(bulb).unwrap();

        let res = conn.get_prop(&["power", "not_exist", "bright"]).unwrap();
        let res = conn.set_ct_abx(6500, SetCtMode::Sudden);

        println!("{:?}", res)
    }
}
