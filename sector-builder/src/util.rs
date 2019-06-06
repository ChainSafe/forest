use rand::{thread_rng, Rng};

// creates a string of size len containing uppercase alpha-chars
pub fn rand_alpha_string(len: u8) -> String {
    let mut str = String::new();
    let mut rng = thread_rng();

    for _ in 0..len {
        let ch = rng.gen_range(b'A', b'Z') as char;
        str.push(ch);
    }

    str
}
