use crate::bulb::Bulb;
use std::io::Read;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4, ToSocketAddrs, UdpSocket};
use std::time::Duration;
use std::{io, str, thread, time};

const MULTICAST_ADDR: &str = "239.255.255.250:1982";

pub enum BulbSearcher {
    UntilDuration(Duration),
    UntilBulbCount(usize),
}

trait SendRecvable {
    fn send_to<A: ToSocketAddrs>(&self, buf: &[u8], addr: A) -> io::Result<usize>;
    fn recv_from(&self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)>;
    fn set_read_timeout(&mut self, dur: Option<Duration>) -> io::Result<()>;
}

impl SendRecvable for UdpSocket {
    fn send_to<A: ToSocketAddrs>(&self, buf: &[u8], addr: A) -> io::Result<usize> {
        self.send_to(buf, addr)
    }

    fn recv_from(&self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        self.recv_from(buf)
    }

    fn set_read_timeout(&mut self, dur: Option<Duration>) -> io::Result<()> {
        self.set_read_timeout(dur)
    }
}

impl BulbSearcher {
    pub fn search(&self) -> Option<Vec<Bulb>> {
        UdpSocket::bind("0.0.0.0:34254")
            .ok()
            .map(|s| self.search_with_socket(s))
            .flatten()
    }

    fn search_with_socket<T: SendRecvable>(&self, mut socket: T) -> Option<Vec<Bulb>> {
        let message = b"M-SEARCH * HTTP/1.1\r\n
                    HOST: 239.255.255.250:1982\r\n
                    MAN: \"ssdp:discover\"\r\n
                    ST: wifi_bulb";

        let send_result = socket.send_to(message, MULTICAST_ADDR);
        if send_result.is_err() {
            return None;
        }

        if let BulbSearcher::UntilDuration(d) = self {
            socket.set_read_timeout(Some(*d));
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

struct MockSendRecvable<'a> {
    send_to_result: usize,
    recv_contents: &'a [u8],
    recv_delay: Option<Duration>,
    recv_timeout: Option<Duration>,
}

impl<'a> SendRecvable for MockSendRecvable<'a> {
    fn send_to<A: ToSocketAddrs>(&self, buf: &[u8], addr: A) -> io::Result<usize> {
        io::Result::Ok(self.send_to_result)
    }

    fn recv_from(&self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        if let Some(delay) = self.recv_delay {
            if let Some(timeout) = self.recv_timeout {
                if delay > timeout {
                    thread::sleep(timeout);
                    return io::Result::Ok((
                        0,
                        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080),
                    ));
                }
                thread::sleep(delay);
            } else {
                thread::sleep(delay);
            }
        }

        for (i, elem) in buf.iter_mut().enumerate() {
            if i >= self.recv_contents.len() {
                break;
            };
            *elem = self.recv_contents[i];
        }

        io::Result::Ok((
            buf.len(),
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080),
        ))
    }

    fn set_read_timeout(&mut self, dur: Option<Duration>) -> io::Result<()> {
        self.recv_timeout = dur;
        io::Result::Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{
        ops::{Div, Mul},
        time::Duration,
    };

    use crate::search::MockSendRecvable;

    use super::BulbSearcher;

    const recv_contents: &str = concat!(
        "HTTP/1.1 200 OK\r\n",
        "Cache-Control: max-age=3600\r\n",
        "Date: \r\n",
        "Ext: \r\n",
        "Location: yeelight://192.168.1.239:55443\r\n",
        "Server: POSIX UPnP/1.0 YGLC/1\r\n",
        "id: 0x000000000015243f\r\n",
        "model: color\r\n",
        "fw_ver: 18\r\n",
        "support: get_prop set_default set_power toggle\r\n",
        "power: on\r\n",
        "bright: 100\r\n",
        "color_mode: 2\r\n",
        "ct: 4000\r\n",
        "rgb: 16711680\r\n",
        "hue: 100\r\n",
        "sat: 35\r\n",
        "name: my_bulb\r\n",
    );

    #[test]
    fn bulb_search_count_one_test() {
        let mock = MockSendRecvable {
            send_to_result: 0,
            recv_contents: recv_contents.as_bytes(),
            recv_delay: None,
            recv_timeout: None,
        };

        let mut bulbs = BulbSearcher::UntilBulbCount(1)
            .search_with_socket(mock)
            .unwrap();

        let bulb = bulbs.remove(0);

        assert_eq!(bulb.id, "0x000000000015243f".to_string())
    }

    #[test]
    fn bulb_search_duration_stop_test() {
        let return_duration = Duration::from_millis(10);
        let mock = MockSendRecvable {
            send_to_result: 0,
            recv_contents: recv_contents.as_bytes(),
            recv_delay: Some(return_duration),
            recv_timeout: None,
        };

        let search_result = BulbSearcher::UntilDuration(return_duration.div(2))
            .search_with_socket(mock)
            .unwrap_or(Vec::new());

        assert!(search_result.is_empty());
    }

    #[test]
    fn bulb_search_duration_find_test() {
        let return_duration = Duration::from_millis(10);
        let mock = MockSendRecvable {
            send_to_result: 0,
            recv_contents: recv_contents.as_bytes(),
            recv_delay: Some(return_duration),
            recv_timeout: None,
        };

        let mut bulbs = BulbSearcher::UntilDuration(return_duration.mul(2))
            .search_with_socket(mock)
            .unwrap();

        let bulb = bulbs.remove(0);

        assert_eq!(bulb.id, "0x000000000015243f".to_string())
    }
}
