use color_eyre::eyre::{eyre, Report, WrapErr};
use wtf_prometheus_agent::AlertChecker;

fn get_config_file_arg() -> Option<String> {
    let mut args = std::env::args().take(3).skip(1);
    let flag = args.next()?;
    if flag != "-c" {
        return None;
    }
    args.next()
}

fn main() -> Result<(), Report> {
    color_eyre::install()?;
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();

    let cfg_file =
        get_config_file_arg().ok_or_else(|| eyre!("Usage: alerts -c <config_file.toml>"))?;
    let cfg = wtf_prometheus_agent::parse_config(cfg_file)?;
    let mut alert_checker: AlertChecker = cfg.prometheus.try_into()?;
    rt.block_on(async move {
        let alerts = alert_checker.check().await.wrap_err("query alerts")?;
        println!("{:?}", alerts);
        Ok::<_, Report>(())
    })?;
    Ok(())
}
