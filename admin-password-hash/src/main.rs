fn main() {
    let password = rpassword::prompt_password("Enter password to be hashed: ")
        .expect("Unable to read password");
    let password_hash = bcrypt::hash(&password, 10).expect("Unable to hash password");
    println!("Password Hash:\n{}", password_hash);
}
