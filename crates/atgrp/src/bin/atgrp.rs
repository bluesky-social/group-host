//! `atgrp`: see [`atgrp::USAGE`].

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let env = atgrp::Env::from(|k| std::env::var(k).ok());
    match atgrp::run(&args, &env).await {
        Ok(out) => print!("{out}"),
        Err(e) => {
            eprintln!("{e:#}");
            std::process::exit(1);
        }
    }
}
