use clap::Parser;
use anyhow::Result;
use tokio::signal;

mod config;
mod persistence;
mod server;
mod engine;
mod types;
mod expire;

use config::load;
use persistence::Persistence;
use sled::Db;
use std::path::PathBuf;

/// Rudis 启动参数
#[derive(Parser, Debug)]
#[command(author, version, about="Rudis server with AOF+RDB", long_about = None)]
struct Args {
    /// 监听地址 (host:port)
    #[arg(short, long, default_value = "127.0.0.1:6380")]
    listen: String,

    /// JSON 配置文件路径
    #[arg(short, long, default_value = "config.json")]
    config: PathBuf,

    /// sled 数据库目录
    #[arg(short = 'd', long, default_value = "kv.db")]
    db_path: PathBuf,

    /// AOF 日志文件路径
    #[arg(long, default_value = "appendonly.aof")]
    aof_path: PathBuf,

    /// RDB 快照文件路径
    #[arg(long, default_value = "dump.rdb")]
    rdb_path: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    // 1. 解析命令行参数
    let args = Args::parse();
    println!("Starting Rudis with args: {:?}", args);

    // 2. 读取 JSON 配置
    let cfg = load(&args.config)?;
    println!("Loaded config: {:?}", cfg);

    // 3. 打开 sled
    let db: Db = sled::open(&args.db_path)?;

    // 4. 构造持久化器 (支持自定义路径)
    let pers = Persistence::new_with_paths(
        cfg.clone(),
        db.clone(),
        args.aof_path.clone(),
        args.rdb_path.clone(),
    )?;

    // 5. 启动前重放 AOF
    pers.load_aof()?;

    // 6. 启动网络服务
    let serve_handle = {
        let db = db.clone();
        let pers = pers.clone();
        let addr = args.listen.clone();
        tokio::spawn(async move {
            server::start_with_addr_db_and_pers(&addr, db, pers)
                .await
                .unwrap();
        })
    };

    // 7. 等 CTRL-C 优雅退出
    signal::ctrl_c().await?;
    println!("Shutting down…");
    serve_handle.abort();
    pers.fsync_and_close();
    Ok(())
}