extern crate bytes;
extern crate mio;

use bytes::{Buf, Take};
use mio::{TryRead, TryWrite};
use mio::tcp::{TcpListener, TcpStream};
use mio::util::Slab;
use std::io::Cursor;
use std::mem;

/// Reserved token for the listener used by the server.
const SERVER: mio::Token = mio::Token(0);

/// Each connection can be in one of three states:
///  - Reading stuff that the client is sending,
///  - Writing back to the client,
///  - Closed, when the client is gone.
enum State {
    Reading(Vec<u8>),
    Writing(Take<Cursor<Vec<u8>>>),
    Closed,
}

impl State {
    /// Returns the mutable reading buffer if in the Reading state, panic otherwise.
    fn mut_read_buf(&mut self) -> &mut Vec<u8> {
        match *self {
            State::Reading(ref mut buf) => buf,
            _ => panic!("Connection not readable"),
        }
    }

    /// Returns the reading buffer if in the Reading state, panic otherwise.
    fn read_buf(&self) -> &Vec<u8> {
        match *self {
            State::Reading(ref buf) => buf,
            _ => panic!("Connection not readable"),
        }
    }

    /// Consumes the reading buffer if in the Reading state, panic otherwise.
    fn unwrap_read_buf(self) -> Vec<u8> {
        match self {
            State::Reading(buf) => buf,
            _ => panic!("Connection not readable"),
        }
    }

    /// Looks for a new line. If found, transitions to the Writing state.
    fn try_transition_to_writing(&mut self) {
        if let Some(pos) = self.read_buf().iter().position(|b| *b == b'\n') {
            self.transition_to_writing(pos + 1);
        }
    }

    /// Moves from Reading to Writing.
    fn transition_to_writing(&mut self, pos: usize) {
        let buf = mem::replace(self, State::Closed).unwrap_read_buf();
        let buf = Cursor::new(buf);
        *self = State::Writing(Take::new(buf, pos));
    }

    /// Returns the mutable writing buffer if in the Writing state, panic otherwise.
    fn mut_write_buf(&mut self) -> &mut Take<Cursor<Vec<u8>>> {
        match *self {
            State::Writing(ref mut buf) => buf,
            _ => panic!("Connection not writeable"),
        }
    }

    /// Returns the writing buffer if in the Writing state, panic otherwise.
    fn write_buf(&self) -> &Take<Cursor<Vec<u8>>> {
        match *self {
            State::Writing(ref buf) => buf,
            _ => panic!("Connection not writeable"),
        }
    }

    /// Consumes the writing buffer if in the Writing state, panic otherwise.
    fn unwrap_write_buf(self) -> Take<Cursor<Vec<u8>>> {
        match self {
            State::Writing(buf) => buf,
            _ => panic!("Connection not writeable"),
        }
    }

    /// If there's nothing left in the write buffer, transition to the Reading state. Might
    /// directly transitions back to Writing if something was already in the reading buffer.
    fn try_transition_to_reading(&mut self) {
        if !self.write_buf().has_remaining() {
            let cursor = mem::replace(self, State::Closed).unwrap_write_buf().into_inner();
            let pos = cursor.position();
            let mut buf = cursor.into_inner();

            for _ in 0..pos { buf.remove(0); }
            *self = State::Reading(buf);

            self.try_transition_to_writing();
        }
    }

    /// Checks whether the state is closed or not.
    fn is_closed(&self) -> bool {
        match *self {
            State::Closed => true,
            _ => false,
        }
    }
}

/// Represents a client connection on this server.
struct Connection {
    socket: TcpStream,
    token: mio::Token,
    state: State,
}

impl Connection {
    /// Builds a new connection from a stream and a token, in the Reading state.
    fn new(socket: TcpStream, token: mio::Token) -> Connection {
        Connection {
            socket: socket,
            token: token,
            state: State::Reading(vec![]),
        }
    }

    /// Called by the server whenever events are ready for this connection.
    fn ready(&mut self, event_loop: &mut mio::EventLoop<Pong>, events: mio::EventSet) {
        match self.state {
            State::Reading(_) => {
                assert!(events.is_readable());
                self.read(event_loop);
            },
            State::Writing(_) => {
                assert!(events.is_writable());
                self.write(event_loop);
            }
            _ => panic!("Unexpected state."),
        }
    }

    /// Try to read the data on the socket in the Reading state buffer, and transition to writing
    /// as needed.
    fn read(&mut self, event_loop: &mut mio::EventLoop<Pong>) {
        match self.socket.try_read_buf(self.state.mut_read_buf()) {
            Ok(Some(0)) => {
                // If we've read anything yet, let's write it back before closing down.
                match self.state.read_buf().len() {
                    n if n > 0 => {
                        self.state.transition_to_writing(n);
                        self.reregister(event_loop);
                    }
                    _ => self.state = State::Closed,
                }
            },
            Ok(Some(_)) => {
                self.state.try_transition_to_writing();
                self.reregister(event_loop);
            },
            Ok(None) => {
                self.reregister(event_loop);
            },
            Err(e) => {
                panic!("Connection reading error: {:?}", e);
            }
        }
    }

    /// Try to write the data in our Writing state buffer to the socket, and transition as needed.
    fn write(&mut self, event_loop: &mut mio::EventLoop<Pong>) {
        match self.socket.try_write_buf(self.state.mut_write_buf()) {
            Ok(Some(_)) => {
                self.state.try_transition_to_reading();
                self.reregister(event_loop);
            },
            Ok(None) => {
                self.reregister(event_loop);
            },
            Err(e) => {
                panic!("Connection writing error: {:?}", e);
            }
        }
    }

    /// Initially register that connection with the event loop.
    fn register(&self, event_loop: &mut mio::EventLoop<Pong>) {
        let poll_opt = mio::PollOpt::edge() | mio::PollOpt::oneshot();
        event_loop.register(&self.socket, self.token, self.get_event_set(), poll_opt).unwrap();
    }

    /// Re-register the event we care about depending on the current state of the connection.
    fn reregister(&self, event_loop: &mut mio::EventLoop<Pong>) {
        let poll_opt = mio::PollOpt::edge() | mio::PollOpt::oneshot();
        event_loop.reregister(&self.socket, self.token, self.get_event_set(), poll_opt).unwrap();
    }

    /// Get the type of events this connection should care about, depending on its current state.
    fn get_event_set(&self) -> mio::EventSet {
        match self.state {
            State::Reading(_) => mio::EventSet::readable(),
            State::Writing(_) => mio::EventSet::writable(),
            _ => mio::EventSet::none(),
        }
    }

    /// Check whether this connection is closed by checking the underlying state.
    fn is_closed(&self) -> bool {
        self.state.is_closed()
    }
}

/// This is our ping-pong server. A listener, and a bunch of connections.
struct Pong {
    listener: TcpListener,
    connections: Slab<Connection>,
}

impl Pong {
    /// Builds a new server from a listener. The connections slab is initialized too.
    fn new(listener: TcpListener) -> Pong {
        let slab = Slab::new_starting_at(mio::Token(1), 1024);

        Pong {
            listener: listener,
            connections: slab,
        }
    }
}

impl mio::Handler for Pong {
    type Timeout = ();
    type Message = ();

    /// Called by the event loop whenever an event we care about is happening.
    fn ready(
        &mut self,
        event_loop: &mut mio::EventLoop<Self>,
        token: mio::Token,
        events: mio::EventSet
    ) {
        match token {
            SERVER => {
                match self.listener.accept() {
                    Ok(Some(socket_addr)) => {
                        let (socket, _) = socket_addr;

                        // Make a new connection object and put it in our connections slab.
                        let token = self.connections
                                        .insert_with(|token| Connection::new(socket, token))
                                        .unwrap();
                        println!("Accepted a socket, token {}.", token.as_usize());

                        // Make sure the event loop now cares about the events happening to this
                        // connection.
                        self.connections[token].register(event_loop);
                    },
                    Ok(None) => {
                        println!("Socket wasn't ready yet");
                    },
                    Err(e) => {
                        println!("listener.accept() error: {:?}", e);
                        event_loop.shutdown();
                    },
                }
            },
            _ => {
                // Forward what happened to the connection object.
                self.connections[token].ready(event_loop, events);

                // Check if the connection is now closed, in which case we can forget about it.
                if self.connections[token].is_closed() {
                    self.connections.remove(token);
                    println!(
                        "Removing the socket with token {} ({} left).",
                        token.as_usize(),
                        self.connections.count()
                    );
                }
            },
        }
    }
}

fn main() {
    // Start listening
    let address = "0.0.0.0:6567".parse().unwrap();
    let listener = TcpListener::bind(&address).unwrap();

    // Create an event queue, register the listener
    let mut event_loop = mio::EventLoop::new().unwrap();
    event_loop.register(
        &listener,
        SERVER,
        mio::EventSet::readable(),
        mio::PollOpt::edge()
    ).unwrap();

    // Run!
    println!("Running pingpong server...");
    event_loop.run(&mut Pong::new(listener)).unwrap();
}
