use std::error::Error;
use std::fs;
use std::env;
use std::net::{ToSocketAddrs, SocketAddr, TcpStream};
use std::thread;
use std::sync::{Arc, Mutex};
use std::time::{Instant, Duration};
use std::thread::{JoinHandle, Thread};
use std::collections::HashMap;
use std::panic::resume_unwind;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::SeqCst;

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_config() -> Result<Config, Box<dyn Error>> {
        let arg_list = [String::from("rust.exe"), String::from("1"), String::from("50")];
        Ok(Config::new(&arg_list)?)
    }

    #[test]
    fn try_get_alexa_rankings() {
        let mut domains = setup_config().unwrap_or_else(|_| panic!("Error setting up config"));
        let domains = domains.get_domains().unwrap();
        assert!(domains.iter().any(|i| i.lock().unwrap().get_url().contains(&String::from("yahoo.com"))));
        assert_eq!(domains.len(), 50);
    }

    #[test]
    fn try_resolve() {
        let mut domain = setup_config().unwrap_or_else(|_| panic!("Error setting up config"));
        domain.resolve_domains();
        let domain = &mut domain.get_domains().unwrap()[0];
        assert!(domain.lock().unwrap().get_ipv4().unwrap().len() > 0);
    }

    #[test]
    fn take_times() {
        let mut domain = setup_config().unwrap_or_else(|_| panic!("Error setting up config"));
        domain.resolve_domains();
        domain.take_time();
        assert_eq!(1, 1);
    }

    #[test]
    fn try_race() {
        let mut domains = setup_config().unwrap_or_else(|_| panic!("Error setting up config"));
        domains.resolve_domains();
        domains.race_domains();
        let domains = domains.get_domains().unwrap();
        assert!(domains.iter().any(|i| i.lock().unwrap().get_stream().is_ok()));
    }

    #[test]
    fn try_csv() {

    }

}

pub struct Config {
    _attempts: usize,
    domains: Vec<Arc<Mutex<Domain>>>
}

impl Config {
    pub fn new(args: &[String]) -> Result<Config, Box<dyn Error>> {
        if args.len() < 3 {
            return Err("not enough arguments".into());
        }
        let _attempts = args[1].parse().unwrap();
        let sites =  args[2].parse().unwrap();

        let websites = env::var("ALEXA_FILE").unwrap_or_else(|_| String::from("alexa.txt"));
        let websites = fs::read_to_string(websites)?;
        let mut domains = Vec::new();
        // Take the first self.sites lines of the websites, split away the rank and collect them to a vec

        for result in websites.lines().take(sites).map(|page| String::from(
            page.split(",").collect::<Vec<&str>>()[1])){
            domains.push(Arc::new(Mutex::new(Domain::new(result))));
        }
        Ok(Config { _attempts, domains})
    }

    pub fn get_domains(&mut self) -> Result<&mut Vec<Arc<Mutex<Domain>>>, Box<dyn Error>> {
        Ok(&mut self.domains)
    }

    pub fn resolve_domains(&self) {
        let time = Instant::now();
        let mut handles = Vec::new();
        for domain in &self.domains {
            let domain = domain.clone();
            handles.push(thread::spawn(move || {
                domain.lock().unwrap().resolve();
            }));
        }
        for handle in handles {
            handle.join().unwrap();
        }

        let time = Instant::now() - time;
        println!("Time: {}", time.as_millis());
    }

    pub fn take_time(&self) {
        let mut handles = Vec::new();
        for domain in &self.domains {
            let domain = domain.clone();
            handles.push(thread::spawn(move || {
                domain.lock().unwrap().time_v4();
                domain.lock().unwrap().time_v6();
            }));
        }
        for handle in handles {
            handle.join().unwrap();
        }
    }

    pub fn race_domains(&self) {
        let mut handles = Vec::new();
        for domain in &self.domains {
            let domain = domain.clone();
            handles.push(thread::spawn(move || {
                domain.lock().unwrap().race();
            }));
        }
        for handle in handles {
            handle.join().unwrap();
        }
    }

    pub fn to_csv(&self, ) {

    }
}

pub struct Domain {
    url: String,
    v4: Vec<SocketAddr>,
    v6: Vec<SocketAddr>,
    connected: Arc<AtomicBool>,
    stream: Arc<Mutex<Option<TcpStream>>>
}

impl Domain {
    pub fn new(url: String) -> Domain {
        Domain {url, v4: Vec::new(), v6: Vec::new(), connected: Arc::new(AtomicBool::new(false)), stream: Arc::new(Mutex::new(None)) }
    }

    pub fn get_url(&self) -> &String {
        &self.url
    }

    pub fn get_ipv4(&self) -> Result<&Vec<SocketAddr>, Box<dyn Error>> {
        Ok(&self.v4)
    }

    pub fn get_ipv6(&self) -> Result<&Vec<SocketAddr>, Box<dyn Error>> {
        Ok(&self.v6)
    }

    pub fn get_stream(&self) -> Result<&Arc<Mutex<Option<TcpStream>>>, String> {
        if !self.connected.load(SeqCst) {
            return Err("Not connected yet".parse().unwrap());
        } else {
            Ok(&self.stream)
        }
    }

    pub fn resolve(&mut self) {
        //println!("Now resolving {:?}", self.url);
        for addr in format!("{}{}",self.url,":80").to_socket_addrs().unwrap_or_else(|_|{Vec::new().into_iter()}) {
            match addr {
                SocketAddr::V4(..) => self.v4.push(addr),
                SocketAddr::V6(..) => self.v6.push(addr)
            }
        } ;

    }
    // Returns average v4 time
    pub fn time_v4(&self) -> std::io::Result<Duration> {
        let mut duration = Duration::new(0,0);
        for addr in &self.v4 {
            duration += Domain::take_time(addr).unwrap();
        }
        let duration = duration;
        //println!("Average time v4 for domain {:?}: {:?}",self.url ,duration);
        Ok(duration)
    }

    // Returns average v6 time
    pub fn time_v6(&self) -> std::io::Result<Duration> {
        let mut duration = Duration::new(0,0);
        for addr in &self.v6 {
            duration += Domain::take_time(addr).unwrap();
        }
        let duration = duration;
        //println!("Average time v6 for domain {:?}: {:?}",self.url ,duration);
        Ok(duration)
    }

    fn take_time(addr: &SocketAddr) -> std::io::Result<Duration> {
        let time = Instant::now();
        TcpStream::connect(addr)?;
        Ok(Instant::now() - time)
    }

    fn race(&self) {
        let mut handles = Vec::new();
        for addr in &self.v4 {
            let addr = addr.clone();
            let connected = self.connected.clone();
            let stream = self.stream.clone();
            handles.push(thread::spawn(move || {
                let new_stream = TcpStream::connect(addr).unwrap();
                if !connected.load(SeqCst) {
                    connected.store(true,SeqCst);
                    *stream.lock().unwrap() = Some(new_stream);
                }
            }));

            if self.connected.load(SeqCst) {
                break;
            }

        }
        for handle in handles {
            handle.join().unwrap();
        }
    }

}