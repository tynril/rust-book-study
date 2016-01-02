extern crate hyper;
extern crate rand;

use std::io::Read;
use std::thread;
use std::sync::mpsc::{channel, Sender};
use std::sync::Mutex;
use hyper::server::{Server, Handler, Request, Response};
use hyper::client::{Client};
use hyper::net::Openssl;

/// This handler sends something on a closing channel whenever a request is received.
struct CloseServerHandler {
    close_chan: Mutex<Sender<()>>,
}

impl Handler for CloseServerHandler {
    fn handle(&self, _req: Request, res: Response) {
        res.send(b"Boop beep boop~").unwrap();
        self.close_chan.lock().unwrap().send(()).unwrap();
    }
}

fn main() {
    let port = 8080;
    let ssl = Openssl::with_cert_and_key("assets/server.crt", "assets/server.key").unwrap();

    // A thread for the server, ...
    let srv = thread::spawn(move || {
        let (tx, rx) = channel();

        let server = Server::https(("0.0.0.0", port), ssl).unwrap();
        let mut guard = server.handle(CloseServerHandler {
            close_chan: Mutex::new(tx),
        }).unwrap();

        // This will block until the handler writes to the closing channel.
        let _close_msg = rx.recv().unwrap();
        guard.close().unwrap();

        println!("Server finished.");
    });

    // ...and one for the client.
    let cli = thread::spawn(move || {
        let client = Client::new();
        let mut res = client.get(&format!("https://localhost:{}/", port)).send().unwrap();
        assert_eq!(res.status, hyper::Ok);

        let mut content = String::new();
        res.read_to_string(&mut content).unwrap();

        println!("Client finished: {:?}", content);
    });

    // And now we wait!
    let _srv_wait = srv.join();
    let _cli_wait = cli.join();
}
