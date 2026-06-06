#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use clap::Parser;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(long, help = "Do not use /tmp for the scan database, even on platforms where it is the default")]
    no_tmp_db: bool,
    #[arg(long, value_name = "DIR", help = "Directory where completed scan databases should be stored")]
    db_dir: Option<String>,
}

fn default_use_tmp_db() -> bool {
    cfg!(target_os = "linux")
}

fn main() {
    let args = Args::parse();
    let env_force_tmp = std::env::var("DUPED_TMP_DB").is_ok();
    let env_disable_tmp = std::env::var("DUPED_NO_TMP_DB").is_ok();
    let storage_dir = args
        .db_dir
        .or_else(|| std::env::var("DUPED_DB_DIR").ok());

    let use_tmp_db = if args.no_tmp_db || env_disable_tmp {
        false
    } else if env_force_tmp {
        true
    } else {
        default_use_tmp_db()
    };

    duped_lib::run(
        use_tmp_db,
        storage_dir.map(std::path::PathBuf::from),
    )
}
