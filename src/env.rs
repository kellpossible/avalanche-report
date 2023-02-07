use eyre::Context;

pub fn initialize() -> eyre::Result<()> {
    match dotenvy::dotenv() {
        Err(error) => match error {
            dotenvy::Error::Io(ref io) => match io.kind() {
                std::io::ErrorKind::NotFound => Ok(()),
                _ => Err(error),
            },
            error => Err(error),
        },
        Ok(path) => {
            println!("INFO: Environment variables loaded from {path:?}");
            Ok(())
        }
    }
    .wrap_err("Error loading .env")
}
