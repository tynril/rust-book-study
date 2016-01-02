extern crate rand;

use std::io::{BufRead, Write};
use std::cmp::Ordering;

fn compare(a:u32, b:u32) -> Ordering {
    if a < b { Ordering::Less }
    else if a > b { Ordering::Greater }
    else { Ordering::Equal }
}

fn print_and_flush(txt:&str) {
    print!("{}", txt);
    std::io::stdout().flush().unwrap();
}

fn main() {
    let args:Vec<_> = std::env::args().collect();
    let random_value = (rand::random::<u32>() % 100) + 1;

    let is_cheating = args.len() > 1 && args[1] == "-cheat";
    if is_cheating {
        println!("The random value is {}, cheater.", random_value);
    }

    print_and_flush("Enter your guess (between 1 and 100): ");

    let stdin = std::io::stdin();
    let mut guesses = 0;
    for line in stdin.lock().lines() {
        guesses += 1;

        let input = line.ok()
                        .expect("Unable to read what you've written.")
                        .trim()
                        .parse::<u32>();
        let num = match input {
            Ok(n)  => n,
            Err(_) => {
                println!("Yeah, that wasn't a number. Clearly, you don't want to play.");
                break;
            }
        };

        match compare(num, random_value) {
            Ordering::Less    => println!("Too small..."),
            Ordering::Greater => println!("Too big..."),
            Ordering::Equal   => {
                println!("You win! It only took you {} guess(es). ", guesses);
                break;
            },
        };

        print_and_flush("Try again: ");
    }
}
