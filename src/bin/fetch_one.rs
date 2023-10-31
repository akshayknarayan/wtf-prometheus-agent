use color_eyre::eyre::{eyre, Context, Report};
use wtf_prometheus_agent::ElementHealth;

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
        get_config_file_arg().ok_or_else(|| eyre!("Usage: fetch_one -c <config_file.toml>"))?;
    let cfg = wtf_prometheus_agent::parse_config(cfg_file)?;
    let mut elements: Vec<ElementHealth> = cfg
        .elements
        .into_iter()
        .map(|e| e.try_into())
        .collect::<Result<_, _>>()
        .wrap_err("Could not create ElementHealth checkers from config file")?;

    rt.block_on(async move {
        for el in &mut elements {
            let triggered_samples = el.check().await?;
            dbg!(triggered_samples);
        }
        Ok::<_, Report>(())
    })?;
    Ok(())
}
