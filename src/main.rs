use std::sync::{Mutex, MutexGuard, Arc};
use std::thread;

struct Philosopher {
	name: String,
	left_handed: bool,
}

impl Philosopher {
	fn new(name: &str, left_handed: bool) -> Philosopher {
		Philosopher {
			name: name.to_string(),
			left_handed: left_handed,
		}
	}

	fn eat(&self, table: &Table) {
		let seat = table.sit();

		let mut seat_fork_index = seat.index;
		let mut neighbor_fork_index = seat.neighbor_index();

		if self.left_handed {
			let tmp = seat_fork_index;
			seat_fork_index = neighbor_fork_index;
			neighbor_fork_index = tmp;
		}

		let _fork_a = table.take_fork(seat_fork_index);
		thread::sleep_ms(150);
		let _fork_b = table.take_fork(neighbor_fork_index);

		println!("{} is eating.", self.name);
		thread::sleep_ms(1000);
		println!("{} is done eating.", self.name);
	}
}

struct Seat<'a> {
	index: usize,
	table: &'a Table,
}

impl<'a> Drop for Seat<'a> {
	fn drop(&mut self) {
		let mut seats = self.table.seats.lock().unwrap();
		seats[self.index] = false;
	}
}

impl<'a> Seat<'a> {
	fn neighbor_index(&self) -> usize {
		self.index.wrapping_add(1) % self.table.forks.len()
	}
}

struct Table {
	seats: Mutex<Vec<bool>>,
	forks: Vec<Mutex<()>>,
}

impl Table {
	fn new(seats_count: usize) -> Table {
		let mut forks = vec![];
		for _ in 0..seats_count {
			forks.push(Mutex::new(()));
		}
		Table {
			seats: Mutex::new(vec![false; seats_count]),
			forks: forks,
		}
	}

	fn sit(&self) -> Seat {
		let mut seats = self.seats.lock().unwrap();
		let found_seat :usize;
		loop {
			let seat = seats.iter().position(|&s| s == false);
			match seat {
				Some(index) => {
					found_seat = index;
					break;
				}
				None => {
					continue;
				}
			}
		}
		seats[found_seat] = true;
		Seat {
			index: found_seat,
			table: self,
		}
	}

	fn take_fork(&self, index: usize) -> MutexGuard<()> {
		self.forks[index].lock().unwrap()
	}
}

fn main() {
	let philosophers = vec![
		Philosopher::new("Judith Butler", true),
		Philosopher::new("Gilles Deleuze", false),
		Philosopher::new("Karl Marx", false),
		Philosopher::new("Emma Goldman", false),
		Philosopher::new("Michel Foucault", false),
	];
	let table = Arc::new(Table::new(philosophers.len()));

	let handles: Vec<_> = philosophers.into_iter().map(|philosopher| {
		let table = table.clone();
		thread::spawn(move || {
			philosopher.eat(&table);
		})
	}).collect();

	for h in handles {
		h.join().unwrap();
	}
}
