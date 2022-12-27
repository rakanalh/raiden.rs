use ethsign::SecretKey;
use std::collections::HashMap;
use std::io::{stdin, stdout, Write};
use web3::types::Address;

use crate::accounts;

pub fn prompt_key(keys: &HashMap<String, Address>) -> String {
    println!("Select key:");
    loop {
        let mut index = 0;
        let mut s = String::new();

        for address in keys.values() {
            println!("[{}]: {}", index, address);
            index += 1;
        }
        print!("Selected key: ");
        let _ = stdout().flush();
        stdin().read_line(&mut s).expect("Did not enter a correct string");
        let selected_value: Result<u32, _> = s.trim().parse();
        if let Ok(chosen_index) = selected_value {
            if (chosen_index as usize) >= keys.len() {
                continue;
            }
            return keys.keys().nth(chosen_index as usize).unwrap().clone();
        }
    }
}

pub fn prompt_password(key_filename: String) -> SecretKey {
    loop {
        let pass = rpassword::read_password_from_tty(Some("Password: ")).unwrap();
        let unlock = accounts::use_key(&key_filename, pass.to_string());
        if let Some(secret_key) = unlock {
            return secret_key;
        }
    }
}
