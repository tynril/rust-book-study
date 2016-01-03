extern crate hyper;
extern crate rand;

use hyper::client::{Client};
use hyper::header::ContentLength;
use hyper::server::{Server, Handler, Request, Response};
use hyper::uri::RequestUri;
use hyper::net::Openssl;
use std::io::Read;
use std::sync::mpsc::{channel, Sender};
use std::sync::Mutex;
use std::thread;

/// This handler has a channel to communicate a request to close the listening server.
struct CloseServerHandler {
    close_chan: Mutex<Sender<()>>,
    text: String,
}

impl CloseServerHandler {
    /// Sends something on the channel to request closing the listening server.
    fn close(&self) {
        self.close_chan.lock().unwrap().send(()).unwrap();
    }
}

impl Handler for CloseServerHandler {
    fn handle(&self, req: Request, mut res: Response) {
        let path = match req.uri {
            RequestUri::AbsolutePath(ref path) => {
                path
            },
            _ => panic!("Unsupported URI format."),
        };

        match path.as_ref() {
            "/" => {
                res.send(self.text.as_bytes()).unwrap();
            },
            "/close" => {
                match req.method {
                    hyper::Post => {
                        self.close();
                    },
                    _ => {
                        *res.status_mut() = hyper::BadRequest;
                        (*res.headers_mut()).set(ContentLength(0));
                    }
                }
            },
            _ => {
                *res.status_mut() = hyper::NotFound;
                (*res.headers_mut()).set(ContentLength(0));
            }
        }
    }
}

/// Spawns a named thread.
fn spawn_thread<F, T>(name: &str, f: F) -> thread::JoinHandle<T>
where F: FnOnce() -> T, F: Send + 'static, T: Send + 'static {
    thread::Builder::new().name(name.to_string()).spawn(f).unwrap()
}

fn main() {
    let port = 1234;
    let ssl = Openssl::with_cert_and_key("assets/server.crt", "assets/server.key").unwrap();
    let txt = "Boop beep boop~";

    // A thread for the server, ...
    let srv = spawn_thread("server", move || {
        let (tx, rx) = channel();

        let mut server = Server::https(("0.0.0.0", port), ssl).unwrap();
        server.keep_alive(Some(std::time::Duration::new(30, 0)));
        let mut guard = server.handle(CloseServerHandler {
            close_chan: Mutex::new(tx),
            text: txt.to_string(),
        }).unwrap();

        // This will block until the handler writes to the closing channel.
        let _close_msg = rx.recv().unwrap();
        guard.close().unwrap();
    });

    // ...and one for the client.
    let cli = spawn_thread("client", move || {
        // Just a small helper to get a URL to our local server.
        let url = |path: &str| {
            format!("https://localhost:{}{}", port, path)
        };

        let client = Client::new();

        // Normal request
        {
            let mut res = client.get(&url("/")).send().unwrap();
            assert_eq!(res.status, hyper::Ok);

            let mut content = String::new();
            res.read_to_string(&mut content).unwrap();

            assert_eq!(content, txt);
        }

        // Request on a URL that won't be found
        {
            let res = client.get(&url("/error")).send().unwrap();
            assert_eq!(res.status, hyper::NotFound);
        }

        // Invalid closing method
        {
            let res = client.get(&url("/close")).send().unwrap();
            assert_eq!(res.status, hyper::BadRequest);
        }

        // Request on the special URL to close the server (note the POST)
        {
            let res = client.post(&url("/close")).send().unwrap();
            assert_eq!(res.status, hyper::Ok);
        }
    });

    // And now we wait!
    let _srv_wait = srv.join();
    let _cli_wait = cli.join();
}
