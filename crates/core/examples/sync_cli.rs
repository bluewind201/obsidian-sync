//! 명령줄 동기화 도구 (개발/테스트용)
//! 사용법: cargo run -p obsidian-sync-core --example sync_cli -- <config.json> <data_dir>
fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: sync_cli <config.json> <data_dir>");
        std::process::exit(2);
    }
    let cfg = obsidian_sync_core::AppConfig::load(std::path::Path::new(&args[1]))?;
    let report =
        obsidian_sync_core::service::sync_once(cfg, std::path::Path::new(&args[2]), &|l| {
            println!("{l}")
        })?;
    println!("{}", report.summary());
    for e in &report.errors {
        eprintln!("ERR {e}");
    }
    Ok(())
}
