use std::{env, process};
use std::time::{UNIX_EPOCH, SystemTime};
use happy_eyeballs::Config;

fn main() {
    let filename = format!("results_{:?}.csv", SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis());
    let domains = Config::new(env::args().collect()).unwrap_or_else(|err| {
        eprintln!("Problem parsing arguments: {}", err);
        process::exit(1);
    });
    let mut time = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis();
    domains.resolve_domains();
    domains.take_time();
    domains.save_results(&*filename).unwrap_or_else(|_| panic!("Error saving results."));
    time = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() - time;
    println!("Execution done, time taken: {:?}", time)
}
