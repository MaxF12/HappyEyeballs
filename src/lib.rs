use std::error::Error;
use std::fs;
use std::env;
use std::net::{ToSocketAddrs, SocketAddr, TcpStream};
use std::thread;
use std::sync::{Arc, Mutex};
use std::time::{Instant, Duration};
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::SeqCst;
use std::fs::{File, remove_file};
use std::io::Write;

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_config() -> Result<Config, Box<dyn Error>> {
        let arg_list = vec![String::from("rust.exe"), String::from("1"), String::from("10")];
        Ok(Config::new(arg_list)?)
    }

    #[test]
    fn try_get_alexa_rankings() {
        let mut domains = setup_config().unwrap_or_else(|_| panic!("Error setting up config"));
        let domains = domains.get_domains().unwrap();
        assert!(domains.iter().any(|i| i.lock().unwrap().get_url().contains(&String::from("google.com"))));
        assert_eq!(domains.len(), 10);
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
        let domain = setup_config().unwrap_or_else(|_| panic!("Error setting up config"));
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
        let filename = "results.csv";
        let domains = setup_config().unwrap_or_else(|_| panic!("Error setting up config"));
        domains.resolve_domains();
        domains.take_time();
        domains.save_results(filename).unwrap_or_else(|_| panic!("Error setting up config"));
        let content = fs::read_to_string(filename)
            .expect("File not readable");
        assert_eq!(content.lines().count(), 10);
    }

}

pub struct Config {
    attempts: usize,
    domains: Vec<Arc<Mutex<Domain>>>
}

impl Config {
    pub fn new(args: Vec<String>) -> Result<Config, Box<dyn Error>> {
        if args.len() < 3 {
            return Err("not enough arguments".into());
        }
        let attempts = args[1].parse().unwrap();
        let sites =  args[2].parse().unwrap();

        let websites = env::var("ALEXA_FILE").unwrap_or_else(|_| String::from("alexa.csv"));
        let websites = fs::read_to_string(websites)?;
        let mut domains = Vec::new();
        // Take the first self.sites lines of the websites, split away the rank and collect them to a vec

        for result in websites.lines().take(sites).map(|page| String::from(
            page.split(",").collect::<Vec<&str>>()[1])){
            domains.push(Arc::new(Mutex::new(Domain::new(result))));
        }
        Ok(Config { attempts, domains})
    }

    pub fn get_domains(&mut self) -> Result<&mut Vec<Arc<Mutex<Domain>>>, Box<dyn Error>> {
        Ok(&mut self.domains)
    }

    pub fn resolve_domains(&self) {
        let time = Instant::now();
        let mut handles = Vec::new();
        let n = self.attempts;
        for domain in &self.domains {
            let domain = domain.clone();
            handles.push(thread::spawn(move || {
                for _ in 0..n {
                    domain.lock().unwrap().resolve();
                };
            }));
        }
        for handle in handles {
            handle.join().unwrap_or_else(|error| {
                println!("Could not join threads: {:?}", error);
            });
        }
        for domain in &self.domains {
            domain.lock().unwrap().v4.sort_unstable();
            domain.lock().unwrap().v4.dedup();
            domain.lock().unwrap().v6.sort_unstable();
            domain.lock().unwrap().v6.dedup();
        }

        let time = Instant::now() - time;
        println!("Time: {}", time.as_millis());
    }

    pub fn take_time(&self) {
        let mut handles = Vec::new();
        for domain in &self.domains {
            let domain = domain.clone();
            handles.push(thread::spawn(move || {
                domain.lock().unwrap().time_v4().unwrap_or_else(|error| {
                    println!("Could not take time: {:?}", error);
                    Duration::new(10000,10000)
                });
                domain.lock().unwrap().time_v6().unwrap_or_else(|error| {
                    println!("Could not take time: {:?}", error);
                    Duration::new(10000,10000)
                });
            println!("{:?}", domain.lock().unwrap().url);
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

    pub fn save_results(&self, filename: &str) -> std::io::Result<()> {
        println!("Saving CSV");
        let mut file = File::create(filename).unwrap_or_else(|error| {
            println!("File already exists, deleting: {:?}", error);
            remove_file(filename).unwrap_or_else(|err| {
                panic!("Deleting of file failed: {:?}", err);
            });
            File::create(filename).unwrap()
        });
        for d in &self.domains {
            let d = d.lock().unwrap();
            let line = format!("{:?};{:?};{:?};{:?};{:?};{:?};{:?}\n", d.url, d.v4, d.v4.len(), d.connect_time_v4.unwrap().as_nanos(), d.v6, d.v6.len(), d.connect_time_v6.unwrap().as_nanos());
            file.write_all(line.as_ref())?;
        }
        Ok(())
    }
}

pub struct Domain {
    url: String,
    v4: Vec<SocketAddr>,
    v6: Vec<SocketAddr>,
    connect_time_v4: Option<Duration>,
    connect_time_v6: Option<Duration>,
    connected: Arc<AtomicBool>,
    stream: Arc<Mutex<Option<TcpStream>>>
}

impl Domain {
    pub fn new(url: String) -> Domain {
        Domain {url, v4: Vec::new(), v6: Vec::new(), connect_time_v4: None, connect_time_v6: None, connected: Arc::new(AtomicBool::new(false)), stream: Arc::new(Mutex::new(None)) }
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
    pub fn time_v4(&mut self) -> std::io::Result<Duration> {
        let mut duration = Duration::new(0,0);
        let mut count = 0;
        for addr in &self.v4 {
            count += 1;
            duration += Domain::take_time(addr).unwrap_or_else(|_|{
                count -= 1;
                Duration::from_nanos(0)
            });
        }
        if count == 0 { duration = Duration::from_nanos(0);}
        else { duration = Duration::from_nanos((duration.as_nanos() / count) as u64); }
        //println!("Average time v4 for domain {:?}: {:?}",self.url ,duration);
        self.connect_time_v4 = Some(duration);
        Ok(duration)
    }

    // Returns average v6 time
    pub fn time_v6(&mut self) -> std::io::Result<Duration> {
        let mut duration = Duration::new(0,0);
        let mut count = 0;
        for addr in &self.v6 {
            count += 1;
            duration += Domain::take_time(addr).unwrap_or_else(|_|{
                count -= 1;
                Duration::from_nanos(0)
            });
        }
        if count == 0 { duration = Duration::from_nanos(0);}
        else { duration = Duration::from_nanos((duration.as_nanos() / count) as u64); }
        //println!("Average time v6 for domain {:?}: {:?}",self.url ,duration);
        self.connect_time_v6 = Some(duration);
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