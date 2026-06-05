#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use clap::Parser;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(long, help = "Use /tmp for database during scanning to reduce disk writes")]
    tmp_db: bool,
}

fn main() {
    let args = Args::parse();
    let use_tmp_db = args.tmp_db || std::env::var("DUPED_TMP_DB").is_ok();
    duped_lib::run(use_tmp_db)
}
