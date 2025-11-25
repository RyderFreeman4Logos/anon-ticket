use std::env;
use std::process;

use anon_ticket_domain::integrated_address::build_integrated_address;
use anon_ticket_domain::model::PaymentId;

fn main() {
    let mut args = env::args().skip(1);
    let Some(primary_address) = args.next() else {
        eprintln!("Usage: gen_integrated_address <primary_address>");
        process::exit(1);
    };

    let payment_id = match PaymentId::generate() {
        Ok(pid) => pid,
        Err(err) => {
            eprintln!("failed to generate payment id: {err}");
            process::exit(1);
        }
    };

    let integrated = match build_integrated_address(&primary_address, &payment_id) {
        Ok(address) => address,
        Err(err) => {
            eprintln!("failed to build integrated address: {err}");
            process::exit(1);
        }
    };

    println!("Payment ID: {}", payment_id);
    println!("Integrated address: {integrated}");
}
