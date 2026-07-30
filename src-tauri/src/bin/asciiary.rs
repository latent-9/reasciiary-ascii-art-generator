//! The command language without the window, for driving a render from a shell
//! or a test.

fn main() {
    let line = std::env::args().skip(1).collect::<Vec<_>>().join(" ");
    match asciiary_tauri_lib::run_line(&line) {
        Ok(message) => println!("{message}"),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
